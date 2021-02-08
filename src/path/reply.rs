//! reply structures.
use std::ffi::OsString;
use std::io;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures_util::stream::Stream;

use crate::{FileType, Inode};
pub use crate::reply::{
    ReplyBmap, ReplyCopyFileRange, ReplyCreated, ReplyData, ReplyLSeek, ReplyOpen, ReplyPoll,
    ReplyStatFs, ReplyWrite, ReplyXAttr,
};
#[cfg(feature = "file-lock")]
pub use crate::reply::ReplyLock;

/// file attributes
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileAttr {
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

impl Into<crate::FileAttr> for (Inode, FileAttr) {
    fn into(self) -> crate::FileAttr {
        let (inode, attr) = self;

        crate::FileAttr {
            ino: inode,
            generation: 0,
            size: attr.size,
            blocks: attr.blocks,
            atime: attr.atime,
            mtime: attr.mtime,
            ctime: attr.ctime,
            kind: attr.kind,
            perm: attr.perm,
            nlink: attr.nlink,
            uid: attr.uid,
            gid: attr.gid,
            rdev: attr.rdev,
            blksize: attr.blksize,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// entry reply.
pub struct ReplyEntry {
    /// the attribute TTL.
    pub ttl: Duration,
    /// the attribute.
    pub attr: FileAttr,
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// reply attr.
pub struct ReplyAttr {
    /// the attribute TTL.
    pub ttl: Duration,
    /// the attribute.
    pub attr: FileAttr,
}

impl From<Bytes> for ReplyData {
    fn from(data: Bytes) -> Self {
        Self { data }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// directory entry.
pub struct DirectoryEntry {
    /// index is point to next entry, for example, if entry is the first one, the index should be 1
    pub index: u64,
    /// entry kind.
    pub kind: FileType,
    /// entry name.
    pub name: OsString,
}

/// readdir reply.
pub struct ReplyDirectory {
    pub entries: Pin<Box<dyn Stream<Item=io::Result<DirectoryEntry>> + Send>>,
}

/*#[derive(Debug)]
pub struct ReplyIoctl {
    pub result: i32,
    pub flags: u32,
    pub in_iovs: u32,
    pub out_iovs: u32,
}*/

#[derive(Debug, Clone, Eq, PartialEq)]
/// directory entry with attribute
pub struct DirectoryEntryPlus {
    /// index is point to next entry, for example, if entry is the first one, the index should be 1
    pub index: u64,
    /// the entry kind.
    pub kind: FileType,
    /// the entry name.
    pub name: OsString,
    /// the entry attribute.
    pub attr: FileAttr,
    /// the entry TTL.
    pub entry_ttl: Duration,
    /// the attribute TTL.
    pub attr_ttl: Duration,
}

/// the readdirplus reply.
pub struct ReplyDirectoryPlus {
    pub entries: Pin<Box<dyn Stream<Item=io::Result<DirectoryEntryPlus>> + Send>>,
}
