use std::ffi::{OsStr, OsString};
use std::path::Path;

use async_trait::async_trait;

use crate::reply::*;
use crate::request::Request;
use crate::{Result, SetAttr};

#[async_trait]
pub trait Filesystem {
    async fn init(&self, req: Request) -> Result<ReplyEntry>;

    async fn destroy(&self, req: Request);

    async fn lookup(&self, _req: Request, _parent: u64, _name: OsString) -> Result<ReplyEntry> {
        Err(libc::ENOSYS)
    }

    async fn forget(&self, _req: Request, _inode: u64, _nlookup: u64) -> Result<()> {
        Ok(())
    }

    async fn getattr(&self, _req: Request, _inode: u64) -> Result<ReplyAttr> {
        Err(libc::ENOSYS)
    }

    async fn setattr(&self, _req: Request, _inode: u64, _set_attr: SetAttr) -> Result<ReplyAttr> {
        Err(libc::ENOSYS)
    }

    async fn readlink<T: AsRef<[u8]>>(&self, _req: Request, _inode: u64) -> Result<ReplyData<T>> {
        Err(libc::ENOSYS)
    }

    async fn symlink(
        &self,
        _req: Request,
        _parent: u64,
        _name: OsString,
        _link: &Path,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS)
    }

    async fn mknod(
        &self,
        _req: Request,
        _parent: u64,
        _name: OsString,
        _mode: u32,
        _rdev: u32,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS)
    }

    async fn mkdir(
        &self,
        _req: Request,
        _parent: u64,
        _name: OsString,
        _mode: u32,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS)
    }

    async fn unlink(&self, _req: Request, _parent: u64, _name: OsString) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn rmdir(&self, _req: Request, _parent: u64, _name: OsString) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn rename(
        &self,
        _req: Request,
        _parent: u64,
        _name: OsString,
        _new_parent: u64,
        _new_name: OsString,
    ) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn link(
        &self,
        _req: Request,
        _inode: u64,
        _new_parent: u64,
        _new_name: OsString,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS)
    }

    async fn open(&self, _req: Request, _inode: u64, _flags: u64) -> Result<ReplyOpen> {
        Err(libc::ENOSYS)
    }

    async fn read<T: AsRef<[u8]>>(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: i64,
        _size: u64,
    ) -> Result<ReplyData<T>> {
        Err(libc::ENOSYS)
    }

    async fn write(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: i64,
        _data: &[u8],
        _flags: u32,
    ) -> Result<ReplyWrite> {
        Err(libc::ENOSYS)
    }

    async fn statsfs(&self, _req: Request, _inode: u64) -> Result<ReplyStatFs> {
        Err(libc::ENOSYS)
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
        Err(libc::ENOSYS)
    }

    async fn fsync(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn setxattr(
        &self,
        _req: Request,
        _inode: u64,
        _name: OsString,
        _value: &[u8],
        _flags: u32,
        _position: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS)
    }

    /// Get an extended attribute. If size is 0, use [`ReplyXAttr::Size`] to send size. If size is not 0,
    /// and the value fits, use [`ReplyXAttr::Data`] to send it, or return error.
    ///
    /// [`ReplyXAttr::Size`]: ReplyXAttr::Size
    /// [`ReplyXAttr::Data`]: ReplyXAttr::Data
    async fn getxattr<T: AsRef<[u8]>>(
        &self,
        _req: Request,
        _inode: u64,
        _name: OsString,
        _size: u32,
    ) -> Result<ReplyXAttr<T>> {
        Err(libc::ENOSYS)
    }

    /// Get an extended attribute. If size is 0, use [`ReplyXAttr::Size`] to send size. If size is not 0,
    /// and the value fits, use [`ReplyXAttr::Data`] to send it, or return error.
    ///
    /// [`ReplyXAttr::Size`]: ReplyXAttr::Size
    /// [`ReplyXAttr::Data`]: ReplyXAttr::Data
    async fn listxattr<T: AsRef<[u8]>>(
        &self,
        _req: Request,
        _inode: u64,
        _size: u32,
    ) -> Result<ReplyXAttr<T>> {
        Err(libc::ENOSYS)
    }

    async fn removexattr(&self, _req: Request, _inode: u64, _name: OsString) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn flush(&self, _req: Request, _inode: u64, _fh: u64, _lock_owner: u64) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn opendir(&self, _req: Request, _inode: u64, _flags: u32) -> Result<ReplyOpen> {
        Ok(ReplyOpen { fh: 0, flags: 0 })
    }

    async fn readdir<I, S>(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: i64,
    ) -> Result<ReplyDirectory<I, S>>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = DirectoryEntry<S>>,
    {
        Err(libc::ENOSYS)
    }

    async fn releasedir(&self, _req: Request, _inode: u64, _fh: u64, _flags: u32) -> Result<()> {
        Ok(())
    }

    async fn fsyncdir(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> Result<()> {
        Err(libc::ENOSYS)
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
    ) -> Result<()>;

    async fn access(&self, _req: Request, _inode: u64, _mask: u32) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn create(
        &self,
        _req: Request,
        _parent: u64,
        _name: OsString,
        _mode: u32,
        _flags: u32,
    ) -> Result<ReplyCreated> {
        Err(libc::ENOSYS)
    }

    async fn interrupt(&self, _req: Request) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn bmap(
        &self,
        _req: Request,
        _inode: u64,
        _blocksize: u32,
        _idx: u64,
    ) -> Result<ReplyBmap> {
        Err(libc::ENOSYS)
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
        Err(libc::ENOSYS)
    }

    async fn poll(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _kh: u64,
        _flags: u32,
    ) -> Result<ReplyPoll> {
        Err(libc::ENOSYS)
    }

    // TODO handle notify
    // async fn notify_reply(&self, )

    async fn batch_forget(&self, _req: Request, _inodes: &[u64]) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn fallocate(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: u64,
        _length: u64,
        _mode: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn readdirplus<I, S>(
        &self,
        _req: Request,
        _parent: u64,
        _fh: u64,
        _offset: u64,
        _lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus<I, S>>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = DirectoryEntryPlus<S>>,
    {
        Err(libc::ENOSYS)
    }

    async fn rename2(
        &self,
        _req: Request,
        _parent: u64,
        _name: OsString,
        _new_parent: u64,
        _new_name: OsString,
        _flags: u32,
    ) -> Result<()> {
        Err(libc::ENOSYS)
    }

    async fn lseek(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _offset: u64,
        _whence: u32,
    ) -> Result<ReplyLSeek> {
        Err(libc::ENOSYS)
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
        Err(libc::ENOSYS)
    }

    // TODO setupmapping and removemapping
}
