use crate::MIN_USER_HASH_LEN;
use std::{fs::{self, File}, fmt};
use crate::store::{
    StoreBackend,
    ObjectId,
    pack::parse_pack_idx
};
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

    candidates.append(&mut resolve_id_loose(id_str));
    candidates.append(&mut resolve_id_packed(id_str));

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

fn resolve_id_loose(id_str: &str) -> Vec<ObjectId> {
    let mut matches = Vec::new();

    match_loose_ids(&mut matches, id_str);

    matches
}

fn match_loose_ids(matches: &mut Vec<ObjectId>, target_id: &str) -> Option<()> {
    let first_byte_str = &target_id[..2];
    let end_bytes_str = &target_id[2..];

    let obj_dir = format!(".git/objects/{}/", first_byte_str);

    let contents = fs::read_dir(obj_dir).ok()?;

    for entry in contents {
        let entry = entry.ok()?;

        let filename = entry
            .file_name()
            .into_string()
            .ok()?;

        if filename.starts_with(end_bytes_str) {
            let id_str_full = format!("{first_byte_str}{filename}");

            let id = id_str_full.try_into().ok()?;

            matches.push(id);
        }
    }

    Some(())
}

fn resolve_id_packed(id_str: &str) -> Vec<ObjectId> {
    let mut matches = Vec::new();

    match_pack_idx_ids(&mut matches, id_str);

    matches
}

fn match_pack_idx_ids(matches: &mut Vec<ObjectId>, target_id: &str) -> Option<()> {
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

        let target_id = hex::decode(target_id).ok()?;

        for oid in objectids {
            if oid.0.starts_with(&target_id) {
                matches.push(oid);
            }
        }
    }

    Some(())
}

// TODO: implement this
pub fn find_backend(_id: ObjectId) -> Option<StoreBackend> {
    Some(StoreBackend::Loose)
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

impl From<[u8; 20]> for ObjectId {
    fn from(value: [u8; 20]) -> ObjectId {
        ObjectId(value)
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
