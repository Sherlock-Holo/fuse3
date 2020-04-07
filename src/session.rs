use std::convert::TryFrom;
use std::ffi::{CStr, OsString};
use std::io::{IoSlice, IoSliceMut};
use std::os::unix::ffi::OsStringExt;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

#[cfg(feature = "async-std-runtime")]
use async_std::fs::{File, OpenOptions};
#[cfg(feature = "async-std-runtime")]
use async_std::io::prelude::*;
use futures::channel::mpsc::UnboundedSender;
use futures::SinkExt;
use log::{debug, error, warn};
#[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
use tokio::prelude::*;

use crate::abi::{
    fuse_attr, fuse_entry_out, fuse_in_header, fuse_init_in, fuse_init_out, fuse_opcode,
    fuse_out_header, BUFFER_SIZE, DEFAULT_CONGESTION_THRESHOLD, DEFAULT_MAP_ALIGNMENT,
    DEFAULT_MAX_BACKGROUND, DEFAULT_MAX_PAGES, DEFAULT_TIME_GRAN, FUSE_ASYNC_READ,
    FUSE_ATOMIC_O_TRUNC, FUSE_AUTO_INVAL_DATA, FUSE_BIG_WRITES, FUSE_DONT_MASK,
    FUSE_DO_READDIRPLUS, FUSE_EXPORT_SUPPORT, FUSE_FILE_OPS, FUSE_FLOCK_LOCKS, FUSE_HAS_IOCTL_DIR,
    FUSE_IN_HEADER_SIZE, FUSE_KERNEL_MINOR_VERSION, FUSE_KERNEL_VERSION, FUSE_OUT_HEADER_SIZE,
    FUSE_PARALLEL_DIROPS, FUSE_POSIX_ACL, FUSE_POSIX_LOCKS, FUSE_READDIRPLUS_AUTO,
    FUSE_SPLICE_MOVE, FUSE_SPLICE_READ, FUSE_SPLICE_WRITE, FUSE_WRITEBACK_CACHE, MAX_WRITE_SIZE,
};
use crate::apply::Apply;
use crate::filesystem::Filesystem;
use crate::request::Request;
use crate::spawn::spawn;
use crate::Result;

