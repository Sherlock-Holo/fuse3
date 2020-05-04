use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_std::sync::RwLock;
use futures_util::stream;
use futures_util::StreamExt;
use log::LevelFilter;

use async_trait::async_trait;
use fuse3::prelude::*;
use fuse3::Session;

const TTL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
enum Entry {
    Dir(Arc<RwLock<Dir>>),
    File(Arc<RwLock<File>>),
}

impl Entry {
    async fn attr(&self) -> FileAttr {
        match self {
            Entry::Dir(dir) => {
                let dir = dir.read().await;

                FileAttr {
                    ino: dir.inode,
                    generation: 0,
                    size: 0,
                    blocks: 0,
                    atime: SystemTime::UNIX_EPOCH,
                    mtime: SystemTime::UNIX_EPOCH,
                    ctime: SystemTime::UNIX_EPOCH,
                    kind: FileType::Directory,
                    perm: fuse3::perm_from_mode_and_kind(FileType::Directory, dir.mode),
                    nlink: 0,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 0,
                }
            }

            Entry::File(file) => {
                let file = file.read().await;

                FileAttr {
                    ino: file.inode,
                    generation: 0,
                    size: file.content.len() as _,
                    blocks: 0,
                    atime: SystemTime::UNIX_EPOCH,
                    mtime: SystemTime::UNIX_EPOCH,
                    ctime: SystemTime::UNIX_EPOCH,
                    kind: FileType::RegularFile,
                    perm: fuse3::perm_from_mode_and_kind(FileType::RegularFile, file.mode),
                    nlink: 0,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 0,
                }
            }
        }
    }

    async fn set_attr(&self, set_attr: SetAttr) -> FileAttr {
        match self {
            Entry::Dir(dir) => {
                let mut dir = dir.write().await;

                if let Some(mode) = set_attr.mode {
                    dir.mode = mode;
                }
            }

            Entry::File(file) => {
                let mut file = file.write().await;

                if let Some(size) = set_attr.size {
                    file.content.truncate(size as _);
                }

                if let Some(mode) = set_attr.mode {
                    file.mode = mode;
                }
            }
        }

        self.attr().await
    }

    fn is_dir(&self) -> bool {
        matches!(self, Entry::Dir(_))
    }

    fn is_file(&self) -> bool {
        !self.is_dir()
    }

    async fn inode(&self) -> u64 {
        match self {
            Entry::Dir(dir) => {
                let dir = dir.read().await;

                dir.inode
            }

            Entry::File(file) => {
                let file = file.read().await;

                file.inode
            }
        }
    }

    fn kind(&self) -> FileType {
        if self.is_dir() {
            FileType::Directory
        } else {
            FileType::RegularFile
        }
    }
}

#[derive(Debug)]
struct Dir {
    inode: u64,
    parent: u64,
    name: OsString,
    children: BTreeMap<OsString, Entry>,
    mode: u32,
}

#[derive(Debug)]
struct File {
    inode: u64,
    parent: u64,
    name: OsString,
    content: Vec<u8>,
    mode: u32,
}

#[derive(Debug)]
struct InnerFs {
    inode_map: BTreeMap<u64, Entry>,
    inode_gen: AtomicU64,
}

#[derive(Debug)]
struct FS(RwLock<InnerFs>);

impl Default for FS {
    fn default() -> Self {
        let root = Entry::Dir(Arc::new(RwLock::new(Dir {
            inode: 1,
            parent: 1,
            name: OsString::from("/"),
            children: BTreeMap::new(),
            mode: 0o755,
        })));

        let mut inode_map = BTreeMap::new();

        inode_map.insert(1, root);

        Self(RwLock::new(InnerFs {
            inode_map,
            inode_gen: AtomicU64::new(2),
        }))
    }
}

#[async_trait]
impl Filesystem for FS {
    async fn init(&self, _req: Request) -> Result<()> {
        Ok(())
    }

    async fn destroy(&self, _req: Request) {}

    async fn lookup(&self, _req: Request, parent: u64, name: &OsStr) -> Result<ReplyEntry> {
        let inner = self.0.read().await;

        let entry = inner
            .inode_map
            .get(&parent)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::Dir(dir) = entry {
            let dir = dir.read().await;

            let attr = dir
                .children
                .get(name)
                .ok_or(Errno::from(libc::ENOENT))?
                .attr()
                .await;

            Ok(ReplyEntry {
                ttl: TTL,
                attr,
                generation: 0,
            })
        } else {
            Err(libc::ENOTDIR.into())
        }
    }

