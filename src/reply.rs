//! reply structures.
use std::ffi::OsString;
use std::pin::Pin;
use std::time::Duration;

use futures_util::stream::Stream;

use crate::abi::{
    fuse_attr_out, fuse_bmap_out, fuse_entry_out, fuse_kstatfs, fuse_lseek_out, fuse_open_out,
    fuse_poll_out, fuse_statfs_out, fuse_write_out,
};
#[cfg(feature = "file-lock")]
use crate::abi::{fuse_file_lock, fuse_lk_out};
use crate::{FileAttr, FileType};

#[derive(Debug)]
/// entry reply.
pub struct ReplyEntry {
    /// the attribute TTL.
    pub ttl: Duration,
    /// the attribute.
    pub attr: FileAttr,
    /// the generation.
    pub generation: u64,
}

impl Into<fuse_entry_out> for ReplyEntry {
    fn into(self) -> fuse_entry_out {
        let attr = self.attr;

        fuse_entry_out {
            nodeid: attr.ino,
            generation: self.generation,
            entry_valid: self.ttl.as_secs(),
            attr_valid: self.ttl.as_secs(),
            entry_valid_nsec: self.ttl.subsec_nanos(),
            attr_valid_nsec: self.ttl.subsec_nanos(),
            attr: attr.into(),
        }
    }
}

#[derive(Debug)]
/// reply attr.
pub struct ReplyAttr {
    /// the attribute TTL.
    pub ttl: Duration,
    /// the attribute.
    pub attr: FileAttr,
}

impl Into<fuse_attr_out> for ReplyAttr {
    fn into(self) -> fuse_attr_out {
        fuse_attr_out {
            attr_valid: self.ttl.as_secs(),
            attr_valid_nsec: self.ttl.subsec_nanos(),
            dummy: 0,
            attr: self.attr.into(),
        }
    }
}

/// data reply.
pub struct ReplyData {
    /// the data.
    pub data: Vec<u8>,
}

#[derive(Debug)]
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

impl Into<fuse_open_out> for ReplyOpen {
    fn into(self) -> fuse_open_out {
        fuse_open_out {
            fh: self.fh,
            open_flags: self.flags,
            padding: 0,
        }
    }
}

#[derive(Debug)]
/// write reply.
pub struct ReplyWrite {
    /// the data written.
    pub written: u64,
}

impl Into<fuse_write_out> for ReplyWrite {
    fn into(self) -> fuse_write_out {
        fuse_write_out {
            size: self.written as u32,
            padding: 0,
        }
    }
}

#[derive(Debug)]
// TODO need more detail.
/// statfs reply.
pub struct ReplyStatFs {
    pub blocks: u64,
    pub bfree: u64,
    pub bavail: u64,
    pub files: u64,
    pub ffree: u64,
    pub bsize: u32,
    pub namelen: u32,
    pub frsize: u32,
}

impl Into<fuse_statfs_out> for ReplyStatFs {
    fn into(self) -> fuse_statfs_out {
        fuse_statfs_out {
            st: fuse_kstatfs {
                blocks: self.blocks,
                bfree: self.bfree,
                bavail: self.bavail,
                files: self.files,
                ffree: self.ffree,
                bsize: self.bsize,
                namelen: self.namelen,
                frsize: self.frsize,
                padding: 0,
                spare: [0; 6],
            },
        }
    }
}

#[derive(Debug)]
/// xattr reply.
pub enum ReplyXAttr {
    Size(u32),
    Data(Vec<u8>),
}

#[derive(Debug)]
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
#[derive(Debug)]
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
impl Into<fuse_lk_out> for ReplyLock {
    fn into(self) -> fuse_lk_out {
        fuse_lk_out {
            lk: fuse_file_lock {
                start: self.start,
                end: self.end,
                r#type: self.r#type,
                pid: self.pid,
            },
        }
    }
}

#[derive(Debug)]
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

impl Into<(fuse_entry_out, fuse_open_out)> for ReplyCreated {
    fn into(self) -> (fuse_entry_out, fuse_open_out) {
        let attr = self.attr;

        let entry_out = fuse_entry_out {
            nodeid: attr.ino,
            generation: attr.generation,
            entry_valid: self.ttl.as_secs(),
            attr_valid: self.ttl.as_secs(),
            entry_valid_nsec: self.ttl.subsec_micros(),
            attr_valid_nsec: self.ttl.subsec_micros(),
            attr: attr.into(),
        };

        let open_out = fuse_open_out {
            fh: self.fh,
            open_flags: self.flags,
            padding: 0,
        };

        (entry_out, open_out)
    }
}

#[derive(Debug)]
// TODO need more detail
/// bmap reply.
pub struct ReplyBmap {
    pub block: u64,
}

impl Into<fuse_bmap_out> for ReplyBmap {
    fn into(self) -> fuse_bmap_out {
        fuse_bmap_out { block: self.block }
    }
}

/*#[derive(Debug)]
pub struct ReplyIoctl {
    pub result: i32,
    pub flags: u32,
    pub in_iovs: u32,
    pub out_iovs: u32,
}*/

#[derive(Debug)]
// TODO need more detail
/// poll reply
pub struct ReplyPoll {
    pub revents: u32,
}

impl Into<fuse_poll_out> for ReplyPoll {
    fn into(self) -> fuse_poll_out {
        fuse_poll_out {
            revents: self.revents,
            padding: 0,
        }
    }
}

#[derive(Debug)]
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

// use fuse_direntplus
/// the readdirplus reply.
pub struct ReplyDirectoryPlus {
    pub entries: Pin<Box<dyn Stream<Item = DirectoryEntryPlus> + Send>>,
}

#[derive(Debug)]
/// the lseek reply.
pub struct ReplyLSeek {
    /// lseek offset.
    pub offset: u64,
}

impl Into<fuse_lseek_out> for ReplyLSeek {
    fn into(self) -> fuse_lseek_out {
        fuse_lseek_out {
            offset: self.offset,
        }
    }
}

#[derive(Debug)]
/// copy_file_range reply.
pub struct ReplyCopyFileRange {
    /// data copied size.
    pub copied: u64,
}

impl Into<fuse_write_out> for ReplyCopyFileRange {
    fn into(self) -> fuse_write_out {
        fuse_write_out {
            size: self.copied as u32,
            padding: 0,
        }
    }
}
