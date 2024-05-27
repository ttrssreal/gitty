mod loose;
mod pack;
mod object;
pub mod util;

use std::fmt::Display;
use std::option::Option;

use crate::store::{
    loose::get_loose_object,
    pack::get_packed_object
};

use crate::SHA1_HASH_SIZE;

/// The primary interface into the git object store
pub struct GitObjectStore;

#[derive(Debug)]
pub enum StoreBackend {
    Loose,
    Packed
}

#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct ObjectId([u8; SHA1_HASH_SIZE]);

#[derive(Debug)]
pub struct GitObject {
    /// The sha1 hash corrosponding to the object
    pub id: ObjectId,

    /// The size of the raw uncompresed object
    pub size: usize,

    pub data: GitObjectData,
}

#[derive(Debug, PartialEq)]
pub enum GitObjectData {
    Blob {
        data: Vec<u8>,
    },
    Tree {
        entries: Vec<TreeEntry>,
    },
    Commit {
        tree: ObjectId,
        parents: Vec<ObjectId>,
        // Assuming git doesn't support multiple authors/committers
        author: String,
        committer: String,
        encoding: Option<String>,
        gpgsig: Option<String>,
        message: Vec<u8>,
    },
    Tag {
        object: ObjectId,
        kind: String,
        tag: String,
        tagger: String,
        // If signed, the signature resides in the message itself
        message: Vec<u8>,
    }
}

// "Each entry has a sha1 identifier, pathname and mode."
#[derive(Debug, PartialEq)]
pub struct TreeEntry {
    pub mode: u32,
    pub kind: String,
    pub path: String,
    pub id: ObjectId,
}

impl GitObject {
    pub fn type_str(&self) -> &str {
        match self.data {
            GitObjectData::Blob { .. } => "blob",
            GitObjectData::Tree { .. } => "tree",
            GitObjectData::Commit { .. } => "commit",
            GitObjectData::Tag { .. } => "tag",
        }
    }
}

impl Display for TreeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:06o} {} {} {}", self.mode, self.kind, self.id, self.path)
    }
}

impl Display for GitObject {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.data {
            GitObjectData::Blob {
                data
            } => {
                write!(f, "{}", String::from_utf8_lossy(&data))
            },
            GitObjectData::Tree {
                entries
            } => {
                for entry in entries {
                    writeln!(f, "{}", entry)?;
                }
                Ok(())
            },
            GitObjectData::Commit {
                tree,
                parents,
                author,
                committer,
                encoding,
                gpgsig,
                message
            } => {
                writeln!(f, "tree {}", tree)?;
                for parent in parents {
                    writeln!(f, "parent {}", parent)?;
                }
                writeln!(f, "author {}", author)?;
                writeln!(f, "committer {}", committer)?;
                if let Some(encoding) = encoding {
                    writeln!(f, "encoding {}", encoding)?;
                }
                if let Some(gpgsig) = gpgsig {
                    writeln!(f, "gpgsig {}", gpgsig)?;
                }
                write!(f, "\n")?;
                write!(f, "{}", String::from_utf8_lossy(message))
            },
            GitObjectData::Tag { object, kind, tag, tagger, message } => {
                write!(f, "object {}\n", object)?;
                write!(f, "type {}\n", kind)?;
                write!(f, "tag {}\n", tag)?;
                write!(f, "tagger {}\n", tagger)?;
                write!(f, "\n")?;
                write!(f, "{}", String::from_utf8_lossy(message))
            },
        }
    }
}

impl GitObjectStore {
    /// Retrives the object keyed by the SHA1 hash `id`
    /// from the git object store.
    ///
    /// This will work reguardless of the format the object currently
    /// is stored in, eg. loose or packed.
    pub fn get(id: ObjectId) -> Option<GitObject> {
        use StoreBackend::*;

        match util::find_backend(id)? {
            Loose => get_loose_object(id),
            Packed => get_packed_object(id)
        }
    }

}