    async fn forget(&self, _req: Request, _inode: u64, _nlookup: u64) {}

    async fn getattr(&self, _req: Request, inode: u64, _fh: u64, _flags: u32) -> Result<ReplyAttr> {
        Ok(ReplyAttr {
            ttl: TTL,
            attr: self
                .0
                .read()
                .await
                .inode_map
                .get(&inode)
                .ok_or(Errno::from(libc::ENOENT))?
                .attr()
                .await,
        })
    }

    async fn setattr(&self, _req: Request, inode: u64, set_attr: SetAttr) -> Result<ReplyAttr> {
        Ok(ReplyAttr {
            ttl: TTL,
            attr: self
                .0
                .read()
                .await
                .inode_map
                .get(&inode)
                .ok_or(Errno::from(libc::ENOENT))?
                .set_attr(set_attr)
                .await,
        })
    }

    async fn mkdir(
        &self,
        _req: Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
    ) -> Result<ReplyEntry> {
        let mut inner = self.0.write().await;

        let entry = inner
            .inode_map
            .get(&parent)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::Dir(dir) = entry {
            let mut dir = dir.write().await;

            if dir.children.get(name).is_some() {
                return Err(libc::EEXIST.into());
            }

            let new_inode = inner.inode_gen.fetch_add(1, Ordering::Relaxed);

            let entry = Entry::Dir(Arc::new(RwLock::new(Dir {
                inode: new_inode,
                parent,
                name: name.to_owned(),
                children: BTreeMap::new(),
                mode,
            })));

            let attr = entry.attr().await;

            dir.children.insert(name.to_os_string(), entry.clone());

            drop(dir); // fix inner can't borrow as mut next line

            inner.inode_map.insert(new_inode, entry);

            Ok(ReplyEntry {
                ttl: TTL,
                attr,
                generation: 0,
            })
        } else {
            Err(libc::ENOTDIR.into())
        }
    }

    async fn unlink(&self, _req: Request, parent: u64, name: &OsStr) -> Result<()> {
        let mut inner = self.0.write().await;

        let entry = inner
            .inode_map
            .get(&parent)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::Dir(dir) = entry {
            let mut dir = dir.write().await;

            if dir
                .children
                .get(name)
                .ok_or(Errno::from(libc::ENOENT))?
                .is_dir()
            {
                return Err(libc::EISDIR.into());
            }

            let inode = dir.children.remove(name).unwrap().inode().await;

            drop(dir); // fix inner can't borrow as mut next line

            inner.inode_map.remove(&inode);

            Ok(())
        } else {
            Err(libc::ENOTDIR.into())
        }
    }

    async fn rmdir(&self, _req: Request, parent: u64, name: &OsStr) -> Result<()> {
        let mut inner = self.0.write().await;

        let entry = inner
            .inode_map
            .get(&parent)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::Dir(dir) = entry {
            let mut dir = dir.write().await;

            if dir
                .children
                .get(name)
                .ok_or(Errno::from(libc::ENOENT))?
                .is_file()
            {
                return Err(libc::ENOTDIR.into());
            }

            let inode = dir.children.remove(name).unwrap().inode().await;

            drop(dir); // fix inner can't borrow as mut next line

            inner.inode_map.remove(&inode);

            Ok(())
        } else {
            Err(libc::ENOTDIR.into())
        }
    }

    async fn rename(
        &self,
        _req: Request,
        parent: u64,
        name: &OsStr,
        new_parent: u64,
        new_name: &OsStr,
    ) -> Result<()> {
        let inner = self.0.read().await;

        let parent_entry = inner
            .inode_map
            .get(&parent)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::Dir(parent_dir) = parent_entry {
            let mut parent_dir = parent_dir.write().await;

            if parent == new_parent {
                let entry = parent_dir
                    .children
                    .remove(name)
                    .ok_or(Errno::from(libc::ENOENT))?;
                parent_dir.children.insert(new_name.to_os_string(), entry);

                return Ok(());
            }

            let new_parent_entry = inner
                .inode_map
                .get(&new_parent)
                .ok_or(Errno::from(libc::ENOENT))?;

            if let Entry::Dir(new_parent_dir) = new_parent_entry {
                let mut new_parent_dir = new_parent_dir.write().await;

                let entry = parent_dir
                    .children
                    .remove(name)
                    .ok_or(Errno::from(libc::ENOENT))?;
                new_parent_dir
                    .children
                    .insert(new_name.to_os_string(), entry);

                return Ok(());
            }
        }

        Err(libc::ENOTDIR.into())
    }

