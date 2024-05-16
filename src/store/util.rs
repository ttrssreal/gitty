use crate::MIN_USER_HASH_LEN;
use std::{fs::{self, File}, fmt};
use std::ops::Deref;
use crate::store::{
    StoreBackend,
    ObjectId,
    pack::parse_pack_idx
};
use hex::FromHexError;
use std::path::Path;
use crate::SHA1_HASH_SIZE;
use std::array::TryFromSliceError;

// Resolves an arbitrary length hex encoded string to an oid
pub fn resolve_id(id_str: &str) -> Option<ObjectId> {
    let id_len = id_str.len();

    if id_len < MIN_USER_HASH_LEN || id_len > SHA1_HASH_SIZE * 2 {
        eprintln!("Invalid hash length.");
        return None;
    };

    let mut candidates = Vec::new();

    let id_bytes = match hex::decode(id_str) {
        Ok(id_bytes) => id_bytes,
        Err(FromHexError::OddLength) => {
            let adjusted = &id_str[..id_len-1];

            let resolved_adjusted = resolve_id(adjusted);

            if resolved_adjusted.is_some() {
                eprintln!("Odd Length, truncating... \"{}\"", adjusted);
            }

            return resolved_adjusted;
        },
        Err(FromHexError::InvalidHexCharacter {
            c,
            ..
        }) => {
            eprintln!("Invalid hex character: {}", c);

            return None;
        }
        _ => return None
    };

    let first_byte = id_bytes[0];
    let first_byte_hint = Some(first_byte);

    let mut match_beginning = |oid: ObjectId| {
        if oid.0.starts_with(&id_bytes) {
            candidates.push(oid);
        }
    };

    visit_loose_ids(first_byte_hint, &mut match_beginning);
    visit_pack_ids(&mut match_beginning);

    if candidates.len() == 0 {
        eprintln!("Can't find object");
        return None;
    }

    if candidates.len() > 1 {
        eprintln!("Object Id is ambiguous");
        eprintln!("Found:");
        for candidate in candidates {
            eprintln!(" - {candidate}");
        }
        return None;
    }

    return candidates.into_iter().next();
}

fn visit_loose_ids<T>(first_byte_hint: Option<u8>, mut visit: T) -> Option<()>
where
    T: FnMut(ObjectId)
{
    let mut visit_obj_dir = |obj_dir_path: &Path| -> Option<()> {
        let contents = fs::read_dir(obj_dir_path).ok()?;

        for entry in contents {
            let entry = entry.ok()?;

            let filename = entry
                .file_name()
                .into_string()
                .ok()?;

            let obj_path = entry.path();
            let parent_path = obj_path.parent()?.to_str()?;
            let first_byte = &parent_path[parent_path.len()-2..];

            let id_str_full = format!("{first_byte}{filename}");
            let id = id_str_full.try_into().ok()?;

            visit(id);
        }

        Some(())
    };

    match first_byte_hint {
        Some(first_byte) => {
            let obj_dir = format!(".git/objects/{:02x}/", first_byte);

            visit_obj_dir(Path::new(&obj_dir))?;
        },
        None => {
            let store_dir = fs::read_dir(".git/objects/").ok()?;

            for dir_ent in store_dir {
                let dir_ent = dir_ent.ok()?;

                let file_type = dir_ent.file_type().ok()?;

                if !file_type.is_dir() {
                    continue;
                }

                let obj_dir_path = dir_ent.path();

                visit_obj_dir(&obj_dir_path)?;
            }
        }
    }

    Some(())
}

fn visit_pack_ids<T>(mut visit: T) -> Option<()>
where
    T: FnMut(ObjectId)
{
    let idx_files = fs::read_dir(".git/objects/pack/").ok()?;

    for entry in idx_files {
        let entry = entry.ok()?;

        let filename = entry
            .file_name()
            .into_string()
            .ok()?;

        let is_idxfile = filename
            .to_lowercase().ends_with(".idx");

        if !is_idxfile {
            continue;
        }

        let filename = format!(".git/objects/pack/{}", filename);

        let file_stream = File::open(filename).ok()?;

        // TODO: fix: we disregard offsets, and therefore do unnecessary work here :(
        let pack_idx = parse_pack_idx(file_stream)?;

        let objectids: Vec<ObjectId> = pack_idx.locations
            .into_keys()
            .collect();

        for oid in objectids {
            visit(oid)
        }
    }

    Some(())
}

pub fn find_backend(id: ObjectId) -> Option<StoreBackend> {
    let mut backend = None;

    let first_byte = id[0];
    let first_byte_hint = Some(first_byte);

    visit_loose_ids(first_byte_hint, |oid| {
        if oid == id {
            backend = Some(StoreBackend::Loose);
        }
    });

    visit_pack_ids(|oid| {
        if oid == id {
            backend = Some(StoreBackend::Packed);
        }
    });

    backend
}

/// From a hex string
impl TryFrom<String> for ObjectId {
    type Error = hex::FromHexError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut id = [0u8; SHA1_HASH_SIZE];
        hex::decode_to_slice(value, &mut id as &mut [u8])?;
        Ok(ObjectId(id))
    }
}

/// From raw bytes
impl TryFrom<&[u8]> for ObjectId {
    type Error = TryFromSliceError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let id: [u8; SHA1_HASH_SIZE] = value.try_into()?;
        Ok(ObjectId(id))
    }
}

impl From<[u8; SHA1_HASH_SIZE]> for ObjectId {
    fn from(value: [u8; SHA1_HASH_SIZE]) -> ObjectId {
        ObjectId(value)
    }
}

impl Deref for ObjectId {
    type Target = [u8; SHA1_HASH_SIZE];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}


impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}
