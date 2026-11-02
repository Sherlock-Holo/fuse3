# fuse3
an async version fuse library for rust

[![Cargo](https://img.shields.io/crates/v/fuse3.svg)](
https://crates.io/crates/fuse3)
[![Documentation](https://docs.rs/fuse3/badge.svg)](
https://docs.rs/fuse3)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](
https://github.com/Sherlock-Holo/fuse3)

## feature

- support unprivileged mode by using `fusermount3`
- support `readdirplus` to improve read dir performance
- support posix file lock
- support handles the `O_TRUNC` open flag
- support async direct IO
- support enable `no_open` and `no_open_dir` option

## still not support

- `ioctl` implement
- fuseblk mode

## unstable

- `poll`
- `notify_reply`

## known limits of the path layer

The path layer (`PathFilesystem` on top of the inode bridge) counts kernel
lookups per inode and keeps an inode number until the kernel forgets it. These
cases are still open:

- `link` gives the new name its own inode number instead of the number of the
  linked inode, so the kernel sees two inodes for one file.
- `PathFilesystem::batch_forget` only gets paths, not the `nlookup` of each
  inode, so a path file system cannot follow partial forgets.
- If a reply cannot be delivered to the kernel, the lookup counts taken for it
  are not given back, so those inode numbers stay taken until unmount.

## Supported Rust Versions

The minimum supported version is 1.75.

## License

MIT
