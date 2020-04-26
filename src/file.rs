#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use std::fs::File as SysFile;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use std::io::{prelude::*, Result};
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
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use crossbeam_utils::thread;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use futures::channel::oneshot;
#[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
use tokio::sync::Mutex;

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
const DEV_FUSE: &str = "/dev/fuse";

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
pub struct File {
    fd: RawFd,
    read_file: Mutex<Option<SysFile>>,
    write_file: Mutex<Option<SysFile>>,
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl File {
    pub async fn new() -> Result<Self> {
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

    pub async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let mut guard = self.read_file.lock().await;

        let file = guard.as_mut().unwrap();

        let (sender, receiver) = oneshot::channel();

        // it should not happened
        let _result = thread::scope(|s| {
            s.spawn(|_| {
                let result = file.read(buf);

                sender
                    .send(result)
                    .expect("receiver shouldn't drop before send");
            });
        });

        receiver.await.expect("sender won't drop before send")
    }

    pub async fn write(&self, buf: &[u8]) -> Result<usize> {
        let mut guard = self.write_file.lock().await;

        let file = guard.as_mut().unwrap();

        let (sender, receiver) = oneshot::channel();

        // it should not happened
        let _result = thread::scope(|s| {
            s.spawn(|_| {
                let result = file.write(buf);

                sender
                    .send(result)
                    .expect("receiver shouldn't drop before send");
            });
        });

        receiver.await.expect("sender shouldn't drop before send")
    }
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl Drop for File {
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
