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
- macos support

## unstable
- `poll`

## License

MIT