use std::io::Result as IoResult;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nix::sys::stat::mode_t;

pub use errno::Errno;
pub use filesystem::Filesystem;
pub use helper::perm_from_mode_and_kind;
pub use mount_option::MountOption;
pub use request::Request;
#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
use session::Session;

use crate::abi::{
    fuse_attr, fuse_setattr_in, FATTR_ATIME, FATTR_CTIME, FATTR_FH, FATTR_GID, FATTR_LOCKOWNER,
    FATTR_MODE, FATTR_MTIME, FATTR_SIZE, FATTR_UID,
};
use crate::helper::mode_from_kind_and_perm;

mod abi;
mod connection;
mod errno;
mod filesystem;
mod helper;
mod mount_option;
pub mod reply;
mod request;
mod session;
mod spawn;

pub type Result<T> = std::result::Result<T, Errno>;

/// File attributes
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
                .expect("won't early")
                .as_secs(),
            mtime: self
                .mtime
                .duration_since(UNIX_EPOCH)
                .expect("won't early")
                .as_secs(),
            ctime: self
                .ctime
                .duration_since(UNIX_EPOCH)
                .expect("won't early")
                .as_secs(),
            atimensec: self
                .atime
                .duration_since(UNIX_EPOCH)
                .expect("won't early")
                .subsec_nanos(),
            mtimensec: self
                .mtime
                .duration_since(UNIX_EPOCH)
                .expect("won't early")
                .subsec_nanos(),
            ctimensec: self
                .ctime
                .duration_since(UNIX_EPOCH)
                .expect("won't early")
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

#[derive(Debug, Clone, Default)]
pub struct SetAttr {
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub size: Option<u64>,
    pub lock_owner: Option<u64>,
    pub atime: Option<SystemTime>,
    pub mtime: Option<SystemTime>,
    pub fh: Option<u64>,
    pub atime_now: Option<SystemTime>,
    pub mtime_now: Option<SystemTime>,
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

        if setattr_in.valid & FATTR_MTIME > 0 {
            set_attr.mtime =
                Some(UNIX_EPOCH + Duration::new(setattr_in.mtime, setattr_in.mtimensec));
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

#[cfg(any(feature = "async-std-runtime", feature = "tokio-runtime"))]
pub async fn mount<FS, P>(fs: FS, mount_path: P, mount_option: MountOption) -> IoResult<()>
where
    FS: Filesystem + Send + Sync + 'static,
    P: AsRef<Path>,
{
    Session::mount(fs, mount_path, mount_option).await
}

pub mod prelude {
    pub use crate::reply::*;
    pub use crate::Errno;
    pub use crate::FileAttr;
    pub use crate::FileType;
    pub use crate::Filesystem;
    pub use crate::MountOption;
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
