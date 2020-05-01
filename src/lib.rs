//! FUSE user-space library async version implementation.
//!
//! This is an improved rewrite of the FUSE user-space library (low-level interface) to fully take
//! advantage of Rust's architecture.
//!
//! This library doesn't depend on `libfuse`, unless enable `unprivileged` feature, this feature
//! will support mount the filesystem without root permission by using `fusermount3`.
//!
//! # Features;
//!
//! - `file-lock`: enable POSIX file lock feature.
//! - `async-std-runtime`: use [async_std](https://docs.rs/async-std) runtime.
//! - `tokio-runtime`: use [tokio](https://docs.rs/tokio) runtime.
//! - `unprivileged`: allow mount filesystem without root permission by using `fusermount3`.
//!
//! # Notes:
//!
//! You must enable `async-std-runtime` or `tokio-runtime` feature.

use std::ffi::OsString;
use std::io::Result as IoResult;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::channel::mpsc::UnboundedSender;
use futures::SinkExt;
use nix::sys::stat::mode_t;

/// re-export async_trait.
pub use async_trait::async_trait;
pub use errno::Errno;
pub use filesystem::Filesystem;
pub use helper::perm_from_mode_and_kind;
use lazy_static::lazy_static;
pub use mount_options::MountOptions;
pub use request::Request;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use session::Session;

use crate::abi::{
    fuse_attr, fuse_notify_delete_out, fuse_notify_inval_entry_out, fuse_notify_inval_inode_out,
    fuse_notify_poll_wakeup_out, fuse_notify_retrieve_out, fuse_notify_store_out, fuse_out_header,
    fuse_setattr_in, FATTR_ATIME, FATTR_ATIME_NOW, FATTR_CTIME, FATTR_FH, FATTR_GID,
    FATTR_LOCKOWNER, FATTR_MODE, FATTR_MTIME, FATTR_MTIME_NOW, FATTR_SIZE, FATTR_UID,
    FUSE_NOTIFY_DELETE_OUT_SIZE, FUSE_NOTIFY_INVAL_ENTRY_OUT_SIZE,
    FUSE_NOTIFY_INVAL_INODE_OUT_SIZE, FUSE_NOTIFY_POLL_WAKEUP_OUT_SIZE,
    FUSE_NOTIFY_RETRIEVE_OUT_SIZE, FUSE_NOTIFY_STORE_OUT_SIZE, FUSE_OUT_HEADER_SIZE,
};
use crate::helper::mode_from_kind_and_perm;

mod abi;
mod connection;
mod errno;
mod filesystem;
mod helper;
mod mount_options;
pub mod reply;
mod request;
mod session;
mod spawn;

/// pre-defined Result, the Err type is [`Errno`].
///
/// [`Errno`]: Errno
pub type Result<T> = std::result::Result<T, Errno>;

/// file attributes
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileAttr {
    /// Inode number
    pub ino: u64,
    /// Generation
    pub generation: u64,
    /// Size in bytes
    pub size: u64,
    /// Size in blocks
    pub blocks: u64,
    /// Time of last access
    pub atime: SystemTime,
    /// Time of last modification
    pub mtime: SystemTime,
    /// Time of last change
    pub ctime: SystemTime,
    #[cfg(target_os = "macos")]
    /// Time of creation (macOS only)
    pub crtime: SystemTime,
    /// Kind of file (directory, file, pipe, etc)
    pub kind: FileType,
    /// Permissions
    pub perm: u16,
    /// Number of hard links
    pub nlink: u32,
    /// User id
    pub uid: u32,
    /// Group id
    pub gid: u32,
    /// Rdev
    pub rdev: u32,
    #[cfg(target_os = "macos")]
    /// Flags (macOS only, see chflags(2))
    pub flags: u32,
    pub blksize: u32,
}

impl Into<fuse_attr> for FileAttr {
    fn into(self) -> fuse_attr {
        fuse_attr {
            ino: self.ino,
            size: self.size,
            blocks: self.blocks,
            atime: self
                .atime
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_secs(),
            mtime: self
                .mtime
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_secs(),
            ctime: self
                .ctime
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_secs(),
            atimensec: self
                .atime
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .subsec_nanos(),
            mtimensec: self
                .mtime
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .subsec_nanos(),
            ctimensec: self
                .ctime
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .subsec_nanos(),
            mode: mode_from_kind_and_perm(self.kind, self.perm),
            nlink: self.nlink,
            uid: self.uid,
            gid: self.gid,
            rdev: self.rdev,
            blksize: self.blksize,
            padding: 0,
        }
    }
}

/// File types
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FileType {
    /// Named pipe (S_IFIFO)
    NamedPipe,
    /// Character device (S_IFCHR)
    CharDevice,
    /// Block device (S_IFBLK)
    BlockDevice,
    /// Directory (S_IFDIR)
    Directory,
    /// Regular file (S_IFREG)
    RegularFile,
    /// Symbolic link (S_IFLNK)
    Symlink,
    /// Unix domain socket (S_IFSOCK)
    Socket,
}

impl From<FileType> for mode_t {
    fn from(kind: FileType) -> Self {
        match kind {
            FileType::NamedPipe => libc::S_IFIFO,
            FileType::CharDevice => libc::S_IFCHR,
            FileType::BlockDevice => libc::S_IFBLK,
            FileType::Directory => libc::S_IFDIR,
            FileType::RegularFile => libc::S_IFREG,
            FileType::Symlink => libc::S_IFLNK,
            FileType::Socket => libc::S_IFSOCK,
        }
    }
}

