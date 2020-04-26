use std::ffi::OsString;

use nix::unistd;

#[derive(Debug, Clone, Default)]
pub struct MountOption {
    pub(crate) uid: Option<u32>,
    pub(crate) gid: Option<u32>,

    pub(crate) fs_name: Option<OsString>,

    // default 40000
    pub(crate) rootmode: Option<u32>,

    pub(crate) allow_root: bool,
    pub(crate) allow_other: bool,
}

impl MountOption {
    pub fn uid(mut self, uid: u32) -> Self {
        self.uid.replace(uid);

        self
    }

    pub fn gid(mut self, gid: u32) -> Self {
        self.gid.replace(gid);

        self
    }

    pub fn fs_name(mut self, name: impl Into<OsString>) -> Self {
        self.fs_name.replace(name.into());

        self
    }

    pub fn rootmode(mut self, rootmode: u32) -> Self {
        self.rootmode.replace(rootmode);

        self
    }

    pub fn allow_root(mut self, allow_root: bool) -> Self {
        self.allow_root = allow_root;

        self
    }

    pub fn allow_other(mut self, allow_other: bool) -> Self {
        self.allow_other = allow_other;

        self
    }

    pub(crate) fn build(&self, fd: i32) -> OsString {
        let mut opts = OsString::new();

        opts.push(format!("fd={},", fd));

        let uid = self.uid.unwrap_or(unistd::getuid().as_raw());

        opts.push(format!("user_id={},", uid));

        let gid = self.gid.unwrap_or(unistd::getgid().as_raw());

        opts.push(format!("group_id={},", gid));

        let rootmode = self.rootmode.unwrap_or(40000);

        opts.push(format!("rootmode={},", rootmode));

        if self.allow_root {
            opts.push("allow_root,");
        }

        if self.allow_other {
            opts.push("allow_other");
        }

        opts
    }
}
