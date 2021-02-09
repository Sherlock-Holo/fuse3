//! reply structures.
use std::ffi::OsString;
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use futures_util::stream::Stream;

use crate::helper::mode_from_kind_and_perm;
use crate::raw::abi::{
    fuse_attr, fuse_attr_out, fuse_bmap_out, fuse_entry_out, fuse_kstatfs, fuse_lseek_out,
    fuse_open_out, fuse_poll_out, fuse_statfs_out, fuse_write_out,
};
#[cfg(feature = "file-lock")]
use crate::raw::abi::{fuse_file_lock, fuse_lk_out};
use crate::FileType;

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

impl From<FileAttr> for fuse_attr {
    fn from(attr: FileAttr) -> Self {
        fuse_attr {
            ino: attr.ino,
            size: attr.size,
            blocks: attr.blocks,
            atime: attr
                .atime
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_secs(),
            mtime: attr
                .mtime
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_secs(),
            ctime: attr
                .ctime
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_secs(),
            atimensec: attr
                .atime
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .subsec_nanos(),
            mtimensec: attr
                .mtime
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .subsec_nanos(),
            ctimensec: attr
                .ctime
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .subsec_nanos(),
            mode: mode_from_kind_and_perm(attr.kind, attr.perm),
            nlink: attr.nlink,
            uid: attr.uid,
            gid: attr.gid,
            rdev: attr.rdev,
            blksize: attr.blksize,
            padding: 0,
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
    /// the generation.
    pub generation: u64,
}

impl From<ReplyEntry> for fuse_entry_out {
    fn from(entry: ReplyEntry) -> Self {
        let attr = entry.attr;

        fuse_entry_out {
            nodeid: attr.ino,
            generation: entry.generation,
            entry_valid: entry.ttl.as_secs(),
            attr_valid: entry.ttl.as_secs(),
            entry_valid_nsec: entry.ttl.subsec_nanos(),
            attr_valid_nsec: entry.ttl.subsec_nanos(),
            attr: attr.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// reply attr.
pub struct ReplyAttr {
    /// the attribute TTL.
    pub ttl: Duration,
    /// the attribute.
    pub attr: FileAttr,
}

impl From<ReplyAttr> for fuse_attr_out {
    fn from(attr: ReplyAttr) -> Self {
        fuse_attr_out {
            attr_valid: attr.ttl.as_secs(),
            attr_valid_nsec: attr.ttl.subsec_nanos(),
            dummy: 0,
            attr: attr.attr.into(),
        }
    }
}

/// data reply.
pub struct ReplyData {
    /// the data.
    pub data: Bytes,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// open reply.
pub struct ReplyOpen {
    /// the file handle id.
    ///
    /// # Notes:
    ///
    /// if set fh 0, means use stateless IO.
    pub fh: u64,
    /// the flags.
    pub flags: u32,
}

impl From<ReplyOpen> for fuse_open_out {
    fn from(opened: ReplyOpen) -> Self {
        fuse_open_out {
            fh: opened.fh,
            open_flags: opened.flags,
            padding: 0,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// write reply.
pub struct ReplyWrite {
    /// the data written.
    pub written: u64,
}

impl From<ReplyWrite> for fuse_write_out {
    fn from(written: ReplyWrite) -> Self {
        fuse_write_out {
            size: written.written as u32,
            padding: 0,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// statfs reply.
pub struct ReplyStatFs {
    /// the number of blocks in the filesystem.
    pub blocks: u64,
    /// the number of free blocks.
    pub bfree: u64,
    /// the number of free blocks for non-priviledge users.
    pub bavail: u64,
    /// the number of inodes.
    pub files: u64,
    /// the number of free inodes.
    pub ffree: u64,
    /// the block size.
    pub bsize: u32,
    /// the maximum length of file name.
    pub namelen: u32,
    /// the fragment size.
    pub frsize: u32,
}

impl From<ReplyStatFs> for fuse_statfs_out {
    fn from(stat_fs: ReplyStatFs) -> Self {
        fuse_statfs_out {
            st: fuse_kstatfs {
                blocks: stat_fs.blocks,
                bfree: stat_fs.bfree,
                bavail: stat_fs.bavail,
                files: stat_fs.files,
                ffree: stat_fs.ffree,
                bsize: stat_fs.bsize,
                namelen: stat_fs.namelen,
                frsize: stat_fs.frsize,
                padding: 0,
                spare: [0; 6],
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// xattr reply.
pub enum ReplyXAttr {
    Size(u32),
    Data(Bytes),
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// directory entry.
pub struct DirectoryEntry {
    /// entry inode.
    pub inode: u64,
    /// index is point to next entry, for example, if entry is the first one, the index should be 1
    pub index: u64,
    /// entry kind.
    pub kind: FileType,
    /// entry name.
    pub name: OsString,
}

/// readdir reply.
pub struct ReplyDirectory {
    pub entries: Pin<Box<dyn Stream<Item = DirectoryEntry> + Send>>,
}

#[cfg(feature = "file-lock")]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// file lock reply.
pub struct ReplyLock {
    /// starting offset for lock.
    pub start: u64,
    /// end offset for lock.
    pub end: u64,
    /// type of lock, such as: [`F_RDLCK`], [`F_WRLCK`] and [`F_UNLCK`]
    ///
    /// [`F_RDLCK`]: libc::F_RDLCK
    /// [`F_WRLCK`]: libc::F_WRLCK
    /// [`F_UNLCK`]: libc::F_UNLCK
    pub r#type: u32,
    /// PID of process blocking our lock
    pub pid: u32,
}

#[cfg(feature = "file-lock")]
impl From<ReplyLock> for fuse_lk_out {
    fn from(lock: ReplyLock) -> Self {
        fuse_lk_out {
            lk: fuse_file_lock {
                start: lock.start,
                end: lock.end,
                r#type: lock.r#type,
                pid: lock.pid,
            },
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// crate reply.
pub struct ReplyCreated {
    /// the attribute TTL.
    pub ttl: Duration,
    /// the attribute of file.
    pub attr: FileAttr,
    /// the generation of file.
    pub generation: u64,
    /// the file handle.
    pub fh: u64,
    /// the flags.
    pub flags: u32,
}

impl From<ReplyCreated> for (fuse_entry_out, fuse_open_out) {
    fn from(created: ReplyCreated) -> Self {
        let attr = created.attr;

        let entry_out = fuse_entry_out {
            nodeid: attr.ino,
            generation: attr.generation,
            entry_valid: created.ttl.as_secs(),
            attr_valid: created.ttl.as_secs(),
            entry_valid_nsec: created.ttl.subsec_micros(),
            attr_valid_nsec: created.ttl.subsec_micros(),
            attr: attr.into(),
        };

        let open_out = fuse_open_out {
            fh: created.fh,
            open_flags: created.flags,
            padding: 0,
        };

        (entry_out, open_out)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
// TODO need more detail
/// bmap reply.
pub struct ReplyBmap {
    pub block: u64,
}

impl From<ReplyBmap> for fuse_bmap_out {
    fn from(bmap: ReplyBmap) -> Self {
        fuse_bmap_out { block: bmap.block }
    }
}

/*#[derive(Debug)]
pub struct ReplyIoctl {
    pub result: i32,
    pub flags: u32,
    pub in_iovs: u32,
    pub out_iovs: u32,
}*/

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
// TODO need more detail
/// poll reply
pub struct ReplyPoll {
    pub revents: u32,
}

impl From<ReplyPoll> for fuse_poll_out {
    fn from(poll: ReplyPoll) -> Self {
        fuse_poll_out {
            revents: poll.revents,
            padding: 0,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// directory entry with attribute
pub struct DirectoryEntryPlus {
    /// the entry inode.
    pub inode: u64,
    /// the entry generation.
    pub generation: u64,
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
    pub entries: Pin<Box<dyn Stream<Item = DirectoryEntryPlus> + Send>>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// the lseek reply.
pub struct ReplyLSeek {
    /// lseek offset.
    pub offset: u64,
}

impl From<ReplyLSeek> for fuse_lseek_out {
    fn from(seek: ReplyLSeek) -> Self {
        fuse_lseek_out {
            offset: seek.offset,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// copy_file_range reply.
pub struct ReplyCopyFileRange {
    /// data copied size.
    pub copied: u64,
}

impl From<ReplyCopyFileRange> for fuse_write_out {
    fn from(copied: ReplyCopyFileRange) -> Self {
        fuse_write_out {
            size: copied.copied as u32,
            padding: 0,
        }
    }
}
