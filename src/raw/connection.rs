#[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
pub use async_io_connection::FuseConnection;
#[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
pub use tokio_connection::FuseConnection;

#[cfg(feature = "tokio-runtime")]
mod tokio_connection {
    use std::io::ErrorKind;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use std::os::fd::FromRawFd;
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    use std::os::unix::io::RawFd;
    use std::pin::pin;
    use std::sync::Arc;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use std::{ffi::OsString, io::IoSliceMut, path::Path};
    use std::{fs, io};

    use async_notify::Notify;
    use futures_util::lock::Mutex;
    use futures_util::{select, FutureExt};
    use nix::unistd;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use nix::{
        fcntl::{FcntlArg, OFlag},
        sys::socket::{self, AddressFamily, ControlMessageOwned, MsgFlags, SockFlag, SockType},
    };
    use tokio::io::unix::AsyncFd;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use tokio::process::Command;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use tokio::task;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use tracing::debug;
    use tracing::warn;

    #[cfg(target_os = "linux")]
    use crate::find_fusermount3;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use crate::MountOptions;

    #[derive(Debug)]
    pub struct FuseConnection {
        fd: AsyncFd<OwnedFd>,
        read: Mutex<()>,
        write: Mutex<()>,
        unmount_notify: Arc<Notify>,
    }

    impl FuseConnection {
        pub fn new(unmount_notify: Arc<Notify>) -> io::Result<Self> {
            const DEV_FUSE: &str = "/dev/fuse";

            match fs::OpenOptions::new()
                .write(true)
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(DEV_FUSE)
            {
                Err(e) => {
                    if e.kind() == ErrorKind::NotFound {
                        warn!("Cannot open /dev/fuse.  Is the module loaded?");
                    }
                    Err(e)
                }
                Ok(file) => Ok(Self {
                    fd: AsyncFd::new(file.into())?,
                    read: Mutex::new(()),
                    write: Mutex::new(()),
                    unmount_notify,
                }),
            }
        }

        #[cfg(all(target_os = "linux", feature = "unprivileged"))]
        pub async fn new_with_unprivileged(
            mount_options: MountOptions,
            mount_path: impl AsRef<Path>,
            unmount_notify: Arc<Notify>,
        ) -> io::Result<Self> {
            let (sock0, sock1) = match socket::socketpair(
                AddressFamily::Unix,
                SockType::SeqPacket,
                None,
                SockFlag::empty(),
            ) {
                Err(err) => return Err(err.into()),

                Ok((sock0, sock1)) => (sock0, sock1),
            };

            let binary_path = find_fusermount3()?;

            const ENV: &str = "_FUSE_COMMFD";

            let options = mount_options.build_with_unprivileged();

            debug!("mount options {:?}", options);

            let mount_path = mount_path.as_ref().as_os_str().to_os_string();

            let fd0 = sock0.as_raw_fd();
            let mut child = Command::new(binary_path)
                .env(ENV, fd0.to_string())
                .args(vec![OsString::from("-o"), options, mount_path])
                .spawn()?;

            if !child.wait().await?.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "fusermount run failed",
                ));
            }

            let fd1 = sock1.as_raw_fd();
            let fd = task::spawn_blocking(move || {
                // let mut buf = vec![0; 10000]; // buf should large enough
                let mut buf = vec![]; // it seems 0 len still works well

                let mut cmsg_buf = nix::cmsg_space!([RawFd; 1]);

                let mut bufs = [IoSliceMut::new(&mut buf)];

                let msg = match socket::recvmsg::<()>(
                    fd1,
                    &mut bufs[..],
                    Some(&mut cmsg_buf),
                    MsgFlags::empty(),
                ) {
                    Err(err) => return Err(err.into()),

                    Ok(msg) => msg,
                };

                let fd = if let Some(ControlMessageOwned::ScmRights(fds)) = msg.cmsgs().next() {
                    if fds.is_empty() {
                        return Err(io::Error::new(ErrorKind::Other, "no fuse fd"));
                    }

                    fds[0]
                } else {
                    return Err(io::Error::new(ErrorKind::Other, "get fuse fd failed"));
                };

                Ok(fd)
            })
            .await
            .unwrap()?;

            Self::set_fd_non_blocking(fd)?;

            // Safety: fd is valid
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };

            Ok(Self {
                fd: AsyncFd::new(fd)?,
                read: Mutex::new(()),
                write: Mutex::new(()),
                unmount_notify,
            })
        }

        #[cfg(all(target_os = "linux", feature = "unprivileged"))]
        fn set_fd_non_blocking(fd: RawFd) -> io::Result<()> {
            let flags = nix::fcntl::fcntl(fd, FcntlArg::F_GETFL).map_err(io::Error::from)?;

            let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;

            nix::fcntl::fcntl(fd, FcntlArg::F_SETFL(flags)).map_err(io::Error::from)?;

            Ok(())
        }

        #[allow(clippy::needless_pass_by_ref_mut)] // Clippy false alarm
        pub async fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, io::Error> {
            let _guard = self.read.lock().await;

            loop {
                let mut unmount_fut = pin!(self.unmount_notify.notified().fuse());
                let mut readable_fut = pin!(self.fd.readable().fuse());

                let mut read_guard = select! {
                    _ = unmount_fut => return Ok(None),
                    res = readable_fut => res?,
                };

                if let Ok(result) = read_guard
                    .try_io(|fd| unistd::read(fd.as_raw_fd(), buf).map_err(io::Error::from))
                {
                    return result.map(Some);
                } else {
                    continue;
                }
            }
        }

        pub async fn write(&self, buf: &[u8]) -> Result<usize, io::Error> {
            let _guard = self.write.lock().await;
            let fd = self.fd.as_raw_fd();

            unistd::write(fd.as_raw_fd(), buf).map_err(Into::into)
        }
    }

    impl AsFd for FuseConnection {
        fn as_fd(&self) -> BorrowedFd<'_> {
            self.fd.as_fd()
        }
    }

    impl AsRawFd for FuseConnection {
        fn as_raw_fd(&self) -> RawFd {
            self.fd.as_raw_fd()
        }
    }
}