    async fn open(&self, _req: Request, inode: u64, _flags: u32) -> Result<ReplyOpen> {
        let inner = self.0.read().await;

        let entry = inner
            .inode_map
            .get(&inode)
            .ok_or(Errno::from(libc::ENOENT))?;

        if matches!(entry, Entry::File(_)) {
            Ok(ReplyOpen { fh: 0, flags: 0 })
        } else {
            Err(libc::EISDIR.into())
        }
    }

    async fn read(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        offset: u64,
        size: u32,
    ) -> Result<ReplyData> {
        let inner = self.0.read().await;

        let entry = inner
            .inode_map
            .get(&inode)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::File(file) = entry {
            let file = file.read().await;

            if file.content.len() <= offset as _ {
                return Ok(ReplyData { data: vec![] });
            }

            let mut data = &file.content[offset as _..];

            if data.len() > size as _ {
                data = &data[..size as _];
            }

            Ok(ReplyData {
                data: data.to_vec(),
            })
        } else {
            Err(libc::EISDIR.into())
        }
    }

    async fn write(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        offset: u64,
        data: &[u8],
        _flags: u32,
    ) -> Result<ReplyWrite> {
        let inner = self.0.read().await;

        let entry = inner
            .inode_map
            .get(&inode)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::File(file) = entry {
            let mut file = file.write().await;

            if file.content.len() > offset as _ {
                let mut content = &mut file.content[offset as _..];

                if content.len() > data.len() {
                    io::copy(&mut (&data[..]), &mut content).unwrap();

                    return Ok(ReplyWrite {
                        written: data.len() as _,
                    });
                }

                let n = io::copy(&mut (&data[..content.len()]), &mut content).unwrap();

                file.content.extend_from_slice(&data[n as _..]);

                Ok(ReplyWrite {
                    written: data.len() as _,
                })
            } else {
                let new_len = file.content.len() + (offset as usize);
                file.content.resize(new_len, 0);

                file.content.extend_from_slice(&data);

                Ok(ReplyWrite {
                    written: data.len() as _,
                })
            }
        } else {
            Err(libc::EISDIR.into())
        }
    }

