use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt::{self, Debug, Formatter};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::lock::Mutex;
use futures_util::stream;
use futures_util::StreamExt;
use slab::Slab;

use crate::notify::Notify;
use crate::raw::reply::*;
use crate::raw::request::Request;
use crate::raw::Filesystem;
use crate::{Errno, SetAttr};
use crate::{Inode, Result};

use super::absolute_path::AbsolutePath;
use super::path_filesystem::PathFilesystem;

#[derive(Debug, Default)]
struct InodePathMap {
    inode_paths: BTreeMap<Inode, Vec<AbsolutePath>>,
    path_inode: BTreeMap<AbsolutePath, Inode>,
    slab: Slab<()>,
}

impl InodePathMap {
    fn remove_path(&mut self, path: &AbsolutePath) -> Option<Inode> {
        if let Some(inode) = self.path_inode.remove(path) {
            let paths = self
                .inode_paths
                .get_mut(&inode)
                .expect("inode_path is incorrect, paths should exist");
            let index = paths
                .iter()
                .enumerate()
                .find_map(|(index, exist_path)| {
                    if exist_path == path {
                        Some(index)
                    } else {
                        None
                    }
                })
                .expect("inode_path is incorrect, path should exist");

            paths.remove(index);

            if paths.is_empty() {
                self.inode_paths.remove(&inode);

                self.slab.remove(inode as _);
            }

            Some(inode)
        } else {
            None
        }
    }

    fn remove_inode(&mut self, inode: Inode) -> Option<Vec<AbsolutePath>> {
        if let Some(paths) = self.inode_paths.remove(&inode) {
            paths.iter().for_each(|path| {
                self.path_inode.remove(path);
            });

            self.slab.remove(inode as _);

            Some(paths)
        } else {
            None
        }
    }

    fn insert_path(&mut self, path: AbsolutePath) -> Inode {
        match self.path_inode.get(&path) {
            Some(inode) => *inode,
            None => {
                let inode = self.slab.insert(()) as Inode;
                self.inode_paths.insert(inode, vec![path.clone()]);
                self.path_inode.insert(path, inode);

                inode
            }
        }
    }
}

pub struct InodePathBridge<FS> {
    path_filesystem: FS,
    inode_path_map: Mutex<InodePathMap>,
}

impl<FS> InodePathBridge<FS> {
    pub fn new(path_filesystem: FS) -> Self {
        let mut slab = Slab::new();
        // drop 0 key
        slab.insert(());

        let mut inode_path_map = InodePathMap {
            inode_paths: Default::default(),
            path_inode: Default::default(),
            slab,
        };

        inode_path_map.insert_path(AbsolutePath::root());

        Self {
            path_filesystem,
            inode_path_map: Mutex::new(inode_path_map),
        }
    }
}

impl<FS> Debug for InodePathBridge<FS> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("InodePathBridge").finish()
    }
}

