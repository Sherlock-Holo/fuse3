use std::ffi::OsStr;

use async_trait::async_trait;
use bytes::Bytes;

use crate::{Result, SetAttr};
use crate::notify::Notify;
use crate::reply::*;
use crate::request::Request;

#[async_trait]
/// Filesystem trait.
///
/// # Notes:
///
/// this trait is defined with async_trait, you can use
/// [`async_trait`](https://docs.rs/async-trait) to implement it, or just implement it directly.
pub trait Filesystem {
    /// initialize filesystem. Called before any other filesystem method.
    async fn init(&self, req: Request) -> Result<()>;

    /// clean up filesystem. Called on filesystem exit.
    async fn destroy(&self, req: Request);

    /// look up a directory entry by name and get its attributes.
    async fn lookup(&self, _req: Request, _parent: u64, _name: &OsStr) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    /// forget an inode. The nlookup parameter indicates the number of lookups previously
    /// performed on this inode. If the filesystem implements inode lifetimes, it is recommended
    /// that inodes acquire a single reference on each lookup, and lose nlookup references on each
    /// forget. The filesystem may ignore forget calls, if the inodes don't need to have a limited
    /// lifetime. On unmount it is not guaranteed, that all referenced inodes will receive a forget
    /// message.
    async fn forget(&self, _req: Request, _inode: u64, _nlookup: u64) {}

    /// get file attributes. If `fh` is None, means `fh` is not set.
    async fn getattr(
        &self,
        _req: Request,
        _inode: u64,
        _fh: Option<u64>,
        _flags: u32,
    ) -> Result<ReplyAttr> {
        Err(libc::ENOSYS.into())
    }

    /// set file attributes.  If `fh` is None, means `fh` is not set.
    async fn setattr(
        &self,
        _req: Request,
        _inode: u64,
        _fh: Option<u64>,
        _set_attr: SetAttr,
    ) -> Result<ReplyAttr> {
        Err(libc::ENOSYS.into())
    }

    /// read symbolic link.
    async fn readlink(&self, _req: Request, _inode: u64) -> Result<ReplyData> {
        Err(libc::ENOSYS.into())
    }

