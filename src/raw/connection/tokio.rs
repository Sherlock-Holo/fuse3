use std::fs::{File, OpenOptions};
use std::io;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use std::io::ErrorKind;
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
#[cfg(target_os = "freebsd")]
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use std::os::unix::io::RawFd;
use std::pin::pin;
use std::sync::Arc;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use std::{ffi::OsString, io::IoSliceMut, path::Path};

use async_notify::Notify;
use futures_util::lock::Mutex;
use futures_util::{select, FutureExt, TryFutureExt};
use nix::unistd;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use nix::{
    fcntl::{FcntlArg, OFlag},
    sys::socket::{self, AddressFamily, ControlMessageOwned, MsgFlags, SockFlag, SockType},
};
#[cfg(any(
    all(target_os = "linux", feature = "unprivileged"),
    target_os = "freebsd"
))]
use tokio::io::unix::AsyncFd;
#[cfg(all(target_os = "linux", feature = "unprivileged"))]
use tokio::process::Command;
#[cfg(target_os = "linux")]
use tokio::task;
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
            let connection = NonBlockFuseConnection::new()?;

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

    pub async fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, io::Error> {
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

    pub async fn write(&self, buf: &[u8]) -> io::Result<usize> {
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

#[derive(Debug)]
struct BlockFuseConnection {
    file: File,
    read: Mutex<Option<Vec<u8>>>,
    write: Mutex<()>,
}

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

        let (inner_buf, res) = task::spawn_blocking(move || {
            let res = unistd::read(fd, &mut inner_buf).map_err(io::Error::from);

            (inner_buf, res)
        })
        .await
        .unwrap();

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

    async fn write(&self, buf: &[u8]) -> io::Result<usize> {
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
    fd: AsyncFd<OwnedFd>,
    read: Mutex<()>,
    write: Mutex<()>,
}

#[cfg(any(
    all(target_os = "linux", feature = "unprivileged"),
    target_os = "freebsd"
))]
impl NonBlockFuseConnection {
    #[cfg(target_os = "freebsd")]
    async fn new() -> io::Result<Self> {
        const DEV_FUSE: &str = "/dev/fuse";

        match OpenOptions::new()
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
            }),
        }
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
    fn set_fd_non_blocking(fd: RawFd) -> io::Result<()> {
        let flags = nix::fcntl::fcntl(fd, FcntlArg::F_GETFL).map_err(io::Error::from)?;

        let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;

        nix::fcntl::fcntl(fd, FcntlArg::F_SETFL(flags)).map_err(io::Error::from)?;

        Ok(())
    }

    #[allow(clippy::needless_pass_by_ref_mut)] // Clippy false alarm
    pub async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let _guard = self.read.lock().await;

        loop {
            let mut read_guard = self.fd.readable().await?;

            if let Ok(result) =
                read_guard.try_io(|fd| unistd::read(fd.as_raw_fd(), buf).map_err(io::Error::from))
            {
                return result;
            } else {
                continue;
            }
        }
    }

    pub async fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let _guard = self.write.lock().await;
        let fd = self.fd.as_raw_fd();

        unistd::write(fd.as_raw_fd(), buf).map_err(Into::into)
    }
}

impl AsFd for FuseConnection {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match &self.mode {
            #[cfg(target_os = "linux")]
            ConnectionMode::Block(connection) => connection.file.as_fd(),

            #[cfg(any(
                all(target_os = "linux", feature = "unprivileged"),
                target_os = "freebsd"
            ))]
            ConnectionMode::NonBlock(connection) => connection.fd.as_fd(),
        }
    }
}
