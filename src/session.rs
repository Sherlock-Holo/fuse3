use std::convert::TryFrom;
use std::ffi::{CStr, OsString};
use std::os::raw::c_int;
use std::os::unix::ffi::OsStringExt;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

#[cfg(feature = "async-std-runtime")]
use async_std::fs::{File, OpenOptions};
#[cfg(feature = "async-std-runtime")]
use async_std::io::prelude::*;
use futures::channel::mpsc::UnboundedSender;
use futures::{Sink, SinkExt};
use log::{debug, error, warn};
#[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
use tokio::prelude::*;

use lazy_static::lazy_static;

use crate::abi::{
    fuse_attr, fuse_attr_out, fuse_entry_out, fuse_forget_in, fuse_getattr_in, fuse_in_header,
    fuse_init_in, fuse_init_out, fuse_mkdir_in, fuse_mknod_in, fuse_opcode, fuse_out_header,
    fuse_setattr_in, BUFFER_SIZE, DEFAULT_CONGESTION_THRESHOLD, DEFAULT_MAP_ALIGNMENT,
    DEFAULT_MAX_BACKGROUND, DEFAULT_MAX_PAGES, DEFAULT_TIME_GRAN, FATTR_GID, FATTR_MODE,
    FATTR_SIZE, FATTR_UID, FUSE_ASYNC_READ, FUSE_ATOMIC_O_TRUNC, FUSE_ATTR_OUT_SIZE,
    FUSE_AUTO_INVAL_DATA, FUSE_BIG_WRITES, FUSE_DONT_MASK, FUSE_DO_READDIRPLUS,
    FUSE_ENTRY_OUT_SIZE, FUSE_EXPORT_SUPPORT, FUSE_FILE_OPS, FUSE_FLOCK_LOCKS, FUSE_HAS_IOCTL_DIR,
    FUSE_IN_HEADER_SIZE, FUSE_KERNEL_MINOR_VERSION, FUSE_KERNEL_VERSION, FUSE_MKDIR_IN_SIZE,
    FUSE_MKNOD_IN_SIZE, FUSE_OUT_HEADER_SIZE, FUSE_PARALLEL_DIROPS, FUSE_POSIX_ACL,
    FUSE_POSIX_LOCKS, FUSE_READDIRPLUS_AUTO, FUSE_SPLICE_MOVE, FUSE_SPLICE_READ, FUSE_SPLICE_WRITE,
    FUSE_WRITEBACK_CACHE, MAX_WRITE_SIZE,
};
use crate::filesystem::Filesystem;
use crate::helper::*;
use crate::request::Request;
use crate::spawn::{spawn, spawn_without_return};
use crate::{Result, SetAttr};

// #[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
// use tokio::fs::{OpenOptions, File};

