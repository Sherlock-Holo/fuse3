use std::ffi::OsString;
use std::time::Duration;

use crate::abi::fuse_entry_out;
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

pub struct ReplyData {
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct ReplyOpen {
    pub fh: u64,
    pub flags: u32,
}

#[derive(Debug)]
pub struct ReplyWrite {
    pub written: u64,
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

#[derive(Debug)]
pub enum ReplyXAttr {
    Size(u32),
    Data(Vec<u8>),
}

#[derive(Debug)]
pub struct DirectoryEntry {
    pub inode: u64,
    pub offset: i64,
    pub kind: FileType,
    pub name: OsString,
}

pub struct ReplyDirectory {
    pub entries: Box<dyn Iterator<Item = DirectoryEntry>>,
}

#[derive(Debug)]
pub struct ReplyLock {
    pub start: u64,
    pub end: u64,
    pub r#type: u32,
    pub pid: u32,
}

#[derive(Debug)]
pub struct ReplyCreated {
    pub ttl: Duration,
    pub attr: FileAttr,
    pub generation: u64,
    pub fh: u64,
    pub flags: u32,
}

#[derive(Debug)]
pub struct ReplyBmap {
    pub block: u64,
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

#[derive(Debug)]
pub struct DirectoryEntryPlus {
    pub inode: u64,
    pub generation: u64,
    pub offset: i64,
    pub kind: FileType,
    pub name: OsString,
    pub attr: FileAttr,
    pub entry_ttl: Duration,
    pub attr_ttl: Duration,
}

// use fuse_direntplus
pub struct ReplyDirectoryPlus {
    pub entries: Box<dyn Iterator<Item = DirectoryEntryPlus>>,
}

#[derive(Debug)]
pub struct ReplyLSeek {
    pub offset: u64,
}

#[derive(Debug)]
pub struct ReplyCopyFileRange {
    pub copied: u64,
}
