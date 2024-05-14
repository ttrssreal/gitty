use std::fs::File;
use std::collections::HashMap;
use crate::store::ObjectId;
use std::io::{BufReader, Read};
use byteorder::{BigEndian, ReadBytesExt};

// A 4-byte magic number \377tOc
const PACK_IDX_MAGIC: u32 = 0xff744f63;

pub struct GitPackIdx {
    // A map of ObjectId's to object offsets within a packfile
    pub locations: HashMap<ObjectId, usize>
}

pub fn parse_pack_idx(idx_file_stream: File) -> Option<GitPackIdx> {
    let mut idx_reader = BufReader::new(idx_file_stream);

    let first_word = idx_reader.read_u32::<BigEndian>().ok()?;

    match first_word {
        PACK_IDX_MAGIC => parse_pack_idx_modern(idx_reader),
        _ => parse_pack_idx_legacy(idx_reader, first_word)
    }
}

// Pack idx v1
// I haven't found any v1 idx files to test with :(
// hopefully works first time!
pub fn parse_pack_idx_legacy(mut idx_reader: BufReader<File>, _fanout_zero: u32) -> Option<GitPackIdx> {

    let mut locations = HashMap::new();
    let mut oid = [0u8; 20];

    // The header consists of 256 4-byte network byte order integers. N-th entry
    // of this table records the number of objects in the corresponding pack, the
    // first byte of whose object name is less than or equal to N. This is called
    // the first-level fan-out table.
    // TODO: actually use this for binary searches etc
    idx_reader.seek_relative(254 * 4).ok()?; //  256 - 2 words for the magic check + final entry

    let oid_entry_count = idx_reader.read_u32::<BigEndian>().ok()?;

    // The header is followed by sorted 24-byte entries, one entry per object in
    // the pack. Each entry is:
    for _ in 0..oid_entry_count {
        // 4-byte network byte order integer, recording where the
        // object is stored in the packfile as the offset from the
        // beginning.
        let offset = idx_reader.read_u32::<BigEndian>().ok()?;

        // one object name of the appropriate size.
        idx_reader.read_exact(&mut oid).ok()?;
        let oid: ObjectId = oid.into();

        locations.insert(oid, offset as usize);
    }

    Some(GitPackIdx {
        locations
    })
}

pub fn parse_pack_idx_modern(mut idx_reader: BufReader<File>) -> Option<GitPackIdx> {
    // A 4-byte version number
    let version_number = idx_reader.read_u32::<BigEndian>().ok()?;

    match version_number {
        2 => parse_pack_idx_v2(idx_reader),
        _ => {
            eprintln!("Gitty currently supports only pack idx formats of v{{1,2}}");
            None
        }
    }
}

// Pack idx v2
pub fn parse_pack_idx_v2(mut idx_reader: BufReader<File>) -> Option<GitPackIdx> {
    // A 256-entry fan-out table just like v1.
    // TODO: actually use this for binary searches etc
    idx_reader.seek_relative(255 * 4).ok()?;

    let oid_entry_count = idx_reader.read_u32::<BigEndian>().ok()?;

    let mut locations = HashMap::new();
    let mut oids = Vec::new();
    let mut oid = [0u8; 20];

    // map from index in 8-byte table -> oid
    let mut offsets_to_patch = HashMap::new();

    // A table of sorted object names. These are packed together without offset
    // values to reduce the cache footprint of the binary search for a specific
    // object name.
    for _ in 0..oid_entry_count {
        idx_reader.read_exact(&mut oid).ok()?;
        let oid: ObjectId = oid.into();

        oids.push(oid);
    }

    // A table of 4-byte CRC32 values of the packed object data. This is new in
    // v2 so compressed data can be copied directly from pack to pack during
    // repacking without undetected data corruption.
    // TODO: implement validating these or something?
    idx_reader.seek_relative((oid_entry_count * 4) as i64).ok()?;

    // A table of 4-byte offset values (in network byte order). These are usually
    // 31-bit pack file offsets, but large offsets are encoded as an index into
    // the next table with the msbit set.
    for table_index in 0..oid_entry_count {
        let offset = idx_reader.read_i32::<BigEndian>().ok()?;

        let table_index = table_index as usize;

        // Equivalent to checking the msb
        if offset.is_negative() {
            // The index into the 8-byte offset table (mask off the msb)
            let offset = (offset & !(1 << 31)) as u32;

            // Defer until we parse the 8-byte table
            offsets_to_patch.insert(offset, oids[table_index]);
        } else {
            locations.insert(oids[table_index], offset as usize);
        }
    }

    // A table of 8-byte offset entries (empty for pack files less than 2 GiB).
    // Pack files are organized with heavily used objects toward the front, so
    // most object references should not need to refer to this table.
    if !offsets_to_patch.is_empty() {
        let mut patchable_indices: Vec<&u32> = offsets_to_patch.keys().collect();
        let mut curr_table_idx = 0; 

        // Visit the low indices first
        patchable_indices.sort();

        for idx_to_patch in patchable_indices {
            // Consume entries until we hit our one
            while *idx_to_patch != curr_table_idx {
                idx_reader.read_u64::<BigEndian>().ok()?;
                curr_table_idx += 1;
            }

            let oid = offsets_to_patch.get(idx_to_patch)?;
            let offset = idx_reader.read_u64::<BigEndian>().ok()?;

            locations.insert(*oid, offset as usize);
        }
    }

    Some(GitPackIdx {
        locations
    })
}