/// the setattr argument.
#[derive(Debug, Clone, Default)]
pub struct SetAttr {
    /// set file or directory mode.
    pub mode: Option<u32>,
    /// set file or directory uid.
    pub uid: Option<u32>,
    /// set file or directory gid.
    pub gid: Option<u32>,
    /// set file or directory size.
    pub size: Option<u64>,
    /// the lock_owner argument.
    pub lock_owner: Option<u64>,
    /// set file or directory atime.
    pub atime: Option<SystemTime>,
    /// set file or directory mtime.
    pub mtime: Option<SystemTime>,
    /// the fh argument.
    pub fh: Option<u64>,
    /// set file or directory atime now.
    pub atime_now: Option<()>,
    /// set file or directory mtime now.
    pub mtime_now: Option<()>,
    /// set file or directory ctime.
    pub ctime: Option<SystemTime>,
    #[cfg(target_os = "macos")]
    pub crtime: Option<SystemTime>,
    #[cfg(target_os = "macos")]
    pub chgtime: Option<SystemTime>,
    #[cfg(target_os = "macos")]
    pub bkuptime: Option<SystemTime>,
    #[cfg(target_os = "macos")]
    pub flags: Option<u32>,
}

impl From<&fuse_setattr_in> for SetAttr {
    fn from(setattr_in: &fuse_setattr_in) -> Self {
        let mut set_attr = Self::default();

        if setattr_in.valid & FATTR_MODE > 0 {
            set_attr.mode = Some(setattr_in.mode);
        }

        if setattr_in.valid & FATTR_UID > 0 {
            set_attr.uid = Some(setattr_in.uid);
        }

        if setattr_in.valid & FATTR_GID > 0 {
            set_attr.gid = Some(setattr_in.gid);
        }

        if setattr_in.valid & FATTR_SIZE > 0 {
            set_attr.size = Some(setattr_in.size);
        }

        if setattr_in.valid & FATTR_ATIME > 0 {
            set_attr.atime =
                Some(UNIX_EPOCH + Duration::new(setattr_in.atime, setattr_in.atimensec));
        }

        if setattr_in.valid & FATTR_ATIME_NOW > 0 {
            set_attr.atime = Some(SystemTime::now());
        }

        if setattr_in.valid & FATTR_MTIME > 0 {
            set_attr.mtime =
                Some(UNIX_EPOCH + Duration::new(setattr_in.mtime, setattr_in.mtimensec));
        }

        if setattr_in.valid & FATTR_MTIME_NOW > 0 {
            set_attr.mtime = Some(SystemTime::now());
        }

        if setattr_in.valid & FATTR_FH > 0 {
            set_attr.fh = Some(setattr_in.fh);
        }

        if setattr_in.valid & FATTR_LOCKOWNER > 0 {
            set_attr.lock_owner = Some(setattr_in.lock_owner);
        }

        if setattr_in.valid & FATTR_CTIME > 0 {
            set_attr.ctime =
                Some(UNIX_EPOCH + Duration::new(setattr_in.ctime, setattr_in.ctimensec));
        }

        set_attr
    }
}

#[derive(Debug)]
pub struct PollNotify {
    sender: UnboundedSender<Vec<u8>>,
}

lazy_static! {
    static ref BINARY: bincode::Config = {
        let mut cfg = bincode::config();
        cfg.little_endian();

        cfg
    };
}

impl PollNotify {
    pub(crate) fn new(sender: UnboundedSender<Vec<u8>>) -> Self {
        Self { sender }
    }

    pub async fn notify(
        &mut self,
        kind: PollNotifyKind,
    ) -> std::result::Result<(), PollNotifyKind> {
        let data = match &kind {
            PollNotifyKind::Wakeup { kh } => {
                let out_header = fuse_out_header {
                    len: (FUSE_OUT_HEADER_SIZE + FUSE_NOTIFY_POLL_WAKEUP_OUT_SIZE) as u32,
                    error: 0,
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
                    error: 0,
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
                    error: 0,
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

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
/// mount the filesystem. This function will block until the filesystem is unmounted.
pub async fn mount<FS, P>(fs: FS, mount_path: P, mount_options: MountOptions) -> IoResult<()>
where
    FS: Filesystem + Send + Sync + 'static,
    P: AsRef<Path>,
{
    Session::mount(fs, mount_path, mount_options).await
}

#[cfg(all(
    any(feature = "async-std-runtime", feature = "tokio-runtime"),
    feature = "unprivileged"
))]
/// mount the filesystem without root permission. This function will block until the filesystem
/// is unmounted.
pub async fn mount_with_unprivileged<FS, P>(
    fs: FS,
    mount_path: P,
    mount_options: MountOptions,
) -> IoResult<()>
where
    FS: Filesystem + Send + Sync + 'static,
    P: AsRef<Path>,
{
    Session::mount_with_unprivileged(fs, mount_path, mount_options).await
}

pub mod prelude {
    pub use crate::reply::*;
    pub use crate::Errno;
    pub use crate::FileAttr;
    pub use crate::FileType;
    pub use crate::Filesystem;
    pub use crate::MountOptions;
    pub use crate::Request;
    pub use crate::Result;
    pub use crate::SetAttr;
}

/*#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}*/