lazy_static! {
    static ref BINARY: bincode::Config = {
        let mut cfg = bincode::config();
        cfg.big_endian();

        cfg
    };
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
pub struct Session<T> {
    fuse_file: File,
    filesystem: Arc<T>,
    response_sender: UnboundedSender<Vec<u8>>,
}

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
impl<T: Filesystem + Send + Sync + 'static> Session<T> {
    pub async fn dispatch(&mut self) -> Result<()> {
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

            data = &data[FUSE_IN_HEADER_SIZE..in_header.len as usize - FUSE_IN_HEADER_SIZE];

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

                    if let Err(err) = self.filesystem.init(request).await {
                        let init_out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE) as u32,
                            error: err,
                            unique: request.unique,
                        };

                        let init_out_header_data =
                            BINARY.serialize(&init_out_header).expect("won't happened");

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

                    let mut init_out_data = BINARY.serialize(&init_out).expect("won't happened");

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + init_out_data.len()) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut out_header_data =
                        BINARY.serialize(&out_header).expect("won't happened");

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

                    let name = match CStr::from_bytes_with_nul(data) {
                        Err(err) => {
                            warn!("receive invalid name in lookup unique {}", request.unique);

                            spawn_without_return(async move {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: libc::EINVAL,
                                    unique: request.unique,
                                };

                                let data = BINARY.serialize(&out_header).expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            });

                            continue;
                        }

                        Ok(name) => OsString::from_vec(name.to_bytes().to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let data = match fs.lookup(request, in_header.nodeid, name).await {
                            Err(err) => {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: err,
                                    unique: request.unique,
                                };

                                BINARY.serialize(&out_header).expect("won't happened")
                            }

                            Ok(entry) => {
                                let entry_out: fuse_entry_out = entry.into();
                                let mut entry_out_data =
                                    BINARY.serialize(&entry_out).expect("won't happened");

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + entry_out_data.len()) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };
                                let mut out_header_data =
                                    BINARY.serialize(&out_header).expect("won't happened");

                                out_header_data.append(&mut entry_out_data);

                                out_header_data
                            }
                        };

                        let _ = resp_sender.send(data).await;
                    });
                }
                fuse_opcode::FUSE_FORGET => {
                    let mut resp_sender = self.response_sender.clone();

                    let forget_in = match BINARY.deserialize::<fuse_forget_in>(data) {
                        Err(err) => {
                            error!("deserialize fuse_forget_in failed {}", err);

                            spawn_without_return(async move {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: libc::EIO,
                                    unique: request.unique,
                                };

                                let data = BINARY.serialize(&out_header).expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            });

                            continue;
                        }

                        Ok(forget_in) => forget_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        if let Err(err) = fs
                            .forget(request, in_header.nodeid, forget_in.nlookup)
                            .await
                        {
                            let out_header = fuse_out_header {
                                len: (FUSE_OUT_HEADER_SIZE) as u32,
                                error: err,
                                unique: request.unique,
                            };

                            let data = BINARY.serialize(&out_header).expect("won't happened");

                            let _ = resp_sender.send(data).await;
                        }
                    });
                }
                fuse_opcode::FUSE_GETATTR => {
                    let mut resp_sender = self.response_sender.clone();

                    let getattr_in = match BINARY.deserialize::<fuse_getattr_in>(data) {
                        Err(err) => {
                            error!("deserialize fuse_forget_in failed {}", err);

                            spawn_without_return(async move {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: libc::EIO,
                                    unique: request.unique,
                                };

                                let data = BINARY.serialize(&out_header).expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            });

                            continue;
                        }

                        Ok(getattr_in) => getattr_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let data = match fs
                            .getattr(
                                request,
                                in_header.nodeid,
                                getattr_in.fh,
                                getattr_in.getattr_flags,
                            )
                            .await
                        {
                            Err(err) => {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: err,
                                    unique: request.unique,
                                };

                                BINARY.serialize(&out_header).expect("won't happened")
                            }

                            Ok(attr) => {
                                let attr_out = fuse_attr_out {
                                    attr_valid: attr.ttl.as_secs(),
                                    attr_valid_nsec: attr.ttl.subsec_nanos(),
                                    dummy: getattr_in.dummy,
                                    attr: attr.attr.into(),
                                };

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE);

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");
                                BINARY
                                    .serialize_into(&mut data, &attr_out)
                                    .expect("won't happened");

                                data
                            }
                        };

                        let _ = resp_sender.send(data).await;
                    });
                }
                fuse_opcode::FUSE_SETATTR => {
                    let mut resp_sender = self.response_sender.clone();

                    let setattr_in = match BINARY.deserialize::<fuse_setattr_in>(data) {
                        Err(err) => {
                            spawn_without_return(async move {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: libc::EIO,
                                    unique: request.unique,
                                };

                                let data = BINARY.serialize(&out_header).expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            });

                            continue;
                        }

                        Ok(setattr_in) => setattr_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let set_attr = SetAttr::from(&setattr_in);

                        let data = match fs.setattr(request, in_header.nodeid, set_attr).await {
                            Err(err) => {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: err,
                                    unique: request.unique,
                                };

                                BINARY.serialize(&out_header).expect("won't happened")
                            }

                            Ok(attr) => {
                                let attr_out = fuse_attr_out {
                                    attr_valid: attr.ttl.as_secs(),
                                    attr_valid_nsec: attr.ttl.subsec_nanos(),
                                    dummy: 0,
                                    attr: attr.attr.into(),
                                };

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ATTR_OUT_SIZE);

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");
                                BINARY
                                    .serialize_into(&mut data, &attr_out)
                                    .expect("won't happened");

                                data
                            }
                        };

                        let _ = resp_sender.send(data).await;
                    });
                }
                fuse_opcode::FUSE_READLINK => {
                    let mut resp_sender = self.response_sender.clone();
                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let data = match fs.readlink(request, in_header.nodeid).await {
                            Err(err) => {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: err,
                                    unique: request.unique,
                                };

                                BINARY.serialize(&out_header).expect("won't happened")
                            }

                            Ok(data) => {
                                let content = data.data;

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + content.len()) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + content.len());

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");

                                data.extend_from_slice(&content);

                                data
                            }
                        };

                        let _ = resp_sender.send(data).await;
                    });
                }
                fuse_opcode::FUSE_SYMLINK => {
                    let mut resp_sender = self.response_sender.clone();

                    let (name, first_null_index) =
                        match data.iter().enumerate().find_map(|(index, char)| {
                            if *char == 0 {
                                Some(index)
                            } else {
                                None
                            }
                        }) {
                            None => {
                                warn!("symlink body has no 0");

                                spawn_without_return(async move {
                                    let out_header = fuse_out_header {
                                        len: (FUSE_OUT_HEADER_SIZE) as u32,
                                        error: libc::EINVAL,
                                        unique: request.unique,
                                    };

                                    let data =
                                        BINARY.serialize(&out_header).expect("won't happened");

                                    let _ = resp_sender.send(data).await;
                                });

                                continue;
                            }

                            Some(index) => (OsString::from_vec((&data[..index]).to_vec()), index),
                        };

                    data = &data[first_null_index + 1..];

                    let link_name = match data.iter().enumerate().find_map(|(index, char)| {
                        if *char == 0 {
                            Some(index)
                        } else {
                            None
                        }
                    }) {
                        None => {
                            warn!("symlink link name has no 0");

                            spawn_without_return(async move {
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE) as u32,
                                    error: libc::EINVAL,
                                    unique: request.unique,
                                };

                                let data = BINARY.serialize(&out_header).expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            });

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let data =
                            match fs.symlink(request, in_header.nodeid, name, link_name).await {
                                Err(err) => {
                                    let out_header = fuse_out_header {
                                        len: (FUSE_OUT_HEADER_SIZE) as u32,
                                        error: err,
                                        unique: request.unique,
                                    };

                                    BINARY.serialize(&out_header).expect("won't happened")
                                }

                                Ok(entry) => {
                                    let entry_out: fuse_entry_out = entry.into();

                                    let out_header = fuse_out_header {
                                        len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                                        error: 0,
                                        unique: request.unique,
                                    };

                                    let mut data = Vec::with_capacity(
                                        FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE,
                                    );

                                    BINARY
                                        .serialize_into(&mut data, &out_header)
                                        .expect("won't happened");
                                    BINARY
                                        .serialize_into(&mut data, &entry_out)
                                        .expect("won't happened");

                                    data
                                }
                            };

                        let _ = resp_sender.send(data).await;
                    });
                }
                fuse_opcode::FUSE_MKNOD => {
                    let mut resp_sender = self.response_sender.clone();

                    let mknod_in = match BINARY.deserialize::<fuse_mknod_in>(data) {
                        Err(err) => {
                            error!("deserialize fuse_mknod_in failed {}", err);

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(mknod_in) => mknod_in,
                    };

                    data = &data[FUSE_MKNOD_IN_SIZE..];

                    let name = match data.iter().enumerate().find_map(|(index, char)| {
                        if *char == 0 {
                            Some(index)
                        } else {
                            None
                        }
                    }) {
                        None => {
                            error!("deserialize fuse_mknod_in body doesn't have 0");

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        match fs
                            .mknod(
                                request,
                                in_header.nodeid,
                                name,
                                mknod_in.mode,
                                mknod_in.rdev,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(libc::EINVAL, request, resp_sender);
                            }

                            Ok(entry) => {
                                let entry_out: fuse_entry_out = entry.into();

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");
                                BINARY
                                    .serialize_into(&mut data, &entry_out)
                                    .expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            }
                        }
                    });
                }
                fuse_opcode::FUSE_MKDIR => {
                    let mut resp_sender = self.response_sender.clone();

                    let mkdir_in = match BINARY.deserialize::<fuse_mkdir_in>(data) {
                        Err(err) => {
                            error!("deserialize fuse_mknod_in failed {}", err);

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(mkdir_in) => mkdir_in,
                    };

                    data = &data[FUSE_MKDIR_IN_SIZE..];

                    let name = match index_first_null(data) {
                        None => {
                            error!("deserialize fuse_mknod_in doesn't have 0");

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        match fs
                            .mkdir(
                                request,
                                in_header.nodeid,
                                name,
                                mkdir_in.mode,
                                mkdir_in.umask,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);
                            }

                            Ok(entry) => {
                                let entry_out: fuse_entry_out = entry.into();

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");
                                BINARY
                                    .serialize_into(&mut data, &entry_out)
                                    .expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            }
                        }
                    });
                }
                fuse_opcode::FUSE_UNLINK => {
                    let mut resp_sender = self.response_sender.clone();

                    let name = match index_first_null(data) {
                        None => {
                            error!("unlink body doesn't have 0");

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value =
                            if let Err(err) = fs.unlink(request, in_header.nodeid, name).await {
                                err
                            } else {
                                0
                            };

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE) as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = sender.send(data).await;
                    });
                }
                fuse_opcode::FUSE_RMDIR => {
                    let mut resp_sender = self.response_sender.clone();

                    let name = match index_first_null(data) {
                        None => {
                            error!("rmdir body doesn't have 0");

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value =
                            if let Err(err) = fs.unlink(request, in_header.nodeid, name).await {
                                err
                            } else {
                                0
                            };

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE) as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = sender.send(data).await;
                    });
                }
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

fn reply_error<S>(err: c_int, request: Request, mut sender: S)
where
    S: Sink<Vec<u8>> + Send + Sync + 'static + Unpin,
{
    spawn_without_return(async move {
        let out_header = fuse_out_header {
            len: (FUSE_OUT_HEADER_SIZE) as u32,
            error: err,
            unique: request.unique,
        };

        let data = BINARY.serialize(&out_header).expect("won't happened");

        let _ = sender.send(data).await;
    });
}
