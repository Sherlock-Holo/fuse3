# fuse3
an async version fuse library for rust

## feature

- support unprivileged mode by using `fusermount3`
- support `readdirplus` to improve read dir performance
- support posix file lock
- support handles the `O_TRUNC` open flag
- support direct IO
- support enable `no_open` and `no_open_dir` option

## still not support
- async DIO
- `ioctl` implement
- fuseblk mode
- macos support

## may not work well
- `poll`

## License

MIT