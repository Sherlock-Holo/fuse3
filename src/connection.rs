#[cfg(feature = "unprivileged")]
use std::ffi::OsString;
use std::io;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::IntoRawFd;
use std::os::unix::io::RawFd;
#[cfg(feature = "unprivileged")]
use std::path::Path;
#[cfg(feature = "unprivileged")]
use std::process::Command;

#[cfg(feature = "async-std-runtime")]
use async_std::sync::Mutex;
#[cfg(feature = "unprivileged")]
use log::debug;
#[cfg(feature = "unprivileged")]
use nix::sys::socket;
#[cfg(feature = "unprivileged")]
use nix::sys::socket::{AddressFamily, ControlMessageOwned, MsgFlags, SockFlag, SockType};
#[cfg(feature = "unprivileged")]
use nix::sys::uio::IoVec;
use nix::unistd;
#[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
use tokio::sync::Mutex;

use crate::helper::io_error_from_nix_error;
use crate::spawn::spawn_blocking;
#[cfg(feature = "unprivileged")]
use crate::MountOptions;

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
pub struct FuseConnection {
    fd: RawFd,
    read: Mutex<()>,
    write: Mutex<()>,
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl FuseConnection {
    pub async fn new() -> io::Result<Self> {
        const DEV_FUSE: &str = "/dev/fuse";

        #[cfg(feature = "async-std-runtime")]
        let fd = async_std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(DEV_FUSE)
            .await?
            .into_raw_fd();

        #[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
        let fd = tokio::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(DEV_FUSE)
            .await?
            .into_std()
            .await
            .into_raw_fd();

        Ok(Self {
            fd,
            read: Mutex::new(()),
            write: Mutex::new(()),
        })
    }

    #[cfg(feature = "unprivileged")]
    pub async fn new_with_unprivileged(
        mount_options: MountOptions,
        mount_path: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let (fd0, fd1) = match socket::socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::empty(),
        ) {
            Err(err) => return Err(io_error_from_nix_error(err)),

            Ok((fd0, fd1)) => (fd0, fd1),
        };

        let binary_path = match which::which("fusermount") {
            Err(err) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("find fusermount binary failed {}", err),
                ));
            }
            Ok(path) => path,
        };

        const ENV: &str = "_FUSE_COMMFD";

        let options = mount_options.build_with_unprivileged();

        debug!("mount options {:?}", options);

        let mount_path = mount_path.as_ref().as_os_str().to_os_string();

        let mut child = spawn_blocking(move || {
            Command::new(binary_path)
                .env(ENV, fd0.to_string())
                .args(vec![OsString::from("-o"), options, mount_path])
                .spawn()
        })
        .await?;

        if !child.wait()?.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "fusermount run failed",
            ));
        }

        let fd = spawn_blocking(move || {
            let mut buf = vec![0; 10000]; // buf should large enough

            let mut cmsg_buf = nix::cmsg_space!([RawFd; 2]);

            let bufs = [IoVec::from_mut_slice(&mut buf)];

            let msg = match socket::recvmsg(fd1, &bufs, Some(&mut cmsg_buf), MsgFlags::empty()) {
                Err(err) => return Err(io_error_from_nix_error(err)),

                Ok(msg) => msg,
            };

            let fd = if let Some(ControlMessageOwned::ScmRights(fds)) = msg.cmsgs().next() {
                if fds.len() < 1 {
                    return Err(io::Error::new(io::ErrorKind::Other, "no fuse fd"));
                }

                fds[0]
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "get fuse fd failed"));
            };

            Ok(fd)
        })
        .await?;

        if let Err(err) = unistd::close(fd0) {
            return Err(io_error_from_nix_error(err));
        }

        if let Err(err) = unistd::close(fd1) {
            return Err(io_error_from_nix_error(err));
        }

        Ok(Self {
            fd,
            read: Mutex::new(()),
            write: Mutex::new(()),
        })
    }

    pub async fn read(&self, mut buf: Vec<u8>) -> Result<(Vec<u8>, usize), (Vec<u8>, io::Error)> {
        let _guard = self.read.lock().await;

        let fd = self.fd;

        spawn_blocking(move || match unistd::read(fd, &mut buf) {
            Err(err) => Err((buf, io_error_from_nix_error(err))),
            Ok(n) => Ok((buf, n)),
        })
        .await
    }

    pub async fn write(
        &self,
        buf: Vec<u8>,
        n: usize,
    ) -> Result<(Vec<u8>, usize), (Vec<u8>, io::Error)> {
        let _guard = self.write.lock().await;

        let fd = self.fd;

        spawn_blocking(move || match unistd::write(fd, &buf[..n]) {
            Err(err) => Err((buf, io_error_from_nix_error(err))),
            Ok(n) => Ok((buf, n)),
        })
        .await
    }
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl AsRawFd for FuseConnection {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl Drop for FuseConnection {
    fn drop(&mut self) {
        let _ = unistd::close(self.fd);
    }
}
