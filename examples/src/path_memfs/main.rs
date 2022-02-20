use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{Cursor, Read, Write};
use std::time::{Duration, SystemTime};
use std::vec::IntoIter;

use async_trait::async_trait;
use bytes::{Buf, BufMut, BytesMut};
use fuse3::path::prelude::*;
use fuse3::{Errno, MountOptions, Result};
use futures_util::stream::{Empty, Iter};
use futures_util::{stream, StreamExt};
use libc::mode_t;
use tokio::sync::RwLock;
use tracing::{debug, Level};

const TTL: Duration = Duration::from_secs(1);
const SEPARATOR: char = '/';

#[derive(Debug)]
enum Entry {
    Dir(Dir),
    File(File),
}

impl Entry {
    fn attr(&self) -> FileAttr {
        match self {
            Entry::Dir(dir) => FileAttr {
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
            },

            Entry::File(file) => FileAttr {
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
            },
        }
    }

    fn set_attr(&mut self, set_attr: SetAttr) -> FileAttr {
        match self {
            Entry::Dir(dir) => {
                if let Some(mode) = set_attr.mode {
                    dir.mode = mode;
                }
            }

            Entry::File(file) => {
                if let Some(size) = set_attr.size {
                    file.content.truncate(size as _);
                }

                if let Some(mode) = set_attr.mode {
                    file.mode = mode;
                }
            }
        }

        self.attr()
    }

    fn is_dir(&self) -> bool {
        matches!(self, Entry::Dir(_))
    }

