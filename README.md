# fuse3
an async version fuse library for rust

## feature

- support unprivileged mode by using `fusermount3`
- support `readdirplus` to improve read dir performance
- support posix file lock
- support handles the O_TRUNC open flag
- support enable no_open and no_open_dir option

## don't support
- async dio
- `ioctl` implement

## may not work well
- `poll`
