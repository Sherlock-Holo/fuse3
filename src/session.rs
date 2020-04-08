use std::convert::TryFrom;
use std::ffi::OsString;
use std::os::raw::c_int;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::sync::Arc;

#[cfg(feature = "async-std-runtime")]
use async_std::fs::File;
#[cfg(feature = "async-std-runtime")]
use async_std::io::prelude::*;
use futures::channel::mpsc::UnboundedSender;
use futures::{Sink, SinkExt};
use log::{debug, error};
#[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
use tokio::prelude::*;

use lazy_static::lazy_static;

use crate::abi::*;
use crate::filesystem::Filesystem;
use crate::helper::*;
use crate::reply::ReplyXAttr;
use crate::request::Request;
use crate::spawn::spawn_without_return;
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

        'dispatch_loop: loop {
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

                    reply_error(libc::ENOSYS, request, self.response_sender.clone());

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
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: err,
                            unique: request.unique,
                        };

                        let init_out_header_data =
                            BINARY.serialize(&init_out_header).expect("won't happened");

                        if let Err(err) = self.fuse_file.write(&init_out_header_data).await {
                            error!("write error init out data to /dev/fuse failed {}", err);
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

                    let out_header = fuse_out_header {
                        len: (FUSE_OUT_HEADER_SIZE + FUSE_INIT_OUT_SIZE) as u32,
                        error: 0,
                        unique: request.unique,
                    };

                    let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_INIT_OUT_SIZE);

                    BINARY
                        .serialize_into(&mut data, &out_header)
                        .expect("won't happened");
                    BINARY
                        .serialize_into(&mut data, &init_out)
                        .expect("won't happened");

                    if let Err(err) = self.fuse_file.write(&data).await {
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

                    let name = match get_first_null_position(data) {
                        None => {
                            error!("lookup body has no null, request unique {}", request.unique);

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let data = match fs.lookup(request, in_header.nodeid, name).await {
                            Err(err) => {
                                let out_header = fuse_out_header {
                                    len: FUSE_OUT_HEADER_SIZE as u32,
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

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE);

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

                fuse_opcode::FUSE_FORGET => {
                    let forget_in = match BINARY.deserialize::<fuse_forget_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_forget_in failed {}, request unique {}",
                                err, request.unique
                            );

                            // don't need reply
                            /*spawn_without_return(async move {
                                let out_header = fuse_out_header {
                                    len: FUSE_OUT_HEADER_SIZE as u32,
                                    error: libc::EIO,
                                    unique: request.unique,
                                };

                                let data = BINARY.serialize(&out_header).expect("won't happened");

                                let _ = resp_sender.send(data).await;
                            });*/

                            continue;
                        }

                        Ok(forget_in) => forget_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        fs.forget(request, in_header.nodeid, forget_in.nlookup)
                            .await
                    });
                }

                fuse_opcode::FUSE_GETATTR => {
                    let mut resp_sender = self.response_sender.clone();

                    let getattr_in = match BINARY.deserialize::<fuse_getattr_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_forget_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

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
                                    len: FUSE_OUT_HEADER_SIZE as u32,
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
                            error!(
                                "deserialize fuse_setattr_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

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
                                    len: FUSE_OUT_HEADER_SIZE as u32,
                                    error: err,
                                    unique: request.unique,
                                };

                                BINARY.serialize(&out_header).expect("won't happened")
                            }

                            Ok(attr) => {
                                let attr_out: fuse_attr_out = attr.into();

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
                                    len: FUSE_OUT_HEADER_SIZE as u32,
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

                    let (name, first_null_index) = match get_first_null_position(data) {
                        None => {
                            error!("symlink has no null, request unique {}", request.unique);

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => (OsString::from_vec((&data[..index]).to_vec()), index),
                    };

                    data = &data[first_null_index + 1..];

                    let link_name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "symlink has no second null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

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
                                        len: FUSE_OUT_HEADER_SIZE as u32,
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
                            error!(
                                "deserialize fuse_mknod_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(mknod_in) => mknod_in,
                    };

                    data = &data[FUSE_MKNOD_IN_SIZE..];

                    let name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_mknod_in body doesn't have null, request unique {}",
                                request.unique
                            );

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

                fuse_opcode::FUSE_MKDIR => {
                    let mut resp_sender = self.response_sender.clone();

                    let mkdir_in = match BINARY.deserialize::<fuse_mkdir_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_mknod_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(mkdir_in) => mkdir_in,
                    };

                    data = &data[FUSE_MKDIR_IN_SIZE..];

                    let name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "deserialize fuse_mknod_in doesn't have null unique {}",
                                request.unique
                            );

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

                    let name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "unlink body doesn't have null, request unique {}",
                                request.unique
                            );

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
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_RMDIR => {
                    let mut resp_sender = self.response_sender.clone();

                    let name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "rmdir body doesn't have null, request unique {}",
                                request.unique
                            );

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
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_RENAME => {
                    let mut resp_sender = self.response_sender.clone();

                    let rename_in = match BINARY.deserialize::<fuse_rename_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_rename_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(rename_in) => rename_in,
                    };

                    data = &data[FUSE_RENAME_IN_SIZE..];

                    let (name, first_null_index) = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_rename_in body doesn't have null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => (OsString::from_vec((&data[..index]).to_vec()), index),
                    };

                    data = &data[first_null_index + 1..];

                    let new_name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_rename_in body doesn't have null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value = if let Err(err) = fs
                            .rename(request, in_header.nodeid, name, rename_in.newdir, new_name)
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_LINK => {
                    let mut resp_sender = self.response_sender.clone();

                    let link_in = match BINARY.deserialize::<fuse_link_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_link_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(link_in) => link_in,
                    };

                    data = &data[FUSE_LINK_IN_SIZE..];

                    let name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_link_in body doesn't have null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        match fs
                            .link(request, link_in.oldnodeid, in_header.nodeid, name)
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

                fuse_opcode::FUSE_OPEN => {
                    let mut resp_sender = self.response_sender.clone();

                    let open_in = match BINARY.deserialize::<fuse_open_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_open_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(open_in) => open_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let opened = match fs.open(request, in_header.nodeid, open_in.flags).await {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(opened) => opened,
                        };

                        let open_out: fuse_open_out = opened.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data =
                            Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &open_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_READ => {
                    let mut resp_sender = self.response_sender.clone();

                    let read_in = match BINARY.deserialize::<fuse_read_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_read_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(read_in) => read_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let mut reply_data = match fs
                            .read(
                                request,
                                in_header.nodeid,
                                read_in.fh,
                                read_in.offset,
                                read_in.size,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_data) => reply_data.data,
                        };

                        reply_data.truncate(read_in.size as usize);

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + reply_data.len()) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + reply_data.len());

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");

                        data.extend_from_slice(&reply_data);

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_WRITE => {
                    let mut resp_sender = self.response_sender.clone();

                    let write_in = match BINARY.deserialize::<fuse_write_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_write_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(write_in) => write_in,
                    };

                    data = &data[FUSE_WRITE_IN_SIZE..];

                    if write_in.size as usize != data.len() {
                        error!("fuse_write_in body len is invalid");

                        reply_error(libc::EINVAL, request, resp_sender);

                        continue;
                    }

                    let data = data.to_vec();

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let reply_write = match fs
                            .write(
                                request,
                                in_header.nodeid,
                                write_in.fh,
                                write_in.offset as i64,
                                data,
                                write_in.flags,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_write) => reply_write,
                        };

                        let write_out: fuse_write_out = reply_write.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data =
                            Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &write_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_STATFS => {
                    let mut resp_sender = self.response_sender.clone();
                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let fs_stat = match fs.statsfs(request, in_header.nodeid).await {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(fs_stat) => fs_stat,
                        };

                        let statfs_out: fuse_statfs_out = fs_stat.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data =
                            Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &statfs_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_RELEASE => {
                    let mut resp_sender = self.response_sender.clone();

                    let release_in = match BINARY.deserialize::<fuse_release_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_release_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(release_in) => release_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let flush = release_in.release_flags & FUSE_RELEASE_FLUSH > 0;

                        let resp_value = if let Err(err) = fs
                            .release(
                                request,
                                in_header.nodeid,
                                release_in.fh,
                                release_in.flags,
                                release_in.lock_owner,
                                flush,
                            )
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_FSYNC => {
                    let mut resp_sender = self.response_sender.clone();

                    let fsync_in = match BINARY.deserialize::<fuse_fsync_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_fsync_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(fsync_in) => fsync_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let data_sync = fsync_in.fsync_flags & 1 > 0;

                        let resp_value = if let Err(err) = fs
                            .fsync(request, in_header.nodeid, fsync_in.fh, data_sync)
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_SETXATTR => {
                    let mut resp_sender = self.response_sender.clone();

                    let setxattr_in = match BINARY.deserialize::<fuse_setxattr_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_setxattr_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(setxattr_in) => setxattr_in,
                    };

                    data = &data[FUSE_SETXATTR_IN_SIZE..];

                    if setxattr_in.size as usize != data.len() {
                        error!(
                            "fuse_setxattr_in body length is not right, request unique {}",
                            request.unique
                        );

                        reply_error(libc::EINVAL, request, resp_sender);

                        continue;
                    }

                    let (name, first_null_index) = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_setxattr_in body has no null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => (OsString::from_vec((&data[..index]).to_vec()), index),
                    };

                    data = &data[first_null_index + 1..];

                    let value = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_setxattr_in value has no second null unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        // TODO handle os X argument
                        let resp_value = if let Err(err) = fs
                            .setxattr(request, in_header.nodeid, name, value, setxattr_in.flags, 0)
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_GETXATTR => {
                    let mut resp_sender = self.response_sender.clone();

                    let getxattr_in = match BINARY.deserialize::<fuse_getxattr_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_getxattr_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(getxattr_in) => getxattr_in,
                    };

                    data = &data[FUSE_GETXATTR_IN_SIZE..];

                    let name = match get_first_null_position(data) {
                        None => {
                            error!("fuse_getxattr_in body has no null {}", request.unique);

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let xattr = match fs
                            .getxattr(request, in_header.nodeid, name, getxattr_in.size)
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(xattr) => xattr,
                        };

                        let data = match xattr {
                            ReplyXAttr::Size(size) => {
                                let getxattr_out = fuse_getxattr_out { size, padding: 0 };

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + FUSE_GETXATTR_OUT_SIZE) as u32,
                                    error: libc::ERANGE,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE);

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");
                                BINARY
                                    .serialize_into(&mut data, &getxattr_out)
                                    .expect("won't happened");

                                data
                            }

                            ReplyXAttr::Data(xattr_data) => {
                                // TODO check is right way or not
                                // TODO should we check data length or not
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + xattr_data.len()) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + xattr_data.len());

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");

                                data.extend_from_slice(&xattr_data);

                                data
                            }
                        };

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_LISTXATTR => {
                    let mut resp_sender = self.response_sender.clone();

                    let listxattr_in = match BINARY.deserialize::<fuse_getxattr_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_getxattr_in in listxattr failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(listxattr_in) => listxattr_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let xattr = match fs
                            .listxattr(request, in_header.nodeid, listxattr_in.size)
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(xattr) => xattr,
                        };

                        let data = match xattr {
                            ReplyXAttr::Size(size) => {
                                let getxattr_out = fuse_getxattr_out { size, padding: 0 };

                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + FUSE_GETXATTR_OUT_SIZE) as u32,
                                    error: libc::ERANGE,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_STATFS_OUT_SIZE);

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");
                                BINARY
                                    .serialize_into(&mut data, &getxattr_out)
                                    .expect("won't happened");

                                data
                            }

                            ReplyXAttr::Data(xattr_data) => {
                                // TODO check is right way or not
                                // TODO should we check data length or not
                                let out_header = fuse_out_header {
                                    len: (FUSE_OUT_HEADER_SIZE + xattr_data.len()) as u32,
                                    error: 0,
                                    unique: request.unique,
                                };

                                let mut data =
                                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + xattr_data.len());

                                BINARY
                                    .serialize_into(&mut data, &out_header)
                                    .expect("won't happened");

                                data.extend_from_slice(&xattr_data);

                                data
                            }
                        };

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_REMOVEXATTR => {
                    let mut resp_sender = self.response_sender.clone();

                    let name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse removexattr body has no null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value = if let Err(err) =
                            fs.removexattr(request, in_header.nodeid, name).await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_FLUSH => {
                    let mut resp_sender = self.response_sender.clone();

                    let flush_in = match BINARY.deserialize::<fuse_flush_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_flush_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(flush_in) => flush_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value = if let Err(err) = fs
                            .flush(request, in_header.nodeid, flush_in.fh, flush_in.lock_owner)
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_OPENDIR => {
                    let mut resp_sender = self.response_sender.clone();

                    let open_in = match BINARY.deserialize::<fuse_open_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_open_in in opendir failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(open_in) => open_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let reply_open =
                            match fs.opendir(request, in_header.nodeid, open_in.flags).await {
                                Err(err) => {
                                    reply_error(err, request, resp_sender);

                                    return;
                                }

                                Ok(reply_open) => reply_open,
                            };

                        let open_out: fuse_open_out = reply_open.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data =
                            Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &open_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_READDIR => {
                    let mut resp_sender = self.response_sender.clone();

                    let read_in = match BINARY.deserialize::<fuse_read_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_read_in in readdir failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(read_in) => read_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let reply_readdir = match fs
                            .readdir(request, in_header.nodeid, read_in.fh, read_in.offset as i64)
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_readdir) => reply_readdir,
                        };

                        let max_size = read_in.size as usize;

                        let mut entry_data = Vec::with_capacity(max_size);

                        const ENTRY_SIZE_BASE: usize = 8;

                        for entry in reply_readdir.entries {
                            let mut dir_entry_size = FUSE_DIRENT_SIZE + entry.name.len(); //TODO should I +1 for the name's null?
                            let padding_size = dir_entry_size % ENTRY_SIZE_BASE;
                            dir_entry_size += padding_size;

                            if entry_data.len() + dir_entry_size > max_size {
                                break;
                            }

                            let dir_entry = fuse_dirent {
                                ino: entry.inode,
                                off: entry.offset,
                                namelen: entry.name.len() as u32, //TODO should I +1 for the name's null?
                                // learn from fuse-rs and golang bazil.org fuse DirentType
                                r#type: mode_from_kind_and_perm(entry.kind, 0) >> 12,
                            };

                            BINARY
                                .serialize_into(&mut entry_data, &dir_entry)
                                .expect("won't happened");

                            entry_data.extend_from_slice(entry.name.as_bytes());

                            // padding
                            for _ in 0..padding_size {
                                entry_data.push(0);
                            }
                        }

                        // TODO find a way to avoid multi allocate

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + entry_data.len()) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + entry_data.len());

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");

                        data.extend_from_slice(&entry_data);

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_RELEASEDIR => {
                    let mut resp_sender = self.response_sender.clone();

                    let release_in = match BINARY.deserialize::<fuse_release_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_release_in in releasedir failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(release_in) => release_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value = if let Err(err) = fs
                            .releasedir(request, in_header.nodeid, release_in.fh, release_in.flags)
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_FSYNCDIR => {
                    let mut resp_sender = self.response_sender.clone();

                    let fsync_in = match BINARY.deserialize::<fuse_fsync_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_fsync_in in fsyncdir failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(fsync_in) => fsync_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let data_sync = fsync_in.fsync_flags & 1 > 0;

                        let resp_value = if let Err(err) = fs
                            .fsyncdir(request, in_header.nodeid, fsync_in.fh, data_sync)
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_GETLK => {
                    let mut resp_sender = self.response_sender.clone();

                    let getlk_in = match BINARY.deserialize::<fuse_lk_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_lk_in in getlk failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(getlk_in) => getlk_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let reply_lock = match fs
                            .getlk(
                                request,
                                in_header.nodeid,
                                getlk_in.fh,
                                getlk_in.owner,
                                getlk_in.lk.start,
                                getlk_in.lk.end,
                                getlk_in.lk.r#type,
                                getlk_in.lk.pid,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_lock) => reply_lock,
                        };

                        let getlk_out: fuse_lk_out = reply_lock.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_LK_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_LK_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &getlk_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_SETLK | fuse_opcode::FUSE_SETLKW => {
                    let mut resp_sender = self.response_sender.clone();

                    let setlk_in = match BINARY.deserialize::<fuse_lk_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_lk_in in {:?} failed {}, request unique {}",
                                opcode, err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(setlk_in) => setlk_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let block = opcode == fuse_opcode::FUSE_SETLKW;

                        let reply_lock = match fs
                            .setlk(
                                request,
                                in_header.nodeid,
                                setlk_in.fh,
                                setlk_in.owner,
                                setlk_in.lk.start,
                                setlk_in.lk.end,
                                setlk_in.lk.r#type,
                                setlk_in.lk.pid,
                                block,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_lock) => reply_lock,
                        };

                        let setlk_out: fuse_lk_out = reply_lock.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_LK_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_LK_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &setlk_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_ACCESS => {
                    let mut resp_sender = self.response_sender.clone();

                    let access_in = match BINARY.deserialize::<fuse_access_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_access_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(access_in) => access_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value = if let Err(err) =
                            fs.access(request, in_header.nodeid, access_in.mask).await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_CREATE => {
                    let mut resp_sender = self.response_sender.clone();

                    let create_in = match BINARY.deserialize::<fuse_create_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_create_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(create_in) => create_in,
                    };

                    data = &data[FUSE_CREATE_IN_SIZE..];

                    let name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_create_in body has no null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let created = match fs
                            .create(
                                request,
                                in_header.nodeid,
                                name,
                                create_in.mode,
                                create_in.flags,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(created) => created,
                        };

                        let (entry_out, open_out): (fuse_entry_out, fuse_open_out) = created.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE + FUSE_OPEN_OUT_SIZE)
                                as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data = Vec::with_capacity(
                            FUSE_OUT_HEADER_SIZE + FUSE_ENTRY_OUT_SIZE + FUSE_OPEN_OUT_SIZE,
                        );

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &entry_out)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &open_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_INTERRUPT => {
                    let mut resp_sender = self.response_sender.clone();

                    let interrupt_in = match BINARY.deserialize::<fuse_interrupt_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_interrupt_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(interrupt_in) => interrupt_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value =
                            if let Err(err) = fs.interrupt(request, interrupt_in.unique).await {
                                err
                            } else {
                                0
                            };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_BMAP => {
                    let mut resp_sender = self.response_sender.clone();

                    let bmap_in = match BINARY.deserialize::<fuse_bmap_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_bmap_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(bmap_in) => bmap_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let reply_bmap = match fs
                            .bmap(request, in_header.nodeid, bmap_in.blocksize, bmap_in.block)
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_bmap) => reply_bmap,
                        };

                        let bmap_out: fuse_bmap_out = reply_bmap.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_BMAP_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data =
                            Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_BMAP_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &bmap_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                /*fuse_opcode::FUSE_IOCTL => {
                    let mut resp_sender = self.response_sender.clone();

                    let ioctl_in = match BINARY.deserialize::<fuse_ioctl_in>(data) {
                        Err(err) => {
                            error!("deserialize fuse_ioctl_in failed {}", err);

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(ioctl_in) => ioctl_in,
                    };

                    let ioctl_data = (&data[FUSE_IOCTL_IN_SIZE..]).to_vec();

                    let fs = self.filesystem.clone();
                }*/
                fuse_opcode::FUSE_POLL => {
                    let mut resp_sender = self.response_sender.clone();

                    let poll_in = match BINARY.deserialize::<fuse_poll_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_poll_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(poll_in) => poll_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let reply_poll = match fs
                            .poll(
                                request,
                                in_header.nodeid,
                                poll_in.fh,
                                poll_in.kh,
                                poll_in.flags,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_poll) => reply_poll,
                        };

                        let poll_out: fuse_poll_out = reply_poll.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_POLL_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data =
                            Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_POLL_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &poll_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                // fuse_opcode::FUSE_NOTIFY_REPLY => {}
                fuse_opcode::FUSE_BATCH_FORGET => {
                    let batch_forget_in = match BINARY.deserialize::<fuse_batch_forget_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_batch_forget_in failed {}, request unique {}",
                                err, request.unique
                            );

                            // no need to reply
                            continue;
                        }

                        Ok(batch_forget_in) => batch_forget_in,
                    };

                    let mut forgets = vec![];

                    data = &data[FUSE_BATCH_FORGET_IN_SIZE..];

                    // TODO if has less data, should I return error?
                    while data.len() >= FUSE_FORGET_ONE_SIZE {
                        match BINARY.deserialize::<fuse_forget_one>(data) {
                            Err(err) => {
                                error!("deserialize fuse_batch_forget_in body fuse_forget_one failed {}, request unique {}", err, request.unique);

                                // no need to reply
                                continue 'dispatch_loop;
                            }

                            Ok(forget_one) => forgets.push(forget_one),
                        }
                    }

                    if forgets.len() != batch_forget_in.count as usize {
                        error!("fuse_forget_one count != fuse_batch_forget_in.count, request unique {}", request.unique);

                        continue;
                    }

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let inodes = forgets
                            .into_iter()
                            .map(|forget_one| forget_one.nodeid)
                            .collect::<Vec<_>>();

                        fs.batch_forget(request, &inodes).await
                    });
                }

                fuse_opcode::FUSE_FALLOCATE => {
                    let mut resp_sender = self.response_sender.clone();

                    let fallocate_in = match BINARY.deserialize::<fuse_fallocate_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_fallocate_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(fallocate_in) => fallocate_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value = if let Err(err) = fs
                            .fallocate(
                                request,
                                in_header.nodeid,
                                fallocate_in.fh,
                                fallocate_in.offset,
                                fallocate_in.length,
                                fallocate_in.mode,
                            )
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_READDIRPLUS => {
                    let mut resp_sender = self.response_sender.clone();

                    let readdirplus_in = match BINARY.deserialize::<fuse_read_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_read_in in readdirplus failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(readdirplus_in) => readdirplus_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let directory_plus = match fs
                            .readdirplus(
                                request,
                                in_header.nodeid,
                                readdirplus_in.fh,
                                readdirplus_in.offset,
                                readdirplus_in.lock_owner,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(directory_plus) => directory_plus,
                        };

                        let max_size = readdirplus_in.size as usize;

                        let mut entry_data = Vec::with_capacity(max_size);

                        const ENTRY_SIZE_BASE: usize = 8;

                        for entry in directory_plus.entries {
                            let mut dir_entry_size = FUSE_DIRENTPLUS_SIZE + entry.name.len(); //TODO should I +1 for the name's null?
                            let padding_size = dir_entry_size % ENTRY_SIZE_BASE;
                            dir_entry_size += padding_size;

                            if entry_data.len() + dir_entry_size > max_size {
                                break;
                            }

                            let attr = entry.attr;

                            let dir_entry = fuse_direntplus {
                                entry_out: fuse_entry_out {
                                    nodeid: attr.ino,
                                    generation: entry.generation,
                                    entry_valid: entry.entry_ttl.as_secs(),
                                    attr_valid: entry.attr_ttl.as_secs(),
                                    entry_valid_nsec: entry.entry_ttl.subsec_nanos(),
                                    attr_valid_nsec: entry.attr_ttl.subsec_nanos(),
                                    attr: attr.into(),
                                },
                                dirent: fuse_dirent {
                                    ino: entry.inode,
                                    off: entry.offset,
                                    namelen: entry.name.len() as u32,
                                    // learn from fuse-rs and golang bazil.org fuse DirentType
                                    r#type: mode_from_kind_and_perm(entry.kind, 0) >> 12,
                                },
                            };

                            BINARY
                                .serialize_into(&mut entry_data, &dir_entry)
                                .expect("won't happened");

                            entry_data.extend_from_slice(entry.name.as_bytes());

                            // padding
                            for _ in 0..padding_size {
                                entry_data.push(0);
                            }
                        }

                        // TODO find a way to avoid multi allocate

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + entry_data.len()) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data = Vec::with_capacity(FUSE_OUT_HEADER_SIZE + entry_data.len());

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");

                        data.extend_from_slice(&entry_data);

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_RENAME2 => {
                    let mut resp_sender = self.response_sender.clone();

                    let rename2_in = match BINARY.deserialize::<fuse_rename2_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_rename2_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(rename2_in) => rename2_in,
                    };

                    data = &data[FUSE_RENAME2_IN_SIZE..];

                    let (old_name, index) = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_rename2_in body doesn't have null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => (OsString::from_vec((&data[..index]).to_vec()), index),
                    };

                    data = &data[index + 1..];

                    let new_name = match get_first_null_position(data) {
                        None => {
                            error!(
                                "fuse_rename2_in body doesn't have second null, request unique {}",
                                request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Some(index) => OsString::from_vec((&data[..index]).to_vec()),
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let resp_value = if let Err(err) = fs
                            .rename2(
                                request,
                                in_header.nodeid,
                                old_name,
                                rename2_in.newdir,
                                new_name,
                                rename2_in.flags,
                            )
                            .await
                        {
                            err
                        } else {
                            0
                        };

                        let out_header = fuse_out_header {
                            len: FUSE_OUT_HEADER_SIZE as u32,
                            error: resp_value,
                            unique: request.unique,
                        };

                        let data = BINARY.serialize(&out_header).expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_LSEEK => {
                    let mut resp_sender = self.response_sender.clone();

                    let lseek_in = match BINARY.deserialize::<fuse_lseek_in>(data) {
                        Err(err) => {
                            error!(
                                "deserialize fuse_lseek_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(lseek_in) => lseek_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let reply_lseek = match fs
                            .lseek(
                                request,
                                in_header.nodeid,
                                lseek_in.fh,
                                lseek_in.offset,
                                lseek_in.whence,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_lseek) => reply_lseek,
                        };

                        let lseek_out: fuse_lseek_out = reply_lseek.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_LSEEK_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data =
                            Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_OPEN_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &lseek_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                fuse_opcode::FUSE_COPY_FILE_RANGE => {
                    let mut resp_sender = self.response_sender.clone();

                    let copy_file_range_in = match BINARY
                        .deserialize::<fuse_copy_file_range_in>(data)
                    {
                        Err(err) => {
                            error!(
                                "deserialize fuse_copy_file_range_in failed {}, request unique {}",
                                err, request.unique
                            );

                            reply_error(libc::EINVAL, request, resp_sender);

                            continue;
                        }

                        Ok(copy_file_range_in) => copy_file_range_in,
                    };

                    let fs = self.filesystem.clone();

                    spawn_without_return(async move {
                        let reply_copy_file_range = match fs
                            .copy_file_range(
                                request,
                                in_header.nodeid,
                                copy_file_range_in.fh_in,
                                copy_file_range_in.off_in,
                                copy_file_range_in.nodeid_out,
                                copy_file_range_in.fh_out,
                                copy_file_range_in.off_out,
                                copy_file_range_in.len,
                                copy_file_range_in.flags,
                            )
                            .await
                        {
                            Err(err) => {
                                reply_error(err, request, resp_sender);

                                return;
                            }

                            Ok(reply_copy_file_range) => reply_copy_file_range,
                        };

                        let write_out: fuse_write_out = reply_copy_file_range.into();

                        let out_header = fuse_out_header {
                            len: (FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE) as u32,
                            error: 0,
                            unique: request.unique,
                        };

                        let mut data =
                            Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_WRITE_OUT_SIZE);

                        BINARY
                            .serialize_into(&mut data, &out_header)
                            .expect("won't happened");
                        BINARY
                            .serialize_into(&mut data, &write_out)
                            .expect("won't happened");

                        let _ = resp_sender.send(data).await;
                    });
                }

                #[cfg(target_os = "macos")]
                fuse_opcode::FUSE_SETVOLNAME => {}

                #[cfg(target_os = "macos")]
                fuse_opcode::FUSE_GETXTIMES => {}

                #[cfg(target_os = "macos")]
                fuse_opcode::FUSE_EXCHANGE => {} // fuse_opcode::CUSE_INIT => {}
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
            len: FUSE_OUT_HEADER_SIZE as u32,
            error: err,
            unique: request.unique,
        };

        let data = BINARY.serialize(&out_header).expect("won't happened");

        let _ = sender.send(data).await;
    });
}
