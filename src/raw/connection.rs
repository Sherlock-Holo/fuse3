#[cfg(all(not(feature = "tokio-runtime"), feature = "async-std-runtime"))]
pub use async_std_connection::FuseConnection;
#[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
pub use tokio_connection::FuseConnection;

#[cfg(feature = "tokio-runtime")]
mod tokio_connection {
    use std::io;
    use std::io::ErrorKind;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use std::os::fd::FromRawFd;
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
    use std::os::unix::io::AsRawFd;
    use std::os::unix::io::RawFd;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use std::{ffi::OsString, io::IoSliceMut, path::Path};

    use futures_util::lock::Mutex;
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
    }

    impl FuseConnection {
        pub async fn new() -> io::Result<Self> {
            const DEV_FUSE: &str = "/dev/fuse";

            match tokio::fs::OpenOptions::new()
                .write(true)
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(DEV_FUSE)
                .await
            {
                Err(e) => {
                    if e.kind() == ErrorKind::NotFound {
                        warn!("Cannot open /dev/fuse.  Is the module loaded?");
                    }
                    Err(e)
                }
                Ok(handle) => {
                    let fd = OwnedFd::from(handle.into_std().await);
                    Ok(Self {
                        fd: AsyncFd::new(fd)?,
                        read: Mutex::new(()),
                        write: Mutex::new(()),
                    })
                }
            }
        }

        #[cfg(all(target_os = "linux", feature = "unprivileged"))]
        pub async fn new_with_unprivileged(
            mount_options: MountOptions,
            mount_path: impl AsRef<Path>,
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
            })
        }

        #[cfg(all(target_os = "linux", feature = "unprivileged"))]
        pub fn set_fd_non_blocking(fd: RawFd) -> io::Result<()> {
            let flags = nix::fcntl::fcntl(fd, FcntlArg::F_GETFL).map_err(io::Error::from)?;

            let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;

            nix::fcntl::fcntl(fd, FcntlArg::F_SETFL(flags)).map_err(io::Error::from)?;

            Ok(())
        }

        #[allow(clippy::needless_pass_by_ref_mut)] // Clippy false alarm
        pub async fn read(&self, buf: &mut [u8]) -> Result<usize, io::Error> {
            let _guard = self.read.lock().await;

            loop {
                let mut read_guard = self.fd.readable().await?;
                if let Ok(result) = read_guard
                    .try_io(|fd| unistd::read(fd.as_raw_fd(), buf).map_err(io::Error::from))
                {
                    return result;
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

#[cfg(feature = "async-std-runtime")]
mod async_std_connection {
    use std::io;
    use std::os::fd::{AsFd, BorrowedFd, FromRawFd, OwnedFd};
    use std::os::unix::io::AsRawFd;
    use std::os::unix::io::IntoRawFd;
    use std::os::unix::io::RawFd;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use std::{ffi::OsString, io::IoSliceMut, path::Path};

    use async_io::Async;
    use async_std::fs;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use async_std::process::Command;
    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    use async_std::task;
    use futures_util::lock::Mutex;
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
    }

    impl FuseConnection {
        pub async fn new() -> io::Result<Self> {
            const DEV_FUSE: &str = "/dev/fuse";

            let file = fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open(DEV_FUSE)
                .await?;

            // Safety: fd is valid
            let fd = unsafe { OwnedFd::from_raw_fd(file.into_raw_fd()) };

            Ok(Self {
                fd: Async::new(fd)?,
                read: Mutex::new(()),
                write: Mutex::new(()),
            })
        }

        #[cfg(all(target_os = "linux", feature = "unprivileged"))]
        pub async fn new_with_unprivileged(
            mount_options: MountOptions,
            mount_path: impl AsRef<Path>,
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
            })
        }

        pub async fn read(&self, buf: &mut [u8]) -> Result<usize, io::Error> {
            let _guard = self.read.lock().await;

            self.fd
                .read_with(|fd| unistd::read(fd.as_raw_fd(), buf).map_err(Into::into))
                .await
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
