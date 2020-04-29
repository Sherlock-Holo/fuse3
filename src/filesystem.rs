use std::ffi::OsStr;

use async_trait::async_trait;

use crate::reply::*;
use crate::request::Request;
use crate::{Result, SetAttr};

#[async_trait]
pub trait Filesystem {
    async fn init(&self, req: Request) -> Result<()>;

    async fn destroy(&self, req: Request);

    async fn lookup(&self, _req: Request, _parent: u64, _name: &OsStr) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn forget(&self, _req: Request, _inode: u64, _nlookup: u64) {}

    async fn getattr(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _flags: u32,
    ) -> Result<ReplyAttr> {
        Err(libc::ENOSYS.into())
    }

    async fn setattr(&self, _req: Request, _inode: u64, _set_attr: SetAttr) -> Result<ReplyAttr> {
        Err(libc::ENOSYS.into())
    }

    async fn readlink(&self, _req: Request, _inode: u64) -> Result<ReplyData> {
        Err(libc::ENOSYS.into())
    }

    async fn symlink(
        &self,
        _req: Request,
        _parent: u64,
        _name: &OsStr,
        _link: &OsStr,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn mknod(
        &self,
        _req: Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _rdev: u32,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn mkdir(
        &self,
        _req: Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn unlink(&self, _req: Request, _parent: u64, _name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn rmdir(&self, _req: Request, _parent: u64, _name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn rename(
        &self,
        _req: Request,
        _parent: u64,
        _name: &OsStr,
        _new_parent: u64,
        _new_name: &OsStr,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn link(
        &self,
        _req: Request,
        _inode: u64,
        _new_parent: u64,
        _new_name: &OsStr,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    async fn open(&self, _req: Request, _inode: u64, _flags: u32) -> Result<ReplyOpen> {
        Err(libc::ENOSYS.into())
    }

    async fn read(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: u64,
        _size: u32,
    ) -> Result<ReplyData> {
        Err(libc::ENOSYS.into())
    }

    async fn write(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: u64,
        _data: &[u8],
        _flags: u32,
    ) -> Result<ReplyWrite> {
        Err(libc::ENOSYS.into())
    }

    async fn statsfs(&self, _req: Request, _inode: u64) -> Result<ReplyStatFs> {
        Err(libc::ENOSYS.into())
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
        Err(libc::ENOSYS.into())
    }

    async fn fsync(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn setxattr(
        &self,
        _req: Request,
        _inode: u64,
        _name: &OsStr,
        _value: &OsStr,
        _flags: u32,
        _position: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    /// Get an extended attribute. If size is too small, use [`ReplyXAttr::Size`] to return correct
    /// size. If size is enough, use [`ReplyXAttr::Data`] to send it, or return error.
    ///
    /// [`ReplyXAttr::Size`]: ReplyXAttr::Size
    /// [`ReplyXAttr::Data`]: ReplyXAttr::Data
    async fn getxattr(
        &self,
        _req: Request,
        _inode: u64,
        _name: &OsStr,
        _size: u32,
    ) -> Result<ReplyXAttr> {
        Err(libc::ENOSYS.into())
    }

    /// Get an extended attribute. If size is too small, use [`ReplyXAttr::Size`] to return correct
    /// size. If size is enough, use [`ReplyXAttr::Data`] to send it, or return error.
    ///
    /// [`ReplyXAttr::Size`]: ReplyXAttr::Size
    /// [`ReplyXAttr::Data`]: ReplyXAttr::Data
    async fn listxattr(&self, _req: Request, _inode: u64, _size: u32) -> Result<ReplyXAttr> {
        Err(libc::ENOSYS.into())
    }

    async fn removexattr(&self, _req: Request, _inode: u64, _name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn flush(&self, _req: Request, _inode: u64, _fh: u64, _lock_owner: u64) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn opendir(&self, _req: Request, _inode: u64, _flags: u32) -> Result<ReplyOpen> {
        Ok(ReplyOpen { fh: 0, flags: 0 })
    }

    async fn readdir(
        &self,
        _req: Request,
        _parent: u64,
        _fh: u64,
        _offset: i64,
    ) -> Result<ReplyDirectory> {
        Err(libc::ENOSYS.into())
    }

    async fn releasedir(&self, _req: Request, _inode: u64, _fh: u64, _flags: u32) -> Result<()> {
        Ok(())
    }

    async fn fsyncdir(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    #[cfg(feature = "file-lock")]
    async fn getlk(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _lock_owner: u64,
        _start: u64,
        _end: u64,
        _type: u32,
        _pid: u32,
    ) -> Result<ReplyLock>;

    #[cfg(feature = "file-lock")]
    async fn setlk(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _lock_owner: u64,
        _start: u64,
        _end: u64,
        _type: u32,
        _pid: u32,
        _block: bool,
    ) -> Result<ReplyLock>;

    async fn access(&self, _req: Request, _inode: u64, _mask: u32) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn create(
        &self,
        _req: Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _flags: u32,
    ) -> Result<ReplyCreated> {
        Err(libc::ENOSYS.into())
    }

    async fn interrupt(&self, _req: Request, _unique: u64) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn bmap(
        &self,
        _req: Request,
        _inode: u64,
        _blocksize: u32,
        _idx: u64,
    ) -> Result<ReplyBmap> {
        Err(libc::ENOSYS.into())
    }

    async fn ioctl(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _flags: u32,
        _cmd: u32,
        _arg: u64,
        _in_size: u32,
        _out_size: u32,
    ) -> Result<ReplyIoctl> {
        Err(libc::ENOSYS.into())
    }

    async fn poll(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _kh: u64,
        _flags: u32,
    ) -> Result<ReplyPoll> {
        Err(libc::ENOSYS.into())
    }

    // TODO handle notify
    // async fn notify_reply(&self, )

    async fn batch_forget(&self, _req: Request, _inodes: &[u64]) {}

    async fn fallocate(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: u64,
        _length: u64,
        _mode: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn readdirplus(
        &self,
        _req: Request,
        _parent: u64,
        _fh: u64,
        _offset: u64,
        _lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus> {
        Err(libc::ENOSYS.into())
    }

    async fn rename2(
        &self,
        _req: Request,
        _parent: u64,
        _name: &OsStr,
        _new_parent: u64,
        _new_name: &OsStr,
        _flags: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    async fn lseek(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: u64,
        _whence: u32,
    ) -> Result<ReplyLSeek> {
        Err(libc::ENOSYS.into())
    }

    async fn copy_file_range(
        &self,
        _req: Request,
        _inode: u64,
        _fh_in: u64,
        _off_in: u64,
        _inode_out: u64,
        _fh_out: u64,
        _off_out: u64,
        _length: u64,
        _flags: u64,
    ) -> Result<ReplyCopyFileRange> {
        Err(libc::ENOSYS.into())
    }

    // TODO setupmapping and removemapping
}
