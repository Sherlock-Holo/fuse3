use std::ffi::OsString;
use std::time::Duration;

use crate::abi::{
    fuse_attr_out, fuse_bmap_out, fuse_entry_out, fuse_kstatfs, fuse_lseek_out, fuse_open_out,
    fuse_poll_out, fuse_statfs_out, fuse_write_out,
};
#[cfg(feature = "file-lock")]
use crate::abi::{fuse_file_lock, fuse_lk_out};
use crate::{FileAttr, FileType};

#[derive(Debug, Default)]
pub struct ReplyEmpty {}

#[derive(Debug)]
pub struct ReplyEntry {
    pub ttl: Duration,
    pub attr: FileAttr,
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
pub struct ReplyAttr {
    pub ttl: Duration,
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

pub struct ReplyData {
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct ReplyOpen {
    pub fh: u64,
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
pub struct ReplyWrite {
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
pub enum ReplyXAttr {
    Size(u32),
    Data(Vec<u8>),
}

#[derive(Debug)]
pub struct DirectoryEntry {
    pub inode: u64,
    /// index is point to next entry, for example, if entry is the first one, the index should be 1
    pub index: u64,
    pub kind: FileType,
    pub name: OsString,
}

pub struct ReplyDirectory {
    pub entries: Box<dyn Iterator<Item = DirectoryEntry> + Send>,
}

#[cfg(feature = "file-lock")]
#[derive(Debug)]
pub struct ReplyLock {
    pub start: u64,
    pub end: u64,
    pub r#type: u32,
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
pub struct ReplyCreated {
    pub ttl: Duration,
    pub attr: FileAttr,
    pub generation: u64,
    pub fh: u64,
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
pub struct ReplyBmap {
    pub block: u64,
}

impl Into<fuse_bmap_out> for ReplyBmap {
    fn into(self) -> fuse_bmap_out {
        fuse_bmap_out { block: self.block }
    }
}

#[derive(Debug)]
pub struct ReplyIoctl {
    pub result: i32,
    pub flags: u32,
    pub in_iovs: u32,
    pub out_iovs: u32,
}

#[derive(Debug)]
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
pub struct DirectoryEntryPlus {
    pub inode: u64,
    pub generation: u64,
    /// index is point to next entry, for example, if entry is the first one, the index should be 1
    pub index: u64,
    pub kind: FileType,
    pub name: OsString,
    pub attr: FileAttr,
    pub entry_ttl: Duration,
    pub attr_ttl: Duration,
}

// use fuse_direntplus
pub struct ReplyDirectoryPlus {
    pub entries: Box<dyn Iterator<Item = DirectoryEntryPlus> + Send>,
}

#[derive(Debug)]
pub struct ReplyLSeek {
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
pub struct ReplyCopyFileRange {
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
