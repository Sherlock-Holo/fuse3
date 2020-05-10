use std::ffi::OsString;
use std::os::unix::io::RawFd;

use nix::unistd;

/// mount options.
#[derive(Debug, Clone, Default)]
pub struct MountOptions {
    // mount syscall data field option
    pub(crate) uid: Option<u32>,
    pub(crate) gid: Option<u32>,

    pub(crate) fs_name: Option<OsString>,

    // default 40000
    pub(crate) rootmode: Option<u32>,

    pub(crate) allow_root: bool,
    pub(crate) allow_other: bool,

    pub(crate) read_only: Option<bool>,

    // when run in privileged mode, it is lib self option
    pub(crate) nonempty: bool,

    // lib self option
    pub(crate) default_permissions: bool,

    pub(crate) dont_mask: bool,

    pub(crate) no_open_support: bool,
    pub(crate) no_open_dir_support: bool,

    pub(crate) handle_killpriv: bool,

    pub(crate) write_back: bool,

    pub(crate) custom_options: Option<OsString>,
}

impl MountOptions {
    /// set fuse filesystem mount `user_id`, default is current uid.
    pub fn uid(mut self, uid: u32) -> Self {
        self.uid.replace(uid);

        self
    }

    /// set fuse filesystem mount `group_id`, default is current gid.
    pub fn gid(mut self, gid: u32) -> Self {
        self.gid.replace(gid);

        self
    }

    /// set fuse filesystem name, default is **fuse**.
    pub fn fs_name(mut self, name: impl Into<OsString>) -> Self {
        self.fs_name.replace(name.into());

        self
    }

    /// set fuse filesystem `rootmode`, default is 40000.
    pub fn rootmode(mut self, rootmode: u32) -> Self {
        self.rootmode.replace(rootmode);

        self
    }

    /// set fuse filesystem `allow_root` mount option, default is disable.
    pub fn allow_root(mut self, allow_root: bool) -> Self {
        self.allow_root = allow_root;

        self
    }

    /// set fuse filesystem `allow_other` mount option, default is disable.
    pub fn allow_other(mut self, allow_other: bool) -> Self {
        self.allow_other = allow_other;

        self
    }

    /// set fuse filesystem `ro` mount option, default is disable.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only.replace(read_only);

        self
    }

    /// allow fuse filesystem mount on a non-empty directory, default is not allowed.
    pub fn nonempty(mut self, nonempty: bool) -> Self {
        self.nonempty = nonempty;

        self
    }

    /// set fuse filesystem `default_permissions` mount option, default is disable.
    ///
    /// When `default_permissions` is set, the [`access`] is useless.
    ///
    /// [`access`]: crate::Filesystem::access
    pub fn default_permissions(mut self, default_permissions: bool) -> Self {
        self.default_permissions = default_permissions;

        self
    }

    /// don't apply umask to file mode on create operations, default is disable.
    pub fn dont_mask(mut self, dont_mask: bool) -> Self {
        self.dont_mask = dont_mask;

        self
    }

    /// make kernel support zero-message opens, default is disable
    pub fn no_open_support(mut self, no_open_support: bool) -> Self {
        self.no_open_support = no_open_support;

        self
    }

    /// make kernel support zero-message opendir, default is disable
    pub fn no_open_dir_support(mut self, no_open_dir_support: bool) -> Self {
        self.no_open_dir_support = no_open_dir_support;

        self
    }

    /// fs handle killing `suid`/`sgid`/`cap` on `write`/`chown`/`trunc`, default is disable.
    pub fn handle_killpriv(mut self, handle_killpriv: bool) -> Self {
        self.handle_killpriv = handle_killpriv;

        self
    }

    /// enable write back cache for buffered writes, default is disable.
    ///
    /// # Notes:
    ///
    /// if enable this feature, when write flags has `FUSE_WRITE_CACHE`, file handle is guessed.
    pub fn write_back(mut self, write_back: bool) -> Self {
        self.write_back = write_back;

        self
    }

    /// set custom options for fuse filesystem, the custom options will be used in mount
    pub fn custom_options(mut self, custom_options: impl Into<OsString>) -> Self {
        self.custom_options = Some(custom_options.into());

        self
    }

    pub(crate) fn build(&mut self, fd: RawFd) -> OsString {
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

        let mut options = OsString::from(opts.join(","));

        if let Some(custom_options) = &self.custom_options {
            options.push(",");
            options.push(custom_options);
        }

        options
    }

    #[cfg(feature = "unprivileged")]
    pub(crate) fn build_with_unprivileged(&self) -> OsString {
        let mut opts = vec![
            format!("user_id={}", self.uid.unwrap_or(unistd::getuid().as_raw())),
            format!("group_id={}", self.gid.unwrap_or(unistd::getgid().as_raw())),
            format!("rootmode={}", self.rootmode.unwrap_or(40000)),
            format!(
                "fsname={:?}",
                self.fs_name.as_ref().unwrap_or(&OsString::from("fuse"))
            ),
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

        let mut options = OsString::from(opts.join(","));

        if let Some(custom_options) = &self.custom_options {
            options.push(",");
            options.push(custom_options);
        }

        options
    }
}
