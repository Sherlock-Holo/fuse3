use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;

use futures::channel::mpsc::UnboundedSender;
use futures::SinkExt;

use lazy_static::lazy_static;

use crate::abi::{
    fuse_notify_code, fuse_notify_delete_out, fuse_notify_inval_entry_out,
    fuse_notify_inval_inode_out, fuse_notify_poll_wakeup_out, fuse_notify_retrieve_out,
    fuse_notify_store_out, fuse_out_header, FUSE_NOTIFY_DELETE_OUT_SIZE,
    FUSE_NOTIFY_INVAL_ENTRY_OUT_SIZE, FUSE_NOTIFY_INVAL_INODE_OUT_SIZE,
    FUSE_NOTIFY_POLL_WAKEUP_OUT_SIZE, FUSE_NOTIFY_RETRIEVE_OUT_SIZE, FUSE_NOTIFY_STORE_OUT_SIZE,
    FUSE_OUT_HEADER_SIZE,
};

lazy_static! {
    static ref BINARY: bincode::Config = {
        let mut cfg = bincode::config();
        cfg.little_endian();

        cfg
    };
}

#[derive(Debug)]
pub struct PollNotify {
    sender: UnboundedSender<Vec<u8>>,
}

impl PollNotify {
    pub(crate) fn new(sender: UnboundedSender<Vec<u8>>) -> Self {
        Self { sender }
    }

    pub async fn notify(mut self, kind: PollNotifyKind) -> std::result::Result<(), PollNotifyKind> {
        let data = match &kind {
            PollNotifyKind::Wakeup { kh } => {
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_POLL_WAKEUP_OUT_SIZE) as u32,
                    error: fuse_notify_code::FUSE_POLL as i32,
                    unique: 0,
                };

                let wakeup_out = fuse_notify_poll_wakeup_out { kh: *kh };

                let mut data =
                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_POLL_WAKEUP_OUT_SIZE);

                BINARY
                    .serialize_into(&mut data, &out_header)
                    .expect("vec size is not enough");
                BINARY
                    .serialize_into(&mut data, &wakeup_out)
                    .expect("vec size is not enough");

                data
            }

            PollNotifyKind::InvalidInode { inode, offset, len } => {
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_INVAL_INODE_OUT_SIZE) as u32,
                    error: fuse_notify_code::FUSE_NOTIFY_INVAL_INODE as i32,
                    unique: 0,
                };

                let invalid_inode_out = fuse_notify_inval_inode_out {
                    ino: *inode,
                    off: *offset,
                    len: *len,
                };

                let mut data =
                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_INVAL_INODE_OUT_SIZE);

                BINARY
                    .serialize_into(&mut data, &out_header)
                    .expect("vec size is not enough");
                BINARY
                    .serialize_into(&mut data, &invalid_inode_out)
                    .expect("vec size is not enough");

                data
            }

            PollNotifyKind::InvalidEntry { parent, name } => {
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_INVAL_ENTRY_OUT_SIZE) as u32,
                    error: fuse_notify_code::FUSE_NOTIFY_INVAL_ENTRY as i32,
                    unique: 0,
                };

                let invalid_entry_out = fuse_notify_inval_entry_out {
                    parent: *parent,
                    namelen: name.len() as _,
                    padding: 0,
                };

                let mut data = Vec::with_capacity(
                    FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_INVAL_ENTRY_OUT_SIZE + name.len(),
                );

                BINARY
                    .serialize_into(&mut data, &out_header)
                    .expect("vec size is not enough");
                BINARY
                    .serialize_into(&mut data, &invalid_entry_out)
                    .expect("vec size is not enough");

                data.extend_from_slice(name.as_bytes());

                // TODO should I add null at the end?

                data
            }

            PollNotifyKind::Delete {
                parent,
                child,
                name,
            } => {
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_DELETE_OUT_SIZE) as u32,
                    error: 0,
                    unique: 0,
                };

                let delete_out = fuse_notify_delete_out {
                    parent: *parent,
                    child: *child,
                    namelen: name.len() as _,
                    padding: 0,
                };

                let mut data = Vec::with_capacity(
                    FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_DELETE_OUT_SIZE + name.len(),
                );

                BINARY
                    .serialize_into(&mut data, &out_header)
                    .expect("vec size is not enough");
                BINARY
                    .serialize_into(&mut data, &delete_out)
                    .expect("vec size is not enough");

                data.extend_from_slice(name.as_bytes());

                // TODO should I add null at the end?

                data
            }

            PollNotifyKind::Store {
                inode,
                offset,
                data,
            } => {
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_STORE_OUT_SIZE) as u32,
                    error: 0,
                    unique: 0,
                };

                let store_out = fuse_notify_store_out {
                    nodeid: *inode,
                    offset: *offset,
                    size: data.len() as _,
                    padding: 0,
                };

                let mut data_buf = Vec::with_capacity(
                    FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_STORE_OUT_SIZE + data.len(),
                );

                BINARY
                    .serialize_into(&mut data_buf, &out_header)
                    .expect("vec size is not enough");
                BINARY
                    .serialize_into(&mut data_buf, &store_out)
                    .expect("vec size is not enough");

                data_buf.extend_from_slice(data);

                data_buf
            }

            PollNotifyKind::Retrieve {
                notify_unique,
                inode,
                offset,
                size,
            } => {
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_RETRIEVE_OUT_SIZE) as u32,
                    error: 0,
                    unique: 0,
                };

                let retrieve_out = fuse_notify_retrieve_out {
                    notify_unique: *notify_unique,
                    nodeid: *inode,
                    offset: *offset,
                    size: *size,
                    padding: 0,
                };

                let mut data =
                    Vec::with_capacity(FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_RETRIEVE_OUT_SIZE);

                BINARY
                    .serialize_into(&mut data, &out_header)
                    .expect("vec size is not enough");
                BINARY
                    .serialize_into(&mut data, &retrieve_out)
                    .expect("vec size is not enough");

                data
            }
        };

        self.sender.send(data).await.or(Err(kind))
    }
}

#[derive(Debug)]
pub enum PollNotifyKind {
    Wakeup {
        kh: u64,
    },

    // TODO need check is right or not
    InvalidInode {
        inode: u64,
        offset: i64,
        len: i64,
    },

    InvalidEntry {
        parent: u64,
        name: OsString,
    },

    Delete {
        parent: u64,
        child: u64,
        name: OsString,
    },

    Store {
        inode: u64,
        offset: u64,
        data: Vec<u8>,
    },

    Retrieve {
        notify_unique: u64,
        inode: u64,
        offset: u64,
        size: u32,
    },
}