    /// create a symbolic link.
    async fn symlink(
        &self,
        _req: Request,
        _parent: u64,
        _name: &OsStr,
        _link: &OsStr,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    /// create file node. Create a regular file, character device, block device, fifo or socket
    /// node. When creating file, most cases user only need to implement [`create`].
    ///
    /// [`create`]: Filesystem::create
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

    /// create a directory.
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

    /// remove a file.
    async fn unlink(&self, _req: Request, _parent: u64, _name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    /// remove a directory.
    async fn rmdir(&self, _req: Request, _parent: u64, _name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    /// rename a file or directory.
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

    /// create a hard link.
    async fn link(
        &self,
        _req: Request,
        _inode: u64,
        _new_parent: u64,
        _new_name: &OsStr,
    ) -> Result<ReplyEntry> {
        Err(libc::ENOSYS.into())
    }

    /// open a file. Open flags (with the exception of `O_CREAT`, `O_EXCL` and `O_NOCTTY`) are
    /// available in flags. Filesystem may store an arbitrary file handle (pointer, index, etc) in
    /// fh, and use this in other all other file operations (read, write, flush, release, fsync).
    /// Filesystem may also implement stateless file I/O and not store anything in fh. There are
    /// also some flags (`direct_io`, `keep_cache`) which the filesystem may set, to change the way
    /// the file is opened.
    ///
    /// # Notes:
    ///
    /// See `fuse_file_info` structure in
    /// [fuse_common.h](https://libfuse.github.io/doxygen/include_2fuse__common_8h_source.html) for
    /// more details.
    async fn open(&self, _req: Request, _inode: u64, _flags: u32) -> Result<ReplyOpen> {
        Err(libc::ENOSYS.into())
    }

    /// read data. Read should send exactly the number of bytes requested except on EOF or error,
    /// otherwise the rest of the data will be substituted with zeroes. An exception to this is
    /// when the file has been opened in `direct_io` mode, in which case the return value of the
    /// read system call will reflect the return value of this operation. `fh` will contain the
    /// value set by the open method, or will be undefined if the open method didn't set any value.
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

    /// write data. Write should return exactly the number of bytes requested except on error. An
    /// exception to this is when the file has been opened in `direct_io` mode, in which case the
    /// return value of the write system call will reflect the return value of this operation. `fh`
    /// will contain the value set by the open method, or will be undefined if the open method
    /// didn't set any value.
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

    /// get filesystem statistics.
    async fn statsfs(&self, _req: Request, _inode: u64) -> Result<ReplyStatFs> {
        Err(libc::ENOSYS.into())
    }

    /// release an open file. Release is called when there are no more references to an open file:
    /// all file descriptors are closed and all memory mappings are unmapped. For every open call
    /// there will be exactly one release call. The filesystem may reply with an error, but error
    /// values are not returned to `close()` or `munmap()` which triggered the release. `fh` will
    /// contain the value set by the open method, or will be undefined if the open method didn't
    /// set any value. `flags` will contain the same flags as for open. `flush` means flush the
    /// data or not when closing file.
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

    /// synchronize file contents. If the `datasync` is true, then only the user data should be
    /// flushed, not the metadata.
    async fn fsync(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> Result<()> {
        Ok(())
    }

    /// set an extended attribute.
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

    /// remove an extended attribute.
    async fn removexattr(&self, _req: Request, _inode: u64, _name: &OsStr) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    /// flush method. This is called on each `close()` of the opened file. Since file descriptors
    /// can be duplicated (`dup`, `dup2`, `fork`), for one open call there may be many flush calls.
    /// Filesystems shouldn't assume that flush will always be called after some writes, or that if
    /// will be called at all. `fh` will contain the value set by the open method, or will be
    /// undefined if the open method didn't set any value.
    ///
    /// # Notes:
    ///
    /// the name of the method is misleading, since (unlike fsync) the filesystem is not forced to
    /// flush pending writes. One reason to flush data, is if the filesystem wants to return write
    /// errors. If the filesystem supports file locking operations ([`setlk`], [`getlk`]) it should
    /// remove all locks belonging to `lock_owner`.
    ///
    /// [`setlk`]: Filesystem::setlk
    /// [`getlk`]: Filesystem::getlk
    async fn flush(&self, _req: Request, _inode: u64, _fh: u64, _lock_owner: u64) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    /// open a directory. Filesystem may store an arbitrary file handle (pointer, index, etc) in
    /// `fh`, and use this in other all other directory stream operations
    /// ([`readdir`], [`releasedir`], [`fsyncdir`]). Filesystem may also implement stateless
    /// directory I/O and not store anything in `fh`, though that makes it impossible to implement
    /// standard conforming directory stream operations in case the contents of the directory can
    /// change between `opendir` and [`releasedir`].
    ///
    /// [`readdir`]: Filesystem::readdir
    /// [`releasedir`]: Filesystem::releasedir
    /// [`fsyncdir`]: Filesystem::fsyncdir
    /// [`releasedir`]: Filesystem::releasedir
    async fn opendir(&self, _req: Request, _inode: u64, _flags: u32) -> Result<ReplyOpen> {
        Ok(ReplyOpen { fh: 0, flags: 0 })
    }

    /// read directory. `offset` is used to track the offset of the directory entries. `fh` will
    /// contain the value set by the [`opendir`] method, or will be undefined if the [`opendir`]
    /// method didn't set any value.
    ///
    /// [`opendir`]: Filesystem::opendir
    async fn readdir(
        &self,
        _req: Request,
        _parent: u64,
        _fh: u64,
        _offset: i64,
    ) -> Result<ReplyDirectory> {
        Err(libc::ENOSYS.into())
    }

    /// release an open directory. For every [`opendir`] call there will be exactly one
    /// `releasedir` call. `fh` will contain the value set by the [`opendir`] method, or will be
    /// undefined if the [`opendir`] method didn't set any value.
    ///
    /// [`opendir`]: Filesystem::opendir
    async fn releasedir(&self, _req: Request, _inode: u64, _fh: u64, _flags: u32) -> Result<()> {
        Ok(())
    }

    /// synchronize directory contents. If the `datasync` is true, then only the directory contents
    /// should be flushed, not the metadata. `fh` will contain the value set by the [`opendir`]
    /// method, or will be undefined if the [`opendir`] method didn't set any value.
    ///
    /// [`opendir`]: Filesystem::opendir
    async fn fsyncdir(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    #[cfg(feature = "file-lock")]
    /// test for a POSIX file lock.
    ///
    /// # Notes:
    ///
    /// this is supported on enable **`file-lock`** feature.
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
    /// acquire, modify or release a POSIX file lock.
    ///
    /// # Notes:
    ///
    /// this is supported on enable **`file-lock`** feature.
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

    /// check file access permissions. This will be called for the `access()` system call. If the
    /// `default_permissions` mount option is given, this method is not be called. This method is
    /// not called under Linux kernel versions 2.4.x.
    async fn access(&self, _req: Request, _inode: u64, _mask: u32) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    /// create and open a file. If the file does not exist, first create it with the specified
    /// mode, and then open it. Open flags (with the exception of `O_NOCTTY`) are available in
    /// flags. Filesystem may store an arbitrary file handle (pointer, index, etc) in `fh`, and use
    /// this in other all other file operations
    /// ([`read`], [`write`], [`flush`], [`release`], [`fsync`]). There are also some flags
    /// (`direct_io`, `keep_cache`) which the filesystem may set, to change the way the file is
    /// opened. If this method is not implemented or under Linux kernel versions earlier than
    /// 2.6.15, the [`mknod`] and [`open`] methods will be called instead.
    ///
    /// # Notes:
    ///
    /// See `fuse_file_info` structure in
    /// [fuse_common.h](https://libfuse.github.io/doxygen/include_2fuse__common_8h_source.html) for
    /// more details.
    ///
    /// [`read`]: Filesystem::read
    /// [`write`]: Filesystem::write
    /// [`flush`]: Filesystem::flush
    /// [`release`]: Filesystem::release
    /// [`fsync`]: Filesystem::fsync
    /// [`mknod`]: Filesystem::mknod
    /// [`open`]: Filesystem::open
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

    /// handle interrupt. When a operation is interrupted, an interrupt request will send to fuse
    /// server with the unique id of the operation.
    async fn interrupt(&self, _req: Request, _unique: u64) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    /// map block index within file to block index within device.
    ///
    /// # Notes:
    ///
    /// This may not works because currently this crate doesn't support fuseblk mode.
    async fn bmap(
        &self,
        _req: Request,
        _inode: u64,
        _blocksize: u32,
        _idx: u64,
    ) -> Result<ReplyBmap> {
        Err(libc::ENOSYS.into())
    }

    /*async fn ioctl(
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
    }*/

    /// poll for IO readiness events.
    async fn poll(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _kh: Option<u64>,
        _flags: u32,
        _events: u32,
        _notify: &Notify,
    ) -> Result<ReplyPoll> {
        Err(libc::ENOSYS.into())
    }

    /// receive notify reply from kernel.
    async fn notify_reply(
        &self,
        _req: Request,
        _inode: u64,
        _offset: u64,
        _data: Bytes,
    ) -> Result<()> {
        Err(libc::ENOSYS.into())
    }

    /// forget more than one inode. This is a batch version [`forget`]
    ///
    /// [`forget`]: Filesystem::forget
    async fn batch_forget(&self, _req: Request, _inodes: &[u64]) {}

    /// allocate space for an open file. This function ensures that required space is allocated for
    /// specified file.
    ///
    /// # Notes:
    ///
    /// more infomation about `fallocate`, please see **`man 2 fallocate`**
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

    /// read directory entries, but with their attribute, like [`readdir`] + [`lookup`] at the same
    /// time.
    ///
    /// [`readdir`]: Filesystem::readdir
    /// [`lookup`]: Filesystem::lookup
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

    /// rename a file or directory with flags.
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

    /// find next data or hole after the specified offset.
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

    /// copy a range of data from one file to another. This can improve performance because it
    /// reduce data copy: in normal, data will copy from FUSE server to kernel, then to user-space,
    /// then to kernel, finally send back to FUSE server. By implement this method, data will only
    /// copy in FUSE server internal.
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
