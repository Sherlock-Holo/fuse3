#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use std::fs::File as SysFile;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use std::io::{self, prelude::*};
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use std::os::unix::io::AsRawFd;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use std::os::unix::io::FromRawFd;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use std::os::unix::io::IntoRawFd;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use std::os::unix::io::RawFd;

#[cfg(feature = "async-std-runtime")]
use async_std::sync::Mutex;
#[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
use tokio::sync::Mutex;

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use crate::spawn::spawn_blocking;

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
const DEV_FUSE: &str = "/dev/fuse";

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
pub struct FuseConnection {
    fd: RawFd,
    read_file: Mutex<Option<SysFile>>,
    write_file: Mutex<Option<SysFile>>,
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl FuseConnection {
    pub async fn new() -> io::Result<Self> {
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
            read_file: Mutex::new(Some(unsafe { SysFile::from_raw_fd(fd) })),
            write_file: Mutex::new(Some(unsafe { SysFile::from_raw_fd(fd) })),
        })
    }

    pub async fn read(&self, mut buf: Vec<u8>) -> Result<(Vec<u8>, usize), (Vec<u8>, io::Error)> {
        let mut guard = self.read_file.lock().await;

        let mut file = guard.take().unwrap();

        match spawn_blocking(move || match file.read(&mut buf) {
            Ok(n) => Ok((file, buf, n)),
            Err(err) => Err((file, buf, err)),
        })
        .await
        {
            Ok((file, buf, n)) => {
                guard.replace(file);

                Ok((buf, n))
            }

            Err((file, buf, err)) => {
                guard.replace(file);

                Err((buf, err))
            }
        }
    }

    pub async fn write(
        &self,
        buf: Vec<u8>,
        n: usize,
    ) -> Result<(Vec<u8>, usize), (Vec<u8>, io::Error)> {
        let mut guard = self.write_file.lock().await;

        let mut file = guard.take().unwrap();

        match spawn_blocking(move || match file.write(&buf[..n]) {
            Ok(n) => Ok((file, buf, n)),
            Err(err) => Err((file, buf, err)),
        })
        .await
        {
            Ok((file, buf, n)) => {
                guard.replace(file);

                Ok((buf, n))
            }

            Err((file, buf, err)) => {
                guard.replace(file);

                Err((buf, err))
            }
        }
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
        let read_file = self
            .read_file
            .try_lock()
            .expect("with &mut self, we must have lock")
            .take()
            .unwrap();

        // make sure fd won't be close twice
        read_file.into_raw_fd();
    }
}