#[async_trait]
impl<FS> Filesystem for InodePathBridge<FS>
where
    FS: PathFilesystem + Send + Sync,
{
    async fn init(&self, req: Request) -> Result<()> {
        self.path_filesystem.init(req).await
    }

    async fn destroy(&self, req: Request) {
        self.path_filesystem.destroy(req).await
    }

    async fn lookup(&self, req: Request, parent: u64, name: &OsStr) -> Result<ReplyEntry> {
        let mut inode_path_map = self.inode_path_map.lock().await;

        let parent_path = match inode_path_map.inode_paths.get(&parent) {
            None => return Err(Errno::new_not_exist()),
            Some(parent) => &parent[0],
        };

        match self
            .path_filesystem
            .lookup(req, parent_path.absolute_path_buf().as_os_str(), name)
            .await
        {
            Err(err) if err.is_not_exist() => {
                let path = AbsolutePath::new(parent_path, name);

                inode_path_map.remove_path(&path);

                return Err(err);
            }

            Err(err) => Err(err),

            Ok(entry) => {
                let path = AbsolutePath::new(parent_path, name);

                let inode = inode_path_map.insert_path(path);

                Ok(ReplyEntry {
                    ttl: entry.ttl,
                    attr: (inode, entry.attr).into(),
                    generation: 0,
                })
            }
        }
    }

    async fn forget(&self, req: Request, inode: u64, nlookup: u64) {
        let inode_path_map = self.inode_path_map.lock().await;
        if let Some(paths) = &inode_path_map.inode_paths.get(&inode) {
            let path = &paths[0];

            self.path_filesystem
                .forget(req, path.absolute_path_buf().as_ref(), nlookup)
                .await
        }
    }

    async fn getattr(
        &self,
        req: Request,
        inode: u64,
        fh: Option<u64>,
        flags: u32,
    ) -> Result<ReplyAttr> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = inode_path_map
            .inode_paths
            .get(&inode)
            .map(|path| path[0].absolute_path_buf().into_os_string());
        let path = path.as_deref();

        match self.path_filesystem.getattr(req, path, fh, flags).await {
            Err(err) if err.is_not_exist() => {
                inode_path_map.remove_inode(inode);
                Err(err)
            }
            Err(err) => Err(err),
            Ok(attr) => Ok(ReplyAttr {
                ttl: attr.ttl,
                attr: (inode, attr.attr).into(),
            }),
        }
    }

    async fn setattr(
        &self,
        req: Request,
        inode: u64,
        fh: Option<u64>,
        set_attr: SetAttr,
    ) -> Result<ReplyAttr> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = inode_path_map
            .inode_paths
            .get(&inode)
            .map(|path| path[0].absolute_path_buf().into_os_string());
        let path = path.as_deref();

        match self.path_filesystem.setattr(req, path, fh, set_attr).await {
            Err(err) if err.is_not_exist() => {
                inode_path_map.remove_inode(inode);

                Err(err)
            }
            Err(err) => Err(err),
            Ok(attr) => Ok(ReplyAttr {
                ttl: attr.ttl,
                attr: (inode, attr.attr).into(),
            }),
        }
    }

    async fn readlink(&self, req: Request, inode: u64) -> Result<ReplyData> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = match inode_path_map.inode_paths.get(&inode) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        match self
            .path_filesystem
            .readlink(req, path.absolute_path_buf().as_os_str())
            .await
        {
            Err(err) if err.is_not_exist() => {
                inode_path_map.remove_inode(inode);

                Err(err)
            }
            Err(err) => Err(err),
            Ok(data) => Ok(data),
        }
    }

    async fn symlink(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        link: &OsStr,
    ) -> Result<ReplyEntry> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let parent_path = match inode_path_map.inode_paths.get(&parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        match self
            .path_filesystem
            .symlink(req, parent_path.absolute_path_buf().as_os_str(), name, link)
            .await
        {
            Err(err) => {
                if err.is_exist() {
                    let path = AbsolutePath::new(parent_path, name);
                    inode_path_map.insert_path(path);
                }

                Err(err)
            }

            Ok(entry) => {
                let path = AbsolutePath::new(parent_path, name);
                let inode = inode_path_map.insert_path(path);

                Ok(ReplyEntry {
                    ttl: entry.ttl,
                    attr: (inode, entry.attr).into(),
                    generation: 0,
                })
            }
        }
    }

    async fn mknod(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        rdev: u32,
    ) -> Result<ReplyEntry> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let parent_path = match inode_path_map.inode_paths.get(&parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        match self
            .path_filesystem
            .mknod(
                req,
                parent_path.absolute_path_buf().as_os_str(),
                name,
                mode,
                rdev,
            )
            .await
        {
            Err(err) => {
                if err.is_exist() {
                    let path = AbsolutePath::new(parent_path, name);
                    inode_path_map.insert_path(path);
                }

                Err(err)
            }

            Ok(entry) => {
                let path = AbsolutePath::new(parent_path, name);
                let inode = inode_path_map.insert_path(path);

                Ok(ReplyEntry {
                    ttl: entry.ttl,
                    attr: (inode, entry.attr).into(),
                    generation: 0,
                })
            }
        }
    }

    async fn mkdir(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        umask: u32,
    ) -> Result<ReplyEntry> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let parent_path = match inode_path_map.inode_paths.get(&parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        match self
            .path_filesystem
            .mkdir(
                req,
                parent_path.absolute_path_buf().as_os_str(),
                name,
                mode,
                umask,
            )
            .await
        {
            Err(err) => {
                if err.is_exist() {
                    let path = AbsolutePath::new(parent_path, name);
                    inode_path_map.insert_path(path);
                }

                Err(err)
            }

            Ok(entry) => {
                let path = AbsolutePath::new(parent_path, name);
                let inode = inode_path_map.insert_path(path);

                Ok(ReplyEntry {
                    ttl: entry.ttl,
                    attr: (inode, entry.attr).into(),
                    generation: 0,
                })
            }
        }
    }

    async fn unlink(&self, req: Request, parent: u64, name: &OsStr) -> Result<()> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let parent_path = match inode_path_map.inode_paths.get(&parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        if let Err(err) = self
            .path_filesystem
            .unlink(req, parent_path.absolute_path_buf().as_os_str(), name)
            .await
        {
            if err.is_not_exist() {
                let path = AbsolutePath::new(parent_path, name);
                inode_path_map.remove_path(&path);
            } else if err.is_dir() {
                let path = AbsolutePath::new(parent_path, name);
                inode_path_map.insert_path(path);
            }

            Err(err)
        } else {
            let path = AbsolutePath::new(parent_path, name);
            inode_path_map.remove_path(&path);

            Ok(())
        }
    }

    async fn rmdir(&self, req: Request, parent: u64, name: &OsStr) -> Result<()> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let parent_path = match inode_path_map.inode_paths.get(&parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        if let Err(err) = self
            .path_filesystem
            .rmdir(req, parent_path.absolute_path_buf().as_os_str(), name)
            .await
        {
            if err.is_not_exist() {
                let path = AbsolutePath::new(parent_path, name);
                inode_path_map.remove_path(&path);
            } else if err.is_not_dir() {
                let path = AbsolutePath::new(parent_path, name);
                inode_path_map.insert_path(path);
            }

            Err(err)
        } else {
            let path = AbsolutePath::new(parent_path, name);
            inode_path_map.remove_path(&path);

            Ok(())
        }
    }

    async fn rename(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        new_parent: u64,
        new_name: &OsStr,
    ) -> Result<()> {
        let mut inode_path_map = self.inode_path_map.lock().await;

        let origin_parent_path = match inode_path_map.inode_paths.get(&parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        let new_parent_path = match inode_path_map.inode_paths.get(&new_parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        // here is very complex so don't modify the inode_path_map when error
        self.path_filesystem
            .rename(
                req,
                origin_parent_path.absolute_path_buf().as_os_str(),
                name,
                new_parent_path.absolute_path_buf().as_os_str(),
                new_name,
            )
            .await?;

        let origin_path = AbsolutePath::new(origin_parent_path, name);
        let new_path = AbsolutePath::new(new_parent_path, new_name);

        match inode_path_map.path_inode.remove(&origin_path) {
            // origin path is not insert into inode_path_map
            None => {
                inode_path_map.insert_path(new_path);
            }

            // origin path is inserted into inode_path_map
            Some(inode) => {
                inode_path_map.remove_path(&new_path);

                inode_path_map
                    .inode_paths
                    .insert(inode, vec![new_path.clone()]);
                inode_path_map.path_inode.insert(new_path, inode);
            }
        }

        Ok(())
    }

    async fn link(
        &self,
        req: Request,
        inode: u64,
        new_parent: u64,
        new_name: &OsStr,
    ) -> Result<ReplyEntry> {
        let mut inode_path_map = self.inode_path_map.lock().await;

        let path = match inode_path_map.inode_paths.get(&inode) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        let new_parent_path = match inode_path_map.inode_paths.get(&new_parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        // here is very complex so don't modify the inode_path_map when error
        let entry = self
            .path_filesystem
            .link(
                req,
                path.absolute_path_buf().as_ref(),
                new_parent_path.absolute_path_buf().as_ref(),
                new_name,
            )
            .await?;

        let new_path = AbsolutePath::new(new_parent_path, new_name);
        // Safety: checked when get path
        inode_path_map
            .inode_paths
            .get_mut(&inode)
            .unwrap()
            .push(new_path);

        Ok(ReplyEntry {
            ttl: entry.ttl,
            attr: (inode, entry.attr).into(),
            generation: 0,
        })
    }

    async fn open(&self, req: Request, inode: u64, flags: u32) -> Result<ReplyOpen> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = match inode_path_map.inode_paths.get(&inode) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        match self
            .path_filesystem
            .open(req, path.absolute_path_buf().as_ref(), flags)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    inode_path_map.remove_inode(inode);
                }

                Err(err)
            }

            Ok(opened) => Ok(opened),
        }
    }

    async fn read(
        &self,
        req: Request,
        inode: u64,
        fh: u64,
        offset: u64,
        size: u32,
    ) -> Result<ReplyData> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        match self
            .path_filesystem
            .read(req, path_str, fh, offset, size)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    if let Some(path) = path {
                        self.inode_path_map.lock().await.remove_path(&path);
                    }
                }

                Err(err)
            }

            Ok(data) => Ok(data),
        }
    }

    async fn write(
        &self,
        req: Request,
        inode: u64,
        fh: u64,
        offset: u64,
        data: &[u8],
        flags: u32,
    ) -> Result<ReplyWrite> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        match self
            .path_filesystem
            .write(req, path_str, fh, offset, data, flags)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    if let Some(path) = path {
                        self.inode_path_map.lock().await.remove_path(&path);
                    }
                }

                Err(err)
            }

            Ok(written) => Ok(written),
        }
    }

    async fn statsfs(&self, req: Request, inode: u64) -> Result<ReplyStatFs> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        match self
            .path_filesystem
            .statsfs(req, path.absolute_path_buf().as_ref())
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    let path = path.clone();
                    inode_path_map.remove_path(&path);
                }

                Err(err)
            }

            Ok(stat_fs) => Ok(stat_fs),
        }
    }

    async fn release(
        &self,
        req: Request,
        inode: u64,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
    ) -> Result<()> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        if let Err(err) = self
            .path_filesystem
            .release(req, path_str, fh, flags, lock_owner, flush)
            .await
        {
            if err.is_not_exist() {
                if let Some(path) = path {
                    self.inode_path_map.lock().await.remove_path(&path);
                }
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    async fn fsync(&self, req: Request, inode: u64, fh: u64, datasync: bool) -> Result<()> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        if let Err(err) = self
            .path_filesystem
            .fsync(req, path_str, fh, datasync)
            .await
        {
            if err.is_not_exist() {
                if let Some(path) = path {
                    self.inode_path_map.lock().await.remove_path(&path);
                }
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    async fn setxattr(
        &self,
        req: Request,
        inode: u64,
        name: &OsStr,
        value: &OsStr,
        flags: u32,
        position: u32,
    ) -> Result<()> {
        let inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        // here don't remove path when error is not exist because it may be the xattr not exist
        self.path_filesystem
            .setxattr(
                req,
                path.absolute_path_buf().as_ref(),
                name,
                value,
                flags,
                position,
            )
            .await
    }

    async fn getxattr(
        &self,
        req: Request,
        inode: u64,
        name: &OsStr,
        size: u32,
    ) -> Result<ReplyXAttr> {
        let inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        // here don't remove path when error is not exist because it may be the xattr not exist
        self.path_filesystem
            .getxattr(req, path.absolute_path_buf().as_ref(), name, size)
            .await
    }

    async fn listxattr(&self, req: Request, inode: u64, size: u32) -> Result<ReplyXAttr> {
        let inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        // here don't remove path when error is not exist because it may be the xattr not exist
        self.path_filesystem
            .listxattr(req, path.absolute_path_buf().as_ref(), size)
            .await
    }

    async fn removexattr(&self, req: Request, inode: u64, name: &OsStr) -> Result<()> {
        let inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        // here don't remove path when error is not exist because it may be the xattr not exist
        self.path_filesystem
            .removexattr(req, path.absolute_path_buf().as_ref(), name)
            .await
    }

    async fn flush(&self, req: Request, inode: u64, fh: u64, lock_owner: u64) -> Result<()> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        if let Err(err) = self
            .path_filesystem
            .flush(req, path_str, fh, lock_owner)
            .await
        {
            if err.is_not_exist() {
                if let Some(path) = path {
                    self.inode_path_map.lock().await.remove_path(&path);
                }
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    async fn opendir(&self, req: Request, inode: u64, flags: u32) -> Result<ReplyOpen> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        match self
            .path_filesystem
            .opendir(req, path.absolute_path_buf().as_ref(), flags)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    let path = path.clone();
                    inode_path_map.remove_path(&path);
                }

                Err(err)
            }

            Ok(opened) => Ok(opened),
        }
    }

    async fn readdir(
        &self,
        req: Request,
        parent: u64,
        fh: u64,
        offset: i64,
    ) -> Result<ReplyDirectory> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let parent = &inode_path_map
            .inode_paths
            .get(&parent)
            .ok_or_else(Errno::new_not_exist)?[0]
            .clone();

        match self
            .path_filesystem
            .readdir(req, parent.absolute_path_buf().as_ref(), fh, offset)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    let parent = parent.clone();
                    inode_path_map.remove_path(&parent);
                }

                Err(err)
            }

            Ok(mut dirs) => {
                let dirs_size = dirs.entries.size_hint().1.unwrap_or(0);
                let mut dir_list = Vec::with_capacity(dirs_size);

                while let Some(result) = dirs.entries.next().await {
                    let entry = result?;

                    let path = AbsolutePath::new(&parent, &entry.name);
                    let inode = inode_path_map.insert_path(path);

                    dir_list.push(DirectoryEntry {
                        inode,
                        index: entry.index,
                        kind: entry.kind,
                        name: entry.name,
                    });
                }

                Ok(ReplyDirectory {
                    entries: Box::pin(stream::iter(dir_list)),
                })
            }
        }
    }

    async fn releasedir(&self, req: Request, inode: u64, fh: u64, flags: u32) -> Result<()> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        if let Err(err) = self
            .path_filesystem
            .releasedir(req, path.absolute_path_buf().as_ref(), fh, flags)
            .await
        {
            if err.is_not_exist() {
                let path = path.clone();
                inode_path_map.remove_path(&path);
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    async fn fsyncdir(&self, req: Request, inode: u64, fh: u64, datasync: bool) -> Result<()> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        if let Err(err) = self
            .path_filesystem
            .fsyncdir(req, path.absolute_path_buf().as_ref(), fh, datasync)
            .await
        {
            if err.is_not_exist() {
                let path = path.clone();
                inode_path_map.remove_path(&path);
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(feature = "file-lock")]
    async fn getlk(
        &self,
        req: Request,
        inode: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        r#type: u32,
        pid: u32,
    ) -> Result<ReplyLock> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        match self
            .path_filesystem
            .getlk(req, path_str, fh, lock_owner, start, end, r#type, pid)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    if let Some(path) = path {
                        self.inode_path_map.lock().await.remove_path(&path);
                    }
                }

                Err(err)
            }

            Ok(locked) => Ok(locked),
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(feature = "file-lock")]
    async fn setlk(
        &self,
        req: Request,
        inode: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        r#type: u32,
        pid: u32,
        block: bool,
    ) -> Result<()> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        if let Err(err) = self
            .path_filesystem
            .setlk(
                req, path_str, fh, lock_owner, start, end, r#type, pid, block,
            )
            .await
        {
            if err.is_not_exist() {
                if let Some(path) = path {
                    self.inode_path_map.lock().await.remove_path(&path);
                }
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    async fn access(&self, req: Request, inode: u64, mask: u32) -> Result<()> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        if let Err(err) = self
            .path_filesystem
            .access(req, path.absolute_path_buf().as_ref(), mask)
            .await
        {
            if err.is_not_exist() {
                let path = path.clone();
                inode_path_map.remove_path(&path);
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    async fn create(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> Result<ReplyCreated> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let parent = &inode_path_map
            .inode_paths
            .get(&parent)
            .ok_or_else(Errno::new_not_exist)?[0];

        match self
            .path_filesystem
            .create(req, parent.absolute_path_buf().as_ref(), name, mode, flags)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    let parent = parent.clone();
                    inode_path_map.remove_path(&parent);
                }

                Err(err)
            }

            Ok(created) => Ok(created),
        }
    }

    #[inline]
    async fn interrupt(&self, req: Request, unique: u64) -> Result<()> {
        self.path_filesystem.interrupt(req, unique).await
    }

    async fn bmap(&self, req: Request, inode: u64, block_size: u32, idx: u64) -> Result<ReplyBmap> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        match self
            .path_filesystem
            .bmap(req, path.absolute_path_buf().as_ref(), block_size, idx)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    let path = path.clone();
                    inode_path_map.remove_path(&path);
                }

                Err(err)
            }

            Ok(bmap) => Ok(bmap),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn poll(
        &self,
        req: Request,
        inode: u64,
        fh: u64,
        kh: Option<u64>,
        flags: u32,
        events: u32,
        notify: &Notify,
    ) -> Result<ReplyPoll> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        match self
            .path_filesystem
            .poll(req, path_str, fh, kh, flags, events, notify)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    if let Some(path) = path {
                        self.inode_path_map.lock().await.remove_path(&path);
                    }
                }

                Err(err)
            }

            Ok(poll) => Ok(poll),
        }
    }

    async fn notify_reply(&self, req: Request, inode: u64, offset: u64, data: Bytes) -> Result<()> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let path = &inode_path_map
            .inode_paths
            .get(&inode)
            .ok_or_else(Errno::new_not_exist)?[0];

        if let Err(err) = self
            .path_filesystem
            .notify_reply(req, path.absolute_path_buf().as_ref(), offset, data)
            .await
        {
            if err.is_not_exist() {
                let path = path.clone();
                inode_path_map.remove_path(&path);
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    async fn batch_forget(&self, req: Request, inodes: &[u64]) {
        let inode_path_map = self.inode_path_map.lock().await;
        let paths = inodes
            .iter()
            .filter_map(|inode| {
                inode_path_map
                    .inode_paths
                    .get(inode)
                    .map(|paths| paths[0].absolute_path_buf())
            })
            .collect::<Vec<_>>();
        let paths = paths.iter().map(|path| path.as_ref()).collect::<Vec<_>>();

        self.path_filesystem.batch_forget(req, &paths).await
    }

    async fn fallocate(
        &self,
        req: Request,
        inode: u64,
        fh: u64,
        offset: u64,
        length: u64,
        mode: u32,
    ) -> Result<()> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        if let Err(err) = self
            .path_filesystem
            .fallocate(req, path_str, fh, offset, length, mode)
            .await
        {
            if err.is_not_exist() {
                if let Some(path) = path {
                    self.inode_path_map.lock().await.remove_path(&path);
                }
            }

            Err(err)
        } else {
            Ok(())
        }
    }

    async fn readdirplus(
        &self,
        req: Request,
        parent: u64,
        fh: u64,
        offset: u64,
        lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus> {
        let mut inode_path_map = self.inode_path_map.lock().await;
        let parent = &inode_path_map
            .inode_paths
            .get(&parent)
            .ok_or_else(Errno::new_not_exist)?[0]
            .clone();

        let mut dirs = match self
            .path_filesystem
            .readdirplus(
                req,
                parent.absolute_path_buf().as_ref(),
                fh,
                offset,
                lock_owner,
            )
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    let parent = parent.clone();
                    inode_path_map.remove_path(&parent);
                }

                return Err(err);
            }

            Ok(dirs) => dirs,
        };

        let dirs_size = dirs.entries.size_hint().1.unwrap_or(0);
        let mut dir_list = Vec::with_capacity(dirs_size);

        while let Some(result) = dirs.entries.next().await {
            let entry = result?;
            let path = AbsolutePath::new(&parent, &entry.name);

            let inode = inode_path_map.insert_path(path);

            dir_list.push(DirectoryEntryPlus {
                inode,
                generation: 0,
                index: entry.index,
                kind: entry.kind,
                name: entry.name,
                attr: (inode, entry.attr).into(),
                entry_ttl: entry.entry_ttl,
                attr_ttl: entry.attr_ttl,
            });
        }

        Ok(ReplyDirectoryPlus {
            entries: Box::pin(stream::iter(dir_list)),
        })
    }

    async fn rename2(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        new_parent: u64,
        new_name: &OsStr,
        flags: u32,
    ) -> Result<()> {
        let mut inode_path_map = self.inode_path_map.lock().await;

        let origin_parent_path = match inode_path_map.inode_paths.get(&parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        let new_parent_path = match inode_path_map.inode_paths.get(&new_parent) {
            None => return Err(Errno::new_not_exist()),
            Some(path) => &path[0],
        };

        // here is very complex so don't modify the inode_path_map when error
        self.path_filesystem
            .rename2(
                req,
                origin_parent_path.absolute_path_buf().as_os_str(),
                name,
                new_parent_path.absolute_path_buf().as_os_str(),
                new_name,
                flags,
            )
            .await?;

        let origin_path = AbsolutePath::new(origin_parent_path, name);
        let new_path = AbsolutePath::new(new_parent_path, new_name);

        match inode_path_map.path_inode.remove(&origin_path) {
            // origin path is not insert into inode_path_map
            None => {
                inode_path_map.insert_path(new_path);
            }

            // origin path is inserted into inode_path_map
            Some(inode) => {
                inode_path_map.remove_path(&new_path);

                inode_path_map
                    .inode_paths
                    .insert(inode, vec![new_path.clone()]);
                inode_path_map.path_inode.insert(new_path, inode);
            }
        }

        Ok(())
    }

    async fn lseek(
        &self,
        req: Request,
        inode: u64,
        fh: u64,
        offset: u64,
        whence: u32,
    ) -> Result<ReplyLSeek> {
        let path = self
            .inode_path_map
            .lock()
            .await
            .inode_paths
            .get(&inode)
            .map(|path| path[0].clone());
        let path_str = path.as_ref().map(|path| path.absolute_path_buf());
        let path_str = path_str.as_ref().map(|path| path.as_ref());

        match self
            .path_filesystem
            .lseek(req, path_str, fh, offset, whence)
            .await
        {
            Err(err) => {
                if err.is_not_exist() {
                    if let Some(path) = path {
                        self.inode_path_map.lock().await.remove_path(&path);
                    }
                }

                Err(err)
            }

            Ok(seek) => Ok(seek),
        }
    }

    #[allow(clippy::too_many_arguments)]
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
        let inode_path_map = self.inode_path_map.lock().await;

        let path = inode_path_map
            .inode_paths
            .get(&inode)
            .map(|path| path[0].absolute_path_buf());
        let path_str = path.as_ref().map(|path| path.as_ref());

        let path_out = inode_path_map
            .inode_paths
            .get(&inode_out)
            .map(|path| path[0].absolute_path_buf());
        let path_out_str = path_out.as_ref().map(|path| path.as_ref());

        drop(inode_path_map);

        self.path_filesystem
            .copy_file_range(
                req,
                path_str,
                fh_in,
                off_in,
                path_out_str,
                fh_out,
                off_out,
                length,
                flags,
            )
            .await
    }
}
