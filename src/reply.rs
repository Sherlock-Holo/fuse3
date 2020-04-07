use std::ffi::OsStr;
use std::time::{Duration, UNIX_EPOCH};

use crate::abi::{fuse_attr, fuse_entry_out};
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
            attr: fuse_attr {
                ino: attr.ino,
                size: attr.size,
                blocks: attr.blocks,
                atime: attr
                    .atime
                    .duration_since(UNIX_EPOCH)
                    .expect("won't early")
                    .as_secs(),
                mtime: attr
                    .mtime
                    .duration_since(UNIX_EPOCH)
                    .expect("won't early")
                    .as_secs(),
                ctime: attr
                    .ctime
                    .duration_since(UNIX_EPOCH)
                    .expect("won't early")
                    .as_secs(),
                atimensec: attr
                    .atime
                    .duration_since(UNIX_EPOCH)
                    .expect("won't early")
                    .subsec_nanos(),
                mtimensec: attr
                    .mtime
                    .duration_since(UNIX_EPOCH)
                    .expect("won't early")
                    .subsec_nanos(),
                ctimensec: attr
                    .ctime
                    .duration_since(UNIX_EPOCH)
                    .expect("won't early")
                    .subsec_nanos(),
                mode: attr.perm as u32,
                nlink: attr.nlink,
                uid: attr.uid,
                gid: attr.gid,
                rdev: attr.rdev,
                blksize: attr.blksize,
                padding: 0,
            },
        }
    }
}

#[derive(Debug)]
pub struct ReplyAttr {
    pub ttl: Duration,
    pub attr: FileAttr,
}

pub struct ReplyData<T: AsRef<[u8]>> {
    pub data: T,
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
pub enum ReplyXAttr<T: AsRef<[u8]>> {
    Size(u32),
    Data(T),
}

#[derive(Debug)]
pub struct DirectoryEntry<S: AsRef<OsStr>> {
    pub inode: u64,
    pub offset: i64,
    pub kind: FileType,
    pub name: S,
}

#[derive(Debug)]
pub struct ReplyDirectory<I, S>
where
    S: AsRef<OsStr>,
    I: IntoIterator<Item = DirectoryEntry<S>>,
{
    pub entries: I,
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
pub struct DirectoryEntryPlus<S: AsRef<OsStr>> {
    pub inode: u64,
    pub generation: u64,
    pub offset: i64,
    pub kind: FileType,
    pub name: S,
    pub attr: FileAttr,
    pub entry_ttl: Duration,
    pub attr_ttl: Duration,
}

// use fuse_direntplus
#[derive(Debug)]
pub struct ReplyDirectoryPlus<I, S>
where
    S: AsRef<OsStr>,
    I: IntoIterator<Item = DirectoryEntryPlus<S>>,
{
    pub entries: I,
}

#[derive(Debug)]
pub struct ReplyLSeek {
    pub offset: u64,
}

#[derive(Debug)]
pub struct ReplyCopyFileRange {
    pub copied: u64,
}
