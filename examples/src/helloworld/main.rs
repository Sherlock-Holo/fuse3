use std::env;
use std::ffi::{OsStr, OsString};
use std::time::{Duration, SystemTime};

use log::LevelFilter;

use async_trait::async_trait;
use fuse3::prelude::*;

const CONTENT: &str = "hello world\n";

const PARENT_INODE: u64 = 1;
const FILE_INODE: u64 = 2;
const FILE_NAME: &str = "hello-world.txt";
const PARENT_MODE: u16 = 0o755;
const FILE_MODE: u16 = 0o644;
const TTL: Duration = Duration::from_secs(1);

struct HelloWorld;

#[async_trait]
impl Filesystem for HelloWorld {
    async fn init(&self, _req: Request) -> Result<()> {
        Ok(())
    }

    async fn destroy(&self, _req: Request) {}

    async fn lookup(&self, _req: Request, parent: u64, name: &OsStr) -> Result<ReplyEntry> {
        if parent != PARENT_INODE {
            return Err(libc::ENOENT.into());
        }

        if name != OsStr::new(FILE_NAME) {
            return Err(libc::ENOENT.into());
        }

        Ok(ReplyEntry {
            ttl: TTL,
            attr: FileAttr {
                ino: FILE_INODE,
                generation: 0,
                size: CONTENT.len() as u64,
                blocks: 0,
                atime: SystemTime::now(),
                mtime: SystemTime::now(),
                ctime: SystemTime::now(),
                kind: FileType::RegularFile,
                perm: FILE_MODE,
                nlink: 0,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 0,
            },
            generation: 0,
        })
    }

    async fn getattr(&self, _req: Request, inode: u64, _fh: u64, _flags: u32) -> Result<ReplyAttr> {
        if inode == PARENT_INODE {
            Ok(ReplyAttr {
                ttl: TTL,
                attr: FileAttr {
                    ino: PARENT_INODE,
                    generation: 0,
                    size: 0,
                    blocks: 0,
                    atime: SystemTime::now(),
                    mtime: SystemTime::now(),
                    ctime: SystemTime::now(),
                    kind: FileType::Directory,
                    perm: PARENT_MODE,
                    nlink: 0,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 0,
                },
            })
        } else if inode == FILE_INODE {
            Ok(ReplyAttr {
                ttl: TTL,
                attr: FileAttr {
                    ino: FILE_INODE,
                    generation: 0,
                    size: CONTENT.len() as _,
                    blocks: 0,
                    atime: SystemTime::now(),
                    mtime: SystemTime::now(),
                    ctime: SystemTime::now(),
                    kind: FileType::RegularFile,
                    perm: FILE_MODE,
                    nlink: 0,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 0,
                },
            })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn open(&self, _req: Request, inode: u64, flags: u32) -> Result<ReplyOpen> {
        if inode != PARENT_INODE && inode != FILE_INODE {
            return Err(libc::ENOENT.into());
        }

        Ok(ReplyOpen { fh: 0, flags })
    }

    async fn read(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        offset: u64,
        size: u32,
    ) -> Result<ReplyData> {
        if inode != FILE_INODE {
            return Err(libc::ENOENT.into());
        }

        if offset as usize >= CONTENT.len() {
            Ok(ReplyData { data: vec![] })
        } else {
            let mut data = &CONTENT.as_bytes()[offset as usize..];

            if data.len() > size as usize {
                data = &data[..size as usize];
            }

            Ok(ReplyData {
                data: data.to_vec(),
            })
        }
    }

    async fn readdir(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        offset: i64,
    ) -> Result<ReplyDirectory> {
        if inode == FILE_INODE {
            return Err(libc::ENOTDIR.into());
        }

        if inode != PARENT_INODE {
            return Err(libc::ENOENT.into());
        }

        let entries = vec![
            DirectoryEntry {
                inode: PARENT_INODE,
                index: 1,
                kind: FileType::Directory,
                name: OsString::from("."),
            },
            DirectoryEntry {
                inode: PARENT_INODE,
                index: 2,
                kind: FileType::Directory,
                name: OsString::from(".."),
            },
            DirectoryEntry {
                inode: FILE_INODE,
                index: 3,
                kind: FileType::RegularFile,
                name: OsString::from(FILE_NAME),
            },
        ];

        Ok(ReplyDirectory {
            entries: Box::new(entries.into_iter().skip(offset as usize)),
        })
    }

    async fn access(&self, _req: Request, inode: u64, _mask: u32) -> Result<()> {
        if inode != PARENT_INODE && inode != FILE_INODE {
            return Err(libc::ENOENT.into());
        }

        Ok(())
    }

    async fn readdirplus(
        &self,
        _req: Request,
        parent: u64,
        _fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus> {
        if parent == FILE_INODE {
            return Err(libc::ENOTDIR.into());
        }

        if parent != PARENT_INODE {
            return Err(libc::ENOENT.into());
        }

        let entries = vec![
            DirectoryEntryPlus {
                inode: PARENT_INODE,
                generation: 0,
                index: 1,
                kind: FileType::Directory,
                name: OsString::from("."),
                attr: FileAttr {
                    ino: PARENT_INODE,
                    generation: 0,
                    size: 0,
                    blocks: 0,
                    atime: SystemTime::now(),
                    mtime: SystemTime::now(),
                    ctime: SystemTime::now(),
                    kind: FileType::Directory,
                    perm: PARENT_MODE,
                    nlink: 0,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 0,
                },
                entry_ttl: TTL,
                attr_ttl: TTL,
            },
            DirectoryEntryPlus {
                inode: PARENT_INODE,
                generation: 0,
                index: 2,
                kind: FileType::Directory,
                name: OsString::from(".."),
                attr: FileAttr {
                    ino: PARENT_INODE,
                    generation: 0,
                    size: 0,
                    blocks: 0,
                    atime: SystemTime::now(),
                    mtime: SystemTime::now(),
                    ctime: SystemTime::now(),
                    kind: FileType::Directory,
                    perm: PARENT_MODE,
                    nlink: 0,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 0,
                },
                entry_ttl: TTL,
                attr_ttl: TTL,
            },
            DirectoryEntryPlus {
                inode: FILE_INODE,
                generation: 0,
                index: 3,
                kind: FileType::Directory,
                name: OsString::from(FILE_NAME),
                attr: FileAttr {
                    ino: FILE_INODE,
                    generation: 0,
                    size: CONTENT.len() as _,
                    blocks: 0,
                    atime: SystemTime::now(),
                    mtime: SystemTime::now(),
                    ctime: SystemTime::now(),
                    kind: FileType::RegularFile,
                    perm: FILE_MODE,
                    nlink: 0,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 0,
                },
                entry_ttl: TTL,
                attr_ttl: TTL,
            },
        ];

        Ok(ReplyDirectoryPlus {
            entries: Box::new(entries.into_iter().skip(offset as usize)),
        })
    }
}

#[async_std::main]
async fn main() {
    log_init();

    let args = env::args_os().skip(1).take(1).collect::<Vec<_>>();

    let mount_path = args.first();

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mount_options = MountOptions::default()
        .allow_other(true)
        .uid(uid)
        .gid(gid)
        .read_only(true);

    if let Some(mount_path) = mount_path {
        fuse3::mount_with_unprivileged(HelloWorld {}, mount_path, mount_options)
            .await
            .unwrap()
    } else {
        panic!("no mount point specified")
    }
}

fn log_init() {
    pretty_env_logger::formatted_timed_builder()
        .filter_level(LevelFilter::Debug)
        .init();
}
