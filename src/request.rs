use crate::abi::fuse_in_header;

#[derive(Debug, Copy, Clone)]
pub struct Request {
    pub unique: u64,
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
}

impl From<&fuse_in_header> for Request {
    fn from(header: &fuse_in_header) -> Self {
        Self {
            unique: header.unique,
            uid: header.uid,
            gid: header.gid,
            pid: header.pid,
        }
    }
}