    async fn release(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> Result<()> {
        Ok(())
    }

    async fn fsync(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> Result<()> {
        Ok(())
    }

    async fn flush(&self, _req: Request, _inode: u64, _fh: u64, _lock_owner: u64) -> Result<()> {
        Ok(())
    }

    async fn readdir(
        &self,
        _req: Request,
        parent: u64,
        _fh: u64,
        offset: i64,
    ) -> Result<ReplyDirectory> {
        let inner = self.0.read().await;

        let entry = inner
            .inode_map
            .get(&parent)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::Dir(dir) = entry {
            let dir = dir.read().await;

            let pre_children = stream::iter(
                vec![
                    (dir.inode, FileType::Directory, OsString::from(".")),
                    (dir.parent, FileType::Directory, OsString::from("..")),
                ]
                .into_iter(),
            );

            let children = pre_children
                .chain(
                    stream::iter(dir.children.iter()).filter_map(|(name, entry)| async move {
                        Some((entry.inode().await, entry.kind(), name.to_os_string()))
                    }),
                )
                .enumerate()
                .map(|(index, (inode, kind, name))| DirectoryEntry {
                    inode,
                    index: index as u64 + 1,
                    kind,
                    name,
                })
                .skip(offset as _)
                .collect::<Vec<_>>()
                .await;

            Ok(ReplyDirectory {
                entries: Box::new(children.into_iter()),
            })
        } else {
            Err(libc::ENOTDIR.into())
        }
    }

    async fn access(&self, _req: Request, _inode: u64, _mask: u32) -> Result<()> {
        Ok(())
    }

    async fn create(
        &self,
        _req: Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> Result<ReplyCreated> {
        let mut inner = self.0.write().await;

        let entry = inner
            .inode_map
            .get(&parent)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::Dir(dir) = entry {
            let mut dir = dir.write().await;

            if dir.children.get(name).is_some() {
                return Err(libc::EEXIST.into());
            }

            let new_inode = inner.inode_gen.fetch_add(1, Ordering::Relaxed);

            let entry = Entry::File(Arc::new(RwLock::new(File {
                inode: new_inode,
                parent,
                name: name.to_os_string(),
                content: vec![],
                mode,
            })));

            let attr = entry.attr().await;

            dir.children.insert(name.to_os_string(), entry.clone());

            drop(dir);

            inner.inode_map.insert(new_inode, entry);

            Ok(ReplyCreated {
                ttl: TTL,
                attr,
                generation: 0,
                fh: 0,
                flags,
            })
        } else {
            Err(libc::ENOTDIR.into())
        }
    }

    async fn interrupt(&self, _req: Request, _unique: u64) -> Result<()> {
        Ok(())
    }

    async fn fallocate(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        offset: u64,
        length: u64,
        _mode: u32,
    ) -> Result<()> {
        let inner = self.0.read().await;

        let entry = inner
            .inode_map
            .get(&inode)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::File(file) = entry {
            let mut file = file.write().await;

            let new_size = (offset + length) as usize;

            let size = file.content.len();

            if new_size > size {
                file.content.reserve(new_size - size);
            } else {
                file.content.truncate(new_size);
            }

            Ok(())
        } else {
            Err(libc::EISDIR.into())
        }
    }

    async fn readdirplus(
        &self,
        _req: Request,
        parent: u64,
        _fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus> {
        let inner = self.0.read().await;

        let entry = inner
            .inode_map
            .get(&parent)
            .ok_or(Errno::from(libc::ENOENT))?;

        if let Entry::Dir(dir) = entry {
            let attr = entry.attr().await;

            let dir = dir.read().await;

            let parent_attr = if dir.parent == dir.inode {
                attr
            } else {
                inner
                    .inode_map
                    .get(&dir.parent)
                    .expect("dir parent not exist")
                    .attr()
                    .await
            };

            let pre_children = stream::iter(
                vec![
                    (dir.inode, FileType::Directory, OsString::from("."), attr),
                    (
                        dir.parent,
                        FileType::Directory,
                        OsString::from(".."),
                        parent_attr,
                    ),
                ]
                .into_iter(),
            );

            let children = pre_children
                .chain(
                    stream::iter(dir.children.iter()).filter_map(|(name, entry)| async move {
                        Some((
                            entry.inode().await,
                            entry.kind(),
                            name.to_os_string(),
                            entry.attr().await,
                        ))
                    }),
                )
                .enumerate()
                .map(|(index, (inode, kind, name, attr))| DirectoryEntryPlus {
                    inode,
                    generation: 0,
                    index: index as u64 + 1,
                    kind,
                    name,
                    attr,
                    entry_ttl: TTL,
                    attr_ttl: TTL,
                })
                .skip(offset as _)
                .collect::<Vec<_>>()
                .await;

            Ok(ReplyDirectoryPlus {
                entries: Box::new(children.into_iter()),
            })
        } else {
            Err(libc::ENOTDIR.into())
        }
    }

    async fn rename2(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        new_parent: u64,
        new_name: &OsStr,
        _flags: u32,
    ) -> Result<()> {
        self.rename(req, parent, name, new_parent, new_name).await
    }

    async fn lseek(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        offset: u64,
        whence: u32,
    ) -> Result<ReplyLSeek> {
        let inner = self.0.read().await;

        let entry = inner
            .inode_map
            .get(&inode)
            .ok_or(Errno::from(libc::ENOENT))?;

        let whence = whence as i32;

        if let Entry::File(file) = entry {
            let offset = if whence == libc::SEEK_CUR || whence == libc::SEEK_SET {
                offset
            } else if whence == libc::SEEK_END {
                let content_size = file.read().await.content.len();

                if content_size >= offset as _ {
                    content_size as u64 - offset
                } else {
                    0
                }
            } else {
                return Err(libc::EINVAL.into());
            };

            Ok(ReplyLSeek { offset })
        } else {
            Err(libc::EISDIR.into())
        }
    }

    async fn copy_file_range(
        &self,
        req: Request,
        inode: u64,
        fh_in: u64,
        off_in: u64,
        inode_out: u64,
        fh_out: u64,
        off_out: u64,
        length: u64,
        flags: u64,
    ) -> Result<ReplyCopyFileRange> {
        let data = self.read(req, inode, fh_in, off_in, length as _).await?;

        let ReplyWrite { written } = self
            .write(req, inode_out, fh_out, off_out, &data.data, flags as _)
            .await?;

        Ok(ReplyCopyFileRange { copied: written })
    }
}

fn log_init() {
    pretty_env_logger::formatted_timed_builder()
        .filter_level(LevelFilter::Debug)
        .init();
}

#[async_std::main]
async fn main() {
    log_init();

    let args = env::args_os().skip(1).take(1).collect::<Vec<_>>();

    let mount_path = args.first();

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mount_options = MountOptions::default().allow_other(true).uid(uid).gid(gid);

    let mount_path = mount_path.expect("no mount point specified");
    Session::new(mount_options)
        .mount(FS::default(), mount_path)
        .await
        .unwrap();
}
