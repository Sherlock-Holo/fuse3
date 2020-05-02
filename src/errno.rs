use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::Error as IoError;
use std::os::raw::c_int;

#[derive(Debug, Copy, Clone)]
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

impl Display for Errno {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "errno is {}", self.0)
    }
}

impl Error for Errno {}
