//! FUSE user-space library async version implementation.
//!
//! This is an improved rewrite of the FUSE user-space library (low-level interface) to fully take
//! advantage of Rust's architecture.
//!
//! This library doesn't depend on `libfuse`, unless enable `unprivileged` feature, this feature
//! will support mount the filesystem without root permission by using `fusermount3` binary.
//!
//! # Features:
//!
//! - `file-lock`: enable POSIX file lock feature.
//! - `async-std-runtime`: use [async_std](https://docs.rs/async-std) runtime.
//! - `tokio-runtime`: use [tokio](https://docs.rs/tokio) runtime.
//! - `unprivileged`: allow mount filesystem without root permission by using `fusermount3`.
//!
//! # Notes:
//!
//! You must enable `async-std-runtime` or `tokio-runtime` feature.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// re-export [`async_trait`].
///
/// [`async_trait`]: async_trait::async_trait
pub use async_trait::async_trait;
use nix::sys::stat::mode_t;

pub use errno::Errno;
pub use filesystem::Filesystem;
pub use helper::perm_from_mode_and_kind;
pub use mount_options::MountOptions;
pub use request::Request;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
/// fuse filesystem session.
pub use session::Session;

use crate::abi::{
    fuse_attr, fuse_setattr_in, FATTR_ATIME, FATTR_ATIME_NOW, FATTR_CTIME, FATTR_GID,
    FATTR_LOCKOWNER, FATTR_MODE, FATTR_MTIME, FATTR_MTIME_NOW, FATTR_SIZE, FATTR_UID,
};
use crate::helper::mode_from_kind_and_perm;

mod abi;
mod connection;
mod errno;
mod filesystem;
mod helper;
mod mount_options;
pub mod notify;
pub mod path;
pub mod reply;
mod request;
mod session;

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
#[derive(Debug, Clone, Default, Eq, PartialEq)]
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

pub mod prelude {
    //! the fuse3 prelude.

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
