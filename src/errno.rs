use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::Error as IoError;
use std::os::raw::c_int;

use nix::Error as NixError;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// linux errno wrap.
pub struct Errno(pub c_int);

impl From<Errno> for c_int {
    fn from(errno: Errno) -> Self {
        -errno.0
    }
}

impl From<c_int> for Errno {
    fn from(errno: c_int) -> Self {
        Self(errno)
    }
}

/// When raw os error is undefined, will return Errno(libc::EIO)
impl From<IoError> for Errno {
    fn from(err: IoError) -> Self {
        if let Some(errno) = err.raw_os_error() {
            Self(errno)
        } else {
            Self(libc::EIO)
        }
    }
}

impl From<Errno> for IoError {
    fn from(errno: Errno) -> Self {
        IoError::from_raw_os_error(errno.0)
    }
}

impl From<NixError> for Errno {
    fn from(err: NixError) -> Self {
        match err {
            NixError::Sys(errno) => Errno(errno as libc::c_int),
            NixError::InvalidPath | NixError::InvalidUtf8 => Errno(libc::EINVAL),
            NixError::UnsupportedOperation => Errno(libc::ENOTSUP),
        }
    }
}

impl From<Errno> for NixError {
    fn from(errno: Errno) -> Self {
        NixError::from_errno(nix::errno::from_i32(errno.0))
    }
}

impl Display for Errno {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "errno is {}", self.0)
    }
}

impl Error for Errno {}