// #[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
// use tokio::fs::{OpenOptions, File};

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
pub struct Session<T> {
    fuse_file: File,
    filesystem: Arc<T>,
    response_sender: UnboundedSender<Vec<u8>>,
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl<T: Filesystem + Send + Sync + 'static> Session<T> {
    pub async fn dispatch(&mut self) -> Result<()> {
        let binary_serialize_cfg = bincode::config().apply(|cfg| {
            cfg.big_endian();
        });

        let mut buffer = vec![0; BUFFER_SIZE];

        loop {
            let n = match self.fuse_file.read(&mut buffer).await {
                Err(err) => {
                    error!("read from /dev/fuse failed {}", err);

                    return if let Some(os_err) = err.raw_os_error() {
                        Err(os_err)
                    } else {
                        Err(libc::EIO)
                    };
                }

                Ok(n) => n,
            };

            let mut data = &buffer[..n];

            let in_header = fuse_in_header::from(data);

            let request = Request::from(&in_header);

            let opcode = match fuse_opcode::try_from(in_header.opcode) {
                Err(err) => {
                    debug!("receive unknown opcode {}", err.0);
                    continue;
                }
                Ok(opcode) => opcode,
            };

            data = &data[FUSE_IN_HEADER_SIZE..];

            match opcode {
                fuse_opcode::FUSE_INIT => {
                    debug!("receive FUSE INIT");

                    let init_in = fuse_init_in::from(data);

                    let mut reply_flags = 0;

                    if init_in.flags & FUSE_ASYNC_READ > 0 {
                        reply_flags |= FUSE_ASYNC_READ;
                    }

                    #[cfg(feature = "file-lock")]
                    if init_in.flags & FUSE_POSIX_LOCKS > 0 {
                        reply_flags |= FUSE_POSIX_LOCKS;
                    }

                    if init_in.flags & FUSE_FILE_OPS > 0 {
                        reply_flags |= FUSE_FILE_OPS;
                    }

                    if init_in.flags & FUSE_ATOMIC_O_TRUNC > 0 {
                        reply_flags |= FUSE_ATOMIC_O_TRUNC;
                    }

                    // TODO should we need it?
                    /*if init_in.flags&FUSE_EXPORT_SUPPORT>0 {
                        reply_flags |= FUSE_EXPORT_SUPPORT;
                    }*/

                    if init_in.flags & FUSE_BIG_WRITES > 0 {
                        reply_flags |= FUSE_BIG_WRITES;
                    }

                    // TODO should we need it?
                    /*if init_in.flags&FUSE_DONT_MASK>0 {
                        reply_flags |= FUSE_DONT_MASK;
                    }*/

                    if init_in.flags & FUSE_SPLICE_WRITE > 0 {
                        reply_flags |= FUSE_SPLICE_WRITE;
                    }

                    if init_in.flags & FUSE_SPLICE_MOVE > 0 {
                        reply_flags |= FUSE_SPLICE_MOVE;
                    }

                    if init_in.flags & FUSE_SPLICE_READ > 0 {
                        reply_flags |= FUSE_SPLICE_READ;
                    }

                    // posix lock used, maybe we don't need bsd lock
                    /*if init_in.flags&FUSE_FLOCK_LOCKS>0 {
                        reply_flags |= FUSE_FLOCK_LOCKS;
                    }*/

                    if init_in.flags & FUSE_HAS_IOCTL_DIR > 0 {
                        reply_flags |= FUSE_HAS_IOCTL_DIR;
                    }

                    if init_in.flags & FUSE_AUTO_INVAL_DATA > 0 {
                        reply_flags |= FUSE_AUTO_INVAL_DATA;
                    }

                    if init_in.flags & FUSE_DO_READDIRPLUS > 0 {
                        reply_flags |= FUSE_DO_READDIRPLUS;
                    }

                    if init_in.flags & FUSE_READDIRPLUS_AUTO > 0 {
                        reply_flags |= FUSE_READDIRPLUS_AUTO;
                    }

                    // TODO should we enable it or add feature?
                    /*if init_in.flags&FUSE_ASYNC_DIO>0 {
                        reply_flags |= FUSE_ASYNC_DIO;
                    }*/

                    if init_in.flags & FUSE_WRITEBACK_CACHE > 0 {
                        reply_flags |= FUSE_WRITEBACK_CACHE;
                    }

                    if init_in.flags & FUSE_PARALLEL_DIROPS > 0 {
                        reply_flags |= FUSE_PARALLEL_DIROPS;
                    }

                    // TODO check if we need to enable it on default
                    /*if init_in.flags&FUSE_HANDLE_KILLPRIV>0 {
                        reply_flags |= FUSE_HANDLE_KILLPRIV;
                    }*/

                    if init_in.flags & FUSE_POSIX_ACL > 0 {
                        reply_flags |= FUSE_POSIX_ACL;
                    }

                    if let Err(err) = self.filesystem.init(request.clone()).await {
                        let init_out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE) as u32,
                            error: err,
                            unique: request.unique,
                        };

                        let init_out_header_data = binary_serialize_cfg
                            .serialize(&init_out_header)
                            .expect("won't happened");

                        if let Err(err) = self.fuse_file.write(&init_out_header_data).await {
                            error!("write init out data to /dev/fuse failed {}", err);
                        }

                        return Err(err);
                    }

                    let init_out = fuse_init_out {
                        major: FUSE_KERNEL_VERSION,
                        minor: FUSE_KERNEL_MINOR_VERSION,
                        max_readahead: init_in.max_readahead,
                        flags: reply_flags,
                        max_background: DEFAULT_MAX_BACKGROUND,
                        congestion_threshold: DEFAULT_CONGESTION_THRESHOLD,
                        max_write: MAX_WRITE_SIZE as u32,
                        time_gran: DEFAULT_TIME_GRAN,
                        max_pages: DEFAULT_MAX_PAGES,
                        map_alignment: DEFAULT_MAP_ALIGNMENT,
                        unused: [0; 8],
                    };

                    let mut init_out_data = binary_serialize_cfg
                        .serialize(&init_out)
                        .expect("won't happened");

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + init_out_data.len()) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut out_header_data = binary_serialize_cfg
                        .serialize(&out_header)
                        .expect("won't happened");

                    /*let init_out_header_data = IoSlice::new(&init_out_header_data);
                    let init_out_data = IoSlice::new(&init_out_data);

                    self.fuse_file.write_vectored(&vec![init_out_header_data, init_out_data]).await*/

                    out_header_data.append(&mut init_out_data);

                    if let Err(err) = self.fuse_file.write(&out_header_data).await {
                        error!("write init out data to /dev/fuse failed {}", err);

                        unimplemented!("handle error")
                    }
                }
                fuse_opcode::FUSE_DESTROY => {
                    self.filesystem.destroy(request).await;

                    return Ok(());
                }
                fuse_opcode::FUSE_LOOKUP => {
                    let mut resp_sender = self.response_sender.clone();
                    let binary_serialize_cfg = binary_serialize_cfg.clone();

                    let name = match CStr::from_bytes_with_nul(
                        &data[..in_header.len as usize - FUSE_IN_HEADER_SIZE],
                    ) {
                        Err(err) => {
                            warn!("receive invalid name in lookup unique {}", request.unique);

                            spawn(async move {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: libc::EINVAL,
                                    unique: request.unique,
                                };

                                let data = binary_serialize_cfg
                                    .serialize(&out_header)
                                    .expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            });

                            continue;
                        }

                        Ok(name) => OsString::from_vec(name.to_bytes().to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn(async move {
                        let data = match fs.lookup(request.clone(), in_header.nodeid, name).await {
                            Err(err) => {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: err,
                                    unique: request.unique,
                                };

                                binary_serialize_cfg
                                    .serialize(&out_header)
                                    .expect("won't happened")
                            }

                            Ok(entry) => {
                                let entry_out: fuse_entry_out = entry.into();
                                let mut entry_out_data = binary_serialize_cfg
                                    .serialize(&entry_out)
                                    .expect("won't happened");

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + entry_out_data.len()) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };
                                let mut out_header_data = binary_serialize_cfg
                                    .serialize(&out_header)
                                    .expect("won't happened");

                                out_header_data.append(&mut entry_out_data);

                                out_header_data
                            }
                        };

                        let _ = resp_sender.send(data).await;
                    });
                }
                fuse_opcode::FUSE_FORGET => {}
                fuse_opcode::FUSE_GETATTR => {}
                fuse_opcode::FUSE_SETATTR => {}
                fuse_opcode::FUSE_READLINK => {}
                fuse_opcode::FUSE_SYMLINK => {}
                fuse_opcode::FUSE_MKNOD => {}
                fuse_opcode::FUSE_MKDIR => {}
                fuse_opcode::FUSE_UNLINK => {}
                fuse_opcode::FUSE_RMDIR => {}
                fuse_opcode::FUSE_RENAME => {}
                fuse_opcode::FUSE_LINK => {}
                fuse_opcode::FUSE_OPEN => {}
                fuse_opcode::FUSE_READ => {}
                fuse_opcode::FUSE_WRITE => {}
                fuse_opcode::FUSE_STATFS => {}
                fuse_opcode::FUSE_RELEASE => {}
                fuse_opcode::FUSE_FSYNC => {}
                fuse_opcode::FUSE_SETXATTR => {}
                fuse_opcode::FUSE_GETXATTR => {}
                fuse_opcode::FUSE_LISTXATTR => {}
                fuse_opcode::FUSE_REMOVEXATTR => {}
                fuse_opcode::FUSE_FLUSH => {}
                fuse_opcode::FUSE_OPENDIR => {}
                fuse_opcode::FUSE_READDIR => {}
                fuse_opcode::FUSE_RELEASEDIR => {}
                fuse_opcode::FUSE_FSYNCDIR => {}
                fuse_opcode::FUSE_GETLK => {}
                fuse_opcode::FUSE_SETLK => {}
                fuse_opcode::FUSE_SETLKW => {}
                fuse_opcode::FUSE_ACCESS => {}
                fuse_opcode::FUSE_CREATE => {}
                fuse_opcode::FUSE_INTERRUPT => {}
                fuse_opcode::FUSE_BMAP => {}
                fuse_opcode::FUSE_IOCTL => {}
                fuse_opcode::FUSE_POLL => {}
                fuse_opcode::FUSE_NOTIFY_REPLY => {}
                fuse_opcode::FUSE_BATCH_FORGET => {}
                fuse_opcode::FUSE_FALLOCATE => {}
                fuse_opcode::FUSE_READDIRPLUS => {}
                fuse_opcode::FUSE_RENAME2 => {}
                fuse_opcode::FUSE_LSEEK => {}
                fuse_opcode::FUSE_COPY_FILE_RANGE => {}

                #[cfg(target_os = "macos")]
                fuse_opcode::FUSE_SETVOLNAME => {}

                #[cfg(target_os = "macos")]
                fuse_opcode::FUSE_GETXTIMES => {}

                #[cfg(target_os = "macos")]
                fuse_opcode::FUSE_EXCHANGE => {}

                fuse_opcode::CUSE_INIT => {}
            }
        }
    }
}