    fn is_file(&self) -> bool {
        !self.is_dir()
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
    name: OsString,
    children: BTreeMap<OsString, Entry>,
    mode: mode_t,
}

#[derive(Debug)]
struct File {
    name: OsString,
    content: BytesMut,
    mode: mode_t,
}

#[derive(Debug)]
struct InnerFs {
    root: Entry,
}

#[derive(Debug)]
struct Fs(RwLock<InnerFs>);

impl Fs {
    pub fn new() -> Self {
        Self(RwLock::new(InnerFs {
            root: Entry::Dir(Dir {
                name: OsString::from("/"),
                children: Default::default(),
                mode: 0o755,
            }),
        }))
    }
}

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PathFilesystem for Fs {
    type DirEntryStream = Empty<Result<DirectoryEntry>>;
    type DirEntryPlusStream = Iter<IntoIter<Result<DirectoryEntryPlus>>>;

    async fn init(&self, _req: Request) -> Result<()> {
        Ok(())
    }

    async fn destroy(&self, _req: Request) {}

    async fn lookup(&self, _req: Request, parent: &OsStr, name: &OsStr) -> Result<ReplyEntry> {
        let parent = parent.to_string_lossy();
        let name = name.to_string_lossy();
        let mut paths = split_path(&parent);
        paths.push(name.as_ref());

        let mut entry = &self.0.read().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        Ok(ReplyEntry {
            ttl: TTL,
            attr: entry.attr(),
        })
    }

    async fn forget(&self, _req: Request, _parent: &OsStr, _nlookup: u64) {}

    async fn getattr(
        &self,
        _req: Request,
        path: Option<&OsStr>,
        _fh: Option<u64>,
        _flags: u32,
    ) -> Result<ReplyAttr> {
        let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();

        debug!("get attr path {}", path);

        let paths = split_path(&path);

        let mut entry = &self.0.read().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        Ok(ReplyAttr {
            ttl: TTL,
            attr: entry.attr(),
        })
    }

    async fn setattr(
        &self,
        _req: Request,
        path: Option<&OsStr>,
        _fh: Option<u64>,
        set_attr: SetAttr,
    ) -> Result<ReplyAttr> {
        let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
        let paths = split_path(&path);

        let mut entry = &mut self.0.write().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get_mut(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        Ok(ReplyAttr {
            ttl: TTL,
            attr: entry.set_attr(set_attr),
        })
    }

    async fn mkdir(
        &self,
        _req: Request,
        parent: &OsStr,
        name: &OsStr,
        mode: u32,
        _umask: u32,
    ) -> Result<ReplyEntry> {
        let path = parent.to_string_lossy();
        let paths = split_path(&path);

        let mut entry = &mut self.0.write().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get_mut(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        if let Entry::Dir(dir) = entry {
            if dir.children.contains_key(name) {
                return Err(Errno::new_exist());
            }

            let entry = Entry::Dir(Dir {
                name: name.to_owned(),
                children: Default::default(),
                mode: mode as mode_t,
            });
            let attr = entry.attr();

            dir.children.insert(name.to_owned(), entry);

            Ok(ReplyEntry { ttl: TTL, attr })
        } else {
            Err(Errno::new_is_not_dir())
        }
    }

    async fn unlink(&self, _req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
        let path = parent.to_string_lossy();
        let paths = split_path(&path);

        let mut entry = &mut self.0.write().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get_mut(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        if let Entry::Dir(dir) = entry {
            if dir
                .children
                .get(name)
                .ok_or_else(Errno::new_not_exist)?
                .is_dir()
            {
                return Err(Errno::new_is_dir());
            }

            dir.children.remove(name);

            Ok(())
        } else {
            Err(Errno::new_is_not_dir())
        }
    }

    async fn rmdir(&self, _req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
        let path = parent.to_string_lossy();
        let paths = split_path(&path);

        let mut entry = &mut self.0.write().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get_mut(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        if let Entry::Dir(dir) = entry {
            let child_dir = dir.children.get(name).ok_or_else(Errno::new_not_exist)?;
            if let Entry::Dir(child_dir) = child_dir {
                if !child_dir.children.is_empty() {
                    return Err(Errno::from(libc::ENOTEMPTY));
                }
            } else {
                return Err(Errno::new_is_not_dir());
            }

            dir.children.remove(name);

            Ok(())
        } else {
            Err(Errno::new_is_not_dir())
        }
    }

    async fn rename(
        &self,
        _req: Request,
        origin_parent: &OsStr,
        origin_name: &OsStr,
        parent: &OsStr,
        name: &OsStr,
    ) -> Result<()> {
        let origin_parent = origin_parent.to_string_lossy();
        let origin_parent_paths = split_path(&origin_parent);

        let inner_fs = &mut *self.0.write().await;
        let mut origin_parent_entry = &inner_fs.root;

        for path in &origin_parent_paths {
            if let Entry::Dir(dir) = origin_parent_entry {
                origin_parent_entry = dir
                    .children
                    .get(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        if origin_parent_entry.is_file() {
            return Err(Errno::new_is_not_dir());
        }

        let mut parent_entry = &inner_fs.root;

        let parent = parent.to_string_lossy();
        let parent_paths = split_path(&parent);

        for path in &parent_paths {
            if let Entry::Dir(dir) = parent_entry {
                parent_entry = dir
                    .children
                    .get(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        if parent_entry.is_file() {
            return Err(Errno::new_is_not_dir());
        }

        let mut origin_parent_entry = &mut inner_fs.root;

        for path in origin_parent_paths {
            if let Entry::Dir(dir) = origin_parent_entry {
                origin_parent_entry = dir.children.get_mut(OsStr::new(path)).unwrap();
            } else {
                unreachable!()
            }
        }

        let entry = if let Entry::Dir(dir) = origin_parent_entry {
            dir.children
                .remove(origin_name)
                .ok_or_else(Errno::new_not_exist)?
        } else {
            return Err(Errno::new_is_not_dir());
        };

        let mut parent_entry = &mut inner_fs.root;

        for path in parent_paths {
            if let Entry::Dir(dir) = parent_entry {
                parent_entry = dir.children.get_mut(OsStr::new(path)).unwrap();
            } else {
                unreachable!()
            }
        }

        if let Entry::Dir(dir) = parent_entry {
            dir.children.insert(name.to_owned(), entry);
        } else {
            unreachable!()
        }

        Ok(())
    }

    async fn open(&self, _req: Request, path: &OsStr, flags: u32) -> Result<ReplyOpen> {
        let path = path.to_string_lossy();
        let paths = split_path(&path);

        debug!("open path {}", path);

        let mut entry = &self.0.read().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        if entry.is_dir() {
            Err(Errno::new_is_dir())
        } else {
            Ok(ReplyOpen { fh: 0, flags })
        }
    }

    async fn read(
        &self,
        _req: Request,
        path: Option<&OsStr>,
        _fh: u64,
        offset: u64,
        size: u32,
    ) -> Result<ReplyData> {
        let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
        let paths = split_path(&path);

        debug!("read path {}", path);

        let mut entry = &self.0.read().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        let file = if let Entry::File(file) = entry {
            file
        } else {
            return Err(Errno::new_is_dir());
        };

        let mut cursor = Cursor::new(&file.content);
        cursor.set_position(offset);

        let size = cursor.remaining().min(size as _);

        let mut data = BytesMut::with_capacity(size);
        // safety
        unsafe {
            data.set_len(size);
        }

        cursor.read_exact(&mut data).unwrap();

        Ok(ReplyData { data: data.into() })
    }

    async fn write(
        &self,
        _req: Request,
        path: Option<&OsStr>,
        _fh: u64,
        offset: u64,
        data: &[u8],
        _flags: u32,
    ) -> Result<ReplyWrite> {
        let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
        let paths = split_path(&path);

        debug!("write path {}, paths {:?}", path, paths);

        let mut entry = &mut self.0.write().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get_mut(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        let file = if let Entry::File(file) = entry {
            file
        } else {
            return Err(Errno::new_is_dir());
        };

        let offset = offset as usize;

        if offset < file.content.len() {
            let mut content = &mut file.content.as_mut()[offset..];

            if content.len() >= data.len() {
                content.write_all(data).unwrap();
            } else {
                content.write_all(&data[..content.len()]).unwrap();
                let written = content.len();

                file.content.put(&data[written..]);
            }
        } else {
            file.content.resize(offset, 0);
            file.content.put(data);
        }

        Ok(ReplyWrite {
            written: data.len() as _,
        })
    }

    async fn release(
        &self,
        _req: Request,
        _path: Option<&OsStr>,
        _fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> Result<()> {
        Ok(())
    }

    async fn fsync(
        &self,
        _req: Request,
        _path: Option<&OsStr>,
        _fh: u64,
        _datasync: bool,
    ) -> Result<()> {
        Ok(())
    }

    async fn flush(
        &self,
        _req: Request,
        _path: Option<&OsStr>,
        _fh: u64,
        _lock_owner: u64,
    ) -> Result<()> {
        Ok(())
    }

    async fn access(&self, _req: Request, _path: &OsStr, _mask: u32) -> Result<()> {
        Ok(())
    }

    async fn create(
        &self,
        _req: Request,
        parent: &OsStr,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> Result<ReplyCreated> {
        let path = parent.to_string_lossy();
        let paths = split_path(&path);

        debug!("create parent path {}, name {:?}", path, name);

        let mut entry = &mut self.0.write().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get_mut(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        if let Entry::Dir(dir) = entry {
            if dir.children.contains_key(name) {
                return Err(Errno::new_exist());
            }

            let entry = Entry::File(File {
                name: name.to_owned(),
                content: Default::default(),
                mode: mode as mode_t,
            });
            let attr = entry.attr();

            dir.children.insert(name.to_owned(), entry);

            Ok(ReplyCreated {
                ttl: TTL,
                attr,
                generation: 0,
                fh: 0,
                flags,
            })
        } else {
            Err(Errno::new_is_not_dir())
        }
    }

    async fn batch_forget(&self, _req: Request, _paths: &[&OsStr]) {}

    // Not supported by fusefs(5) as of FreeBSD 13.0
    #[cfg(target_os = "linux")]
    async fn fallocate(
        &self,
        _req: Request,
        path: Option<&OsStr>,
        _fh: u64,
        offset: u64,
        length: u64,
        mode: u32,
    ) -> Result<()> {
        use std::os::raw::c_int;

        let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
        let paths = split_path(&path);

        let mut entry = &mut self.0.write().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get_mut(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        let file = if let Entry::File(file) = entry {
            file
        } else {
            return Err(Errno::new_is_dir());
        };

        let offset = offset as usize;
        let length = length as usize;

        match mode as c_int {
            0 => {
                if offset + length > file.content.len() {
                    file.content.resize(offset + length, 0);
                }

                Ok(())
            }

            libc::FALLOC_FL_KEEP_SIZE => {
                if offset + length > file.content.len() {
                    file.content.reserve(offset + length - file.content.len());
                }

                Ok(())
            }

            _ => Err(Errno::from(libc::EOPNOTSUPP)),
        }
    }

    async fn readdirplus(
        &self,
        _req: Request,
        parent: &OsStr,
        _fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus<Self::DirEntryPlusStream>> {
        let path = parent.to_string_lossy();
        let paths = split_path(&path);

        let mut entry = &self.0.read().await.root;
        let mut parent = entry;

        for path in paths {
            parent = entry;

            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        if let Entry::Dir(dir) = entry {
            let pre_children = vec![
                (FileType::Directory, OsString::from("."), entry.attr(), 1),
                (FileType::Directory, OsString::from(".."), parent.attr(), 2),
            ];

            let pre_children = stream::iter(pre_children);

            let children =
                pre_children
                    .chain(stream::iter(dir.children.iter()).enumerate().map(
                        |(i, (name, entry))| {
                            let kind = entry.kind();
                            let name = name.to_owned();
                            let attr = entry.attr();

                            (kind, name, attr, i as i64 + 3)
                        },
                    ))
                    .map(|(kind, name, attr, offset)| DirectoryEntryPlus {
                        kind,
                        name,
                        offset,
                        attr,
                        entry_ttl: TTL,
                        attr_ttl: TTL,
                    })
                    .skip(offset as _)
                    .map(Ok)
                    .collect::<Vec<_>>()
                    .await;

            Ok(ReplyDirectoryPlus {
                entries: stream::iter(children),
            })
        } else {
            Err(Errno::new_is_not_dir())
        }
    }

    async fn rename2(
        &self,
        req: Request,
        origin_parent: &OsStr,
        origin_name: &OsStr,
        parent: &OsStr,
        name: &OsStr,
        _flags: u32,
    ) -> Result<()> {
        self.rename(req, origin_parent, origin_name, parent, name)
            .await
    }

    async fn lseek(
        &self,
        _req: Request,
        path: Option<&OsStr>,
        _fh: u64,
        offset: u64,
        whence: u32,
    ) -> Result<ReplyLSeek> {
        let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
        let paths = split_path(&path);

        let mut entry = &self.0.read().await.root;

        for path in paths {
            if let Entry::Dir(dir) = entry {
                entry = dir
                    .children
                    .get(OsStr::new(path))
                    .ok_or_else(Errno::new_not_exist)?;
            } else {
                return Err(Errno::new_is_not_dir());
            }
        }

        let file = if let Entry::File(file) = entry {
            file
        } else {
            return Err(Errno::new_is_dir());
        };

        let whence = whence as i32;

        let offset = if whence == libc::SEEK_CUR || whence == libc::SEEK_SET {
            offset
        } else if whence == libc::SEEK_END {
            let size = file.content.len();

            if size >= offset as _ {
                size as u64 - offset
            } else {
                0
            }
        } else {
            return Err(libc::EINVAL.into());
        };

        Ok(ReplyLSeek { offset })
    }

    async fn copy_file_range(
        &self,
        req: Request,
        from_path: Option<&OsStr>,
        fh_in: u64,
        offset_in: u64,
        to_path: Option<&OsStr>,
        fh_out: u64,
        offset_out: u64,
        length: u64,
        flags: u64,
    ) -> Result<ReplyCopyFileRange> {
        let data = self
            .read(req, from_path, fh_in, offset_in, length as _)
            .await?;

        let ReplyWrite { written } = self
            .write(req, to_path, fh_out, offset_out, &data.data, flags as _)
            .await?;

        Ok(ReplyCopyFileRange {
            copied: u64::from(written),
        })
    }
}

fn split_path(path: &str) -> Vec<&str> {
    if path == "/" {
        vec![]
    } else {
        path.split(SEPARATOR).skip(1).collect()
    }
}

fn log_init() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    log_init();

    let args = env::args_os().skip(1).take(1).collect::<Vec<_>>();

    let mount_path = args.first();

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mut mount_options = MountOptions::default();
    // .allow_other(true)
    mount_options.force_readdir_plus(true).uid(uid).gid(gid);

    let mount_path = mount_path.expect("no mount point specified");
    Session::new(mount_options)
        .mount_with_unprivileged(Fs::default(), mount_path)
        .await
        .unwrap()
        .await
        .unwrap();
}
