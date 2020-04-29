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

    pub(crate) read_only: Option<bool>,

    pub(crate) default_permissions: bool,

    pub(crate) nonempty: bool,
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

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only.replace(read_only);

        self
    }

    pub fn default_permissions(mut self, default_permissions: bool) -> Self {
        self.default_permissions = default_permissions;

        self
    }

    pub fn nonempty(mut self, nonempty: bool) -> Self {
        self.nonempty = nonempty;

        self
    }

    pub(crate) fn build(&self, fd: i32) -> OsString {
        let mut opts = vec![
            format!("fd={}", fd),
            format!("user_id={}", self.uid.unwrap_or(unistd::getuid().as_raw())),
            format!("group_id={}", self.gid.unwrap_or(unistd::getgid().as_raw())),
            format!("rootmode={}", self.rootmode.unwrap_or(40000)),
        ];

        if self.allow_root {
            opts.push("allow_root".to_string());
        }

        if self.allow_other {
            opts.push("allow_other".to_string());
        }

        if matches!(self.read_only, Some(true)) {
            opts.push("ro".to_string());
        }

        if self.default_permissions {
            opts.push("default_permissions".to_string());
        }

        OsString::from(opts.join(","))
    }
}
