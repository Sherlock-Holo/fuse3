use std::ffi::OsStr;

use async_trait::async_trait;
use bytes::Bytes;

use crate::{Request, SetAttr};
use crate::notify::Notify;
use crate::Result;

use super::reply::{
    ReplyAttr, ReplyBmap, ReplyCopyFileRange, ReplyCreated, ReplyData, ReplyDirectory,
    ReplyDirectoryPlus, ReplyEntry, ReplyLSeek, ReplyOpen, ReplyPoll, ReplyStatFs, ReplyWrite,
    ReplyXAttr,
};
#[cfg(feature = "file-lock")]
use super::reply::ReplyLock;

#[async_trait]
pub trait PathFilesystem {
    async fn init(&self, req: Request) -> Result<()>;

    async fn destroy(&self, req: Request);

    async fn lookup(&self, req: Request, parent: &OsStr, name: &OsStr) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn forget(&self, req: Request, parent: &OsStr, nlookup: u64) {}

    async fn getattr(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: Option<u64>,
        flags: u32,
    ) -> Result<ReplyAttr> {
        Err(libc::ENOSYS.into())
    }

    async fn setattr(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: Option<u64>,
        set_attr: SetAttr,
    ) -> Result<ReplyAttr> {
        Err(libc::ENOSYS.into())
    }

    async fn readlink(&self, req: Request, path: &OsStr) -> Result<ReplyData> {
        Err(libc::ENOSYS.into())
    }

    async fn symlink(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        link_path: &OsStr,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn mknod(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        mode: u32,
        rdev: u32,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn mkdir(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        mode: u32,
        umask: u32,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn unlink(&self, req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn rmdir(&self, req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn rename(
        &self,
        req: Request,
        origin_parent: &OsStr,
        origin_name: &OsStr,
        parent: &OsStr,
        name: &OsStr,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn link(
        &self,
        req: Request,
        path: &OsStr,
        new_parent: &OsStr,
        new_name: &OsStr,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn open(&self, req: Request, path: &OsStr, flags: u32) -> Result<ReplyOpen> {
        Err(libc::ENOSYS.into())
    }

    async fn read(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        size: u32,
    ) -> Result<ReplyData> {
        Err(libc::ENOSYS.into())
    }

    async fn write(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        data: &[u8],
        flags: u32,
    ) -> Result<ReplyWrite> {
        Err(libc::ENOSYS.into())
    }

    async fn statsfs(&self, req: Request, path: &OsStr) -> Result<ReplyStatFs> {
        Err(libc::ENOSYS.into())
    }

    async fn release(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn fsync(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        datasync: bool,
    ) -> Result<()> {
        Ok(())
    }

    async fn setxattr(
        &self,
        req: Request,
        path: &OsStr,
        name: &OsStr,
        value: &OsStr,
        flags: u32,
        position: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn getxattr(
        &self,
        req: Request,
        path: &OsStr,
        name: &OsStr,
        size: u32,
    ) -> Result<ReplyXAttr> {
        Err(libc::ENOSYS.into())
    }

    async fn listxattr(&self, req: Request, path: &OsStr, size: u32) -> Result<ReplyXAttr> {
        Err(libc::ENOSYS.into())
    }

    async fn removexattr(&self, req: Request, path: &OsStr, name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn flush(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        lock_owner: u64,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn opendir(&self, req: Request, path: &OsStr, flags: u32) -> Result<ReplyOpen> {
        Ok(ReplyOpen { fh: 0, flags: 0 })
    }

    async fn readdir(
        &self,
        req: Request,
        path: &OsStr,
        fh: u64,
        offset: i64,
    ) -> Result<ReplyDirectory> {
        Err(libc::ENOSYS.into())
    }

    async fn releasedir(&self, req: Request, path: &OsStr, fh: u64, flags: u32) -> Result<()> {
        Ok(())
    }

    async fn fsyncdir(&self, req: Request, path: &OsStr, fh: u64, datasync: bool) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    #[cfg(feature = "file-lock")]
    async fn getlk(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        r#type: u32,
        pid: u32,
    ) -> Result<ReplyLock>;

    #[cfg(feature = "file-lock")]
    async fn setlk(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        r#type: u32,
        pid: u32,
        block: bool,
    ) -> Result<()>;

    async fn access(&self, req: Request, path: &OsStr, mask: u32) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn create(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> Result<ReplyCreated> {
        Err(libc::ENOSYS.into())
    }

    /// handle interrupt. When a operation is interrupted, an interrupt request will send to fuse
    /// server with the unique id of the operation.
    async fn interrupt(&self, req: Request, unique: u64) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn bmap(
        &self,
        req: Request,
        path: &OsStr,
        block_size: u32,
        idx: u64,
    ) -> Result<ReplyBmap> {
        Err(libc::ENOSYS.into())
    }

    async fn poll(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        kn: Option<u64>,
        flags: u32,
        envents: u32,
        notify: &Notify,
    ) -> Result<ReplyPoll> {
        Err(libc::ENOSYS.into())
    }

    async fn notify_reply(
        &self,
        req: Request,
        path: &OsStr,
        offset: u64,
        data: Bytes,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn batch_forget(&self, req: Request, paths: &[&OsStr]) {}

    async fn fallocate(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        length: u64,
        mode: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn readdirplus(
        &self,
        req: Request,
        parent: &OsStr,
        fh: u64,
        offset: u64,
        lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus> {
        Err(libc::ENOSYS.into())
    }

    async fn rename2(
        &self,
        req: Request,
        origin_parent: &OsStr,
        origin_name: &OsStr,
        parent: &OsStr,
        name: &OsStr,
        flags: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn lseek(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        whence: u32,
    ) -> Result<ReplyLSeek> {
        Err(libc::ENOSYS.into())
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
        Err(libc::ENOSYS.into())
    }
}
