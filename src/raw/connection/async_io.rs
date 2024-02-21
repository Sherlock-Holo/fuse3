use std::fs::{File, OpenOptions};
use std::io;
#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use std::os::fd::FromRawFd;
#[cfg(any(
    all(target_os = "linux", feature = "unprivileged"),
    target_os = "freebsd"
))]
use std::os::fd::OwnedFd;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::io::AsRawFd;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use std::os::unix::io::RawFd;
use std::pin::pin;
use std::sync::Arc;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use std::{ffi::OsString, io::IoSliceMut, path::Path};

#[cfg(any(
    all(target_os = "linux", feature = "unprivileged"),
    target_os = "freebsd"
))]
use async_io::Async;
use async_lock::Mutex;
use async_notify::Notify;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use async_process::Command;
use futures_util::{select, FutureExt, TryFutureExt};
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use nix::sys::socket::{self, AddressFamily, ControlMessageOwned, MsgFlags, SockFlag, SockType};
use nix::unistd;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use tracing::debug;
#[cfg(target_os = "freebsd")]
use tracing::warn;

#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use crate::find_fusermount3;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use crate::MountOptions;

#[derive(Debug)]
pub struct FuseConnection {
    unmount_notify: Arc<Notify>,
    mode: ConnectionMode,
}

impl FuseConnection {
    pub fn new(unmount_notify: Arc<Notify>) -> io::Result<Self> {
        #[cfg(target_os = "freebsd")]
        {
            let connection = NonBlockFuseConnection::new(unmount_notify)?;

            Ok(Self {
                unmount_notify,
                mode: ConnectionMode::NonBlock(connection),
            })
        }

        #[cfg(target_os = "linux")]
        {
            let connection = BlockFuseConnection::new()?;

            Ok(Self {
                unmount_notify,
                mode: ConnectionMode::Block(connection),
            })
        }
    }

    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    pub async fn new_with_unprivileged(
        mount_options: MountOptions,
        mount_path: impl AsRef<Path>,
        unmount_notify: Arc<Notify>,
    ) -> io::Result<Self> {
        let connection =
            NonBlockFuseConnection::new_with_unprivileged(mount_options, mount_path).await?;

        Ok(Self {
            unmount_notify,
            mode: ConnectionMode::NonBlock(connection),
        })
    }

    pub async fn read(&self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        let mut unmount_fut = pin!(self.unmount_notify.notified().fuse());
        let mut read_fut = pin!(self.inner_read(buf).map_ok(Some).fuse());

        select! {
            _ = unmount_fut => Ok(None),
            res = read_fut => res
        }
    }

    async fn inner_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        match &self.mode {
            #[cfg(target_os = "linux")]
            ConnectionMode::Block(connection) => connection.read(buf).await,
            #[cfg(any(
                all(target_os = "linux", feature = "unprivileged"),
                target_os = "freebsd"
            ))]
            ConnectionMode::NonBlock(connection) => connection.read(buf).await,
        }
    }

    pub async fn write(&self, buf: &[u8]) -> Result<usize, io::Error> {
        match &self.mode {
            #[cfg(target_os = "linux")]
            ConnectionMode::Block(connection) => connection.write(buf).await,
            #[cfg(any(
                all(target_os = "linux", feature = "unprivileged"),
                target_os = "freebsd"
            ))]
            ConnectionMode::NonBlock(connection) => connection.write(buf).await,
        }
    }
}

#[derive(Debug)]
enum ConnectionMode {
    #[cfg(target_os = "linux")]
    Block(BlockFuseConnection),
    #[cfg(any(
        all(target_os = "linux", feature = "unprivileged"),
        target_os = "freebsd"
    ))]
    NonBlock(NonBlockFuseConnection),
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct BlockFuseConnection {
    file: File,
    read: Mutex<Option<Vec<u8>>>,
    write: Mutex<()>,
}

#[cfg(target_os = "linux")]
impl BlockFuseConnection {
    pub fn new() -> io::Result<Self> {
        const DEV_FUSE: &str = "/dev/fuse";

        let file = OpenOptions::new().write(true).read(true).open(DEV_FUSE)?;

        Ok(Self {
            file,
            read: Mutex::new(Some(vec![0; 4096])),
            write: Mutex::new(()),
        })
    }

    async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut inner_buf_guard = self.read.lock().await;
        let mut inner_buf = inner_buf_guard.take().expect("read inner buf should exist");
        if inner_buf.len() < buf.len() {
            inner_buf.resize(buf.len(), 0);
        }
        let fd = self.file.as_raw_fd();

        let (inner_buf, res) = async_global_executor::spawn_blocking(move || {
            let res = unistd::read(fd, &mut inner_buf).map_err(io::Error::from);

            (inner_buf, res)
        })
        .await;

        match res {
            Err(err) => {
                inner_buf_guard.replace(inner_buf);

                Err(err)
            }

            Ok(n) => {
                buf[..n].copy_from_slice(&inner_buf[..n]);
                inner_buf_guard.replace(inner_buf);

                Ok(n)
            }
        }
    }

    async fn write(&self, buf: &[u8]) -> Result<usize, io::Error> {
        let _guard = self.write.lock().await;

        (&self.file).write(buf)
    }
}

#[cfg(any(
    all(target_os = "linux", feature = "unprivileged"),
    target_os = "freebsd"
))]
#[derive(Debug)]
struct NonBlockFuseConnection {
    fd: Async<OwnedFd>,
    read: Mutex<()>,
    write: Mutex<()>,
}

#[cfg(any(
    all(target_os = "linux", feature = "unprivileged"),
    target_os = "freebsd"
))]
impl NonBlockFuseConnection {
    #[cfg(target_os = "freebsd")]
    fn new() -> io::Result<Self> {
        const DEV_FUSE: &str = "/dev/fuse";

        let file = fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(DEV_FUSE)?;

        Ok(Self {
            fd: Async::new(file.into())?,
            read: Mutex::new(()),
            write: Mutex::new(()),
        })
    }

    #[cfg(all(target_os = "linux", feature = "unprivileged"))]
    async fn new_with_unprivileged(
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
        })
    }

    async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let _guard = self.read.lock().await;

        self.fd
            .read_with(|fd| unistd::read(fd.as_raw_fd(), buf).map_err(Into::into))
            .await
    }

    async fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let _guard = self.write.lock().await;

        unistd::write(self.fd.as_raw_fd(), buf).map_err(Into::into)
    }
}

impl AsFd for FuseConnection {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match &self.mode {
            #[cfg(target_os = "linux")]
            ConnectionMode::Block(connection) => {
                // Safety: we own the File
                connection.file.as_fd()
            }

            #[cfg(any(
                all(target_os = "linux", feature = "unprivileged"),
                target_os = "freebsd"
            ))]
            ConnectionMode::NonBlock(connection) => connection.fd.as_fd(),
        }
    }
}
