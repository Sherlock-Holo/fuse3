//! inode based
//!
//! it is not recommend to use this inode-based [`Filesystem`] as you need to handle inode
//! allocate, recycle and sometimes map to the path. [`PathFilesystem`][crate::path::PathFilesystem]
//! helps you do those jobs so you can pay more attention to your filesystem design. However, if you
//! want to control the inode or do the path<->inode map on yourself, then [`Filesystem`] is
//! the one to choose.

use bytes::Bytes;
pub use filesystem::Filesystem;
use futures_util::future::Either;
pub use request::Request;
#[cfg(any(feature = "async-io-runtime", feature = "tokio-runtime"))]
pub use session::{MountHandle, Session};

pub(crate) type FuseData = Either<Vec<u8>, (Vec<u8>, Bytes)>;

pub(crate) mod abi;
mod connection;
mod filesystem;
pub mod flags;
pub mod reply;
mod request;
pub(crate) mod session;

pub mod prelude {
    pub use super::reply::FileAttr;
    pub use super::reply::*;
    pub use super::Filesystem;
    pub use super::Request;
    pub use super::Session;
    pub use crate::notify::Notify;
    pub use crate::FileType;
    pub use crate::SetAttr;
}
