use std::cmp::Ordering;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::helper::Apply;

#[derive(Debug, Clone)]
enum InnerPath {
    Root,
    Child {
        parent: Box<InnerPath>,
        name: Arc<PathBuf>,
    },
}

impl PartialEq for InnerPath {
    fn eq(&self, other: &Self) -> bool {
        // quick path for root /
        if matches!(self, Self::Root) && matches!(other, Self::Root) {
            true
        } else {
            self.absolute_path() == other.absolute_path()
        }
    }
}

impl Eq for InnerPath {}

impl Hash for InnerPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // quick path for root /
        if matches!(self, Self::Root) {
            Path::new("/").hash(state)
        } else {
            self.absolute_path().hash(state)
        }
    }
}

impl PartialOrd for InnerPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // quick path for root /
        if matches!(self, Self::Root) && matches!(other, Self::Root) {
            return Some(Ordering::Equal);
        }

        self.absolute_path().partial_cmp(&other.absolute_path())
    }
}

impl Ord for InnerPath {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl InnerPath {
    fn absolute_path(&self) -> PathBuf {
        match self {
            Self::Root => PathBuf::from("/"),
            Self::Child { parent, name } => parent
                .absolute_path()
                .apply(|path| path.push(name.as_path())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AbsolutePath(InnerPath);

impl AbsolutePath {
    pub fn root() -> Self {
        Self(InnerPath::Root)
    }

    pub fn new(parent: &AbsolutePath, name: &OsStr) -> Self {
        match &parent.0 {
            InnerPath::Root => Self(InnerPath::Child {
                parent: Box::new(InnerPath::Root),
                name: Arc::new(PathBuf::from(name.to_owned())),
            }),
            parent @ InnerPath::Child { .. } => Self(InnerPath::Child {
                parent: Box::new(parent.clone()),
                name: Arc::new(PathBuf::from(name.to_owned())),
            }),
        }
    }

    /*pub fn name(&self) -> &Path {
        match &self.0 {
            InnerPath::Root => Path::new("/"),
            InnerPath::Child { name, .. } => Path::new(name.as_os_str()),
        }
    }*/

    pub fn absolute_path_buf(&self) -> PathBuf {
        self.0.absolute_path()
    }

    pub fn parent(&self) -> Option<AbsolutePath> {
        match &self.0 {
            InnerPath::Root => None,
            parent @ InnerPath::Child { .. } => Some(AbsolutePath(parent.clone())),
        }
    }
}

impl PartialEq for AbsolutePath {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl Eq for AbsolutePath {}

impl Hash for AbsolutePath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl PartialOrd for AbsolutePath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for AbsolutePath {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}