#[cfg(feature = "async-io-runtime")]
mod async_io_connection {
    use std::os::fd::{AsFd, BorrowedFd, FromRawFd, OwnedFd};
    use std::os::unix::io::AsRawFd;
    use std::os::unix::io::RawFd;
    use std::pin::pin;
    use std::sync::Arc;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use std::{ffi::OsString, io::IoSliceMut, path::Path};
    use std::{fs, io};

    use async_io::Async;
    use async_notify::Notify;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use async_process::Command;
    use futures_util::lock::Mutex;
    use futures_util::{select, FutureExt};
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use nix::sys::socket::{
        self, AddressFamily, ControlMessageOwned, MsgFlags, SockFlag, SockType,
    };
    use nix::unistd;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use tracing::debug;

    #[cfg(target_os = "linux")]
    use crate::find_fusermount3;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use crate::MountOptions;

    #[derive(Debug)]
    pub struct FuseConnection {
        fd: Async<OwnedFd>,
        read: Mutex<()>,
        write: Mutex<()>,
        unmount_notify: Arc<Notify>,
    }

    impl FuseConnection {
        pub fn new(unmount_notify: Arc<Notify>) -> io::Result<Self> {
            const DEV_FUSE: &str = "/dev/fuse";

            let file = fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open(DEV_FUSE)?;

            Ok(Self {
                fd: Async::new(file.into())?,
                read: Mutex::new(()),
                write: Mutex::new(()),
                unmount_notify,
            })
        }

        #[cfg(all(target_os = "linux", feature = "unprivileged"))]
        pub async fn new_with_unprivileged(
            mount_options: MountOptions,
            mount_path: impl AsRef<Path>,
            unmount_notify: Arc<Notify>,
        ) -> io::Result<Self> {
            let (sock0, sock1) = match socket::socketpair(
                AddressFamily::Unix,
                SockType::SeqPacket,
                None,
                SockFlag::empty(),
            ) {
                Err(err) => return Err(err.into()),

                Ok((sock0, sock1)) => (sock0, sock1),
            };

            let binary_path = find_fusermount3()?;

            const ENV: &str = "_FUSE_COMMFD";

            let options = mount_options.build_with_unprivileged();

            debug!("mount options {:?}", options);

            let mount_path = mount_path.as_ref().as_os_str().to_os_string();

            let fd0 = sock0.as_raw_fd();
            let mut child = Command::new(binary_path)
                .env(ENV, fd0.to_string())
                .args(vec![OsString::from("-o"), options, mount_path])
                .spawn()?;

            if !child.status().await?.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "fusermount run failed",
                ));
            }

            let fd1 = sock1.as_raw_fd();
            let fd = async_global_executor::spawn_blocking(move || {
                // let mut buf = vec![0; 10000]; // buf should large enough
                let mut buf = vec![]; // it seems 0 len still works well

                let mut cmsg_buf = nix::cmsg_space!([RawFd; 1]);

                let mut bufs = [IoSliceMut::new(&mut buf)];

                let msg = match socket::recvmsg::<()>(
                    fd1,
                    &mut bufs[..],
                    Some(&mut cmsg_buf),
                    MsgFlags::empty(),
                ) {
                    Err(err) => return Err(err.into()),

                    Ok(msg) => msg,
                };

                let fd = if let Some(ControlMessageOwned::ScmRights(fds)) = msg.cmsgs().next() {
                    if fds.is_empty() {
                        return Err(io::Error::new(io::ErrorKind::Other, "no fuse fd"));
                    }

                    fds[0]
                } else {
                    return Err(io::Error::new(io::ErrorKind::Other, "get fuse fd failed"));
                };

                Ok(fd)
            })
            .await?;

            // Safety: fd is valid
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };

            Ok(Self {
                fd: Async::new(fd)?,
                read: Mutex::new(()),
                write: Mutex::new(()),
                unmount_notify,
            })
        }

        pub async fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, io::Error> {
            let _guard = self.read.lock().await;

            let mut notify_fut = pin!(self.unmount_notify.notified().fuse());
            let mut read_fut = pin!(self
                .fd
                .read_with(|fd| unistd::read(fd.as_raw_fd(), buf)
                    .map(Some)
                    .map_err(Into::into))
                .fuse());

            select! {
                _ = notify_fut => Ok(None),
                res = read_fut => res
            }
        }

        pub async fn write(&self, buf: &[u8]) -> Result<usize, io::Error> {
            let _guard = self.write.lock().await;

            unistd::write(self.fd.as_raw_fd(), buf).map_err(Into::into)
        }
    }

    impl AsFd for FuseConnection {
        fn as_fd(&self) -> BorrowedFd<'_> {
            self.fd.as_fd()
        }
    }

    impl AsRawFd for FuseConnection {
        fn as_raw_fd(&self) -> RawFd {
            self.fd.as_raw_fd()
        }
    }
}
