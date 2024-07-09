use std::io::{
    Read,
    BufReader,
    Cursor,
    SeekFrom,
    Seek
};
use std::fs::File;
use crate::store::pack::{
    read_kind_length_obj_header,
    PackedObjectKind::{ self, Delta },
    DeltaKind,
};
use byteorder::ReadBytesExt;

// Deltified object:
// size-encoded n-byte integer:
//  - type: 3-bit,
//  - uncompressed data length: all-other-bits
// base-reference, either:
// OBJ_OFS_DELTA:
//  - negative-relative-offset
//  - compressed data:
//   - size-encoded byte-length of base object
//   - size-encoded byte-length of the resultant object
// OBJ_REF_DELTA:
//  - hash-sized oid
//  - compressed data:
//   - size-encoded byte-length of base object
//   - size-encoded byte-length of the resultant object

#[derive(Debug)]
struct DeltaStackItem {
    base_size: u64,
    result_size: u64,
    instructions: Box<[u8]>,
}

fn read_negative_relative_offset<R>(data: &mut R) -> Option<u64>
where
    R: Read,
{
    let mut byte = data.read_i8().ok()?;
    let mut neg_relative_offs;

    // 4-bit least significant part of length
    neg_relative_offs = (byte & 0x7f) as u64;

    // check MSB
    while byte.is_negative() {
        neg_relative_offs += 1;
        neg_relative_offs <<= 7;
        byte = data.read_i8().ok()?;
        neg_relative_offs |= (byte & 0x7f) as u64;
    }

    Some(neg_relative_offs)
}

pub fn resolve_delta(delta_object: &mut BufReader<File>) -> Option<(PackedObjectKind, Vec<u8>)> {
    let mut delta_stack = Vec::new();
    let mut kind;
    let mut length;

    // start of current object
    let mut start_offset;

    // follow the delta chain, pushing deltified objects until we reach the first
    // concrete object. (ie. blob, commit, tree, tag)
    loop {
        start_offset = delta_object.stream_position().ok()?;

        (kind, length) = read_kind_length_obj_header(delta_object)?;

        use DeltaKind::*;
        match kind {
            Delta(delta_kind) => match delta_kind {
                Offset => {
                    let mut delta_data = vec![0u8; length as usize];
                    let mut instructions = Vec::new();

                    // parse the base objects negative offset from us
                    let negative_offset = read_negative_relative_offset(delta_object)?;

                    // decompress the delta
                    compress::zlib::Decoder::new(delta_object.by_ref())
                        .read_exact(&mut delta_data).ok()?;

                    let mut delta_reader = Cursor::new(delta_data);

                    let base_size = size_decode(&mut delta_reader)?;
                    let result_size = size_decode(&mut delta_reader)?;

                    // the rest of the data are the encoded instructions
                    delta_reader.read_to_end(&mut instructions).ok()?;

                    delta_stack.push(DeltaStackItem {
                        base_size,
                        result_size,
                        instructions: instructions.into_boxed_slice()
                    });

                    // jump to the base object
                    let base_offset = start_offset - negative_offset;
                    delta_object.seek(SeekFrom::Start(base_offset)).ok()?;
                }
                Reference => unimplemented!("OBJ_REF_DELTA"),
            },
            // found base object!
            _ => break
        }
    }

    if delta_stack.len() == 0 {
        eprintln!("No delta to resolve.");
        return None;
    }

    let initial_delta = delta_stack.pop()?;

    let mut base_buffer: Vec<u8> = vec![0; initial_delta.base_size as usize];
    let mut dest_buffer: Vec<u8> = vec![0; initial_delta.result_size as usize];

    delta_object.seek(SeekFrom::Start(start_offset)).ok()?;
    let (kind, _) = read_kind_length_obj_header(delta_object)?;

    compress::zlib::Decoder::new(delta_object.by_ref())
        .read_exact(&mut base_buffer).ok()?;

    apply_delta(&base_buffer, &mut dest_buffer, &initial_delta.instructions);

    while let Some(delta_stack_item) = delta_stack.pop() {
        // the previous result becomes the new base
        std::mem::swap(&mut base_buffer, &mut dest_buffer);

        dest_buffer.resize(delta_stack_item.result_size as usize, 0);

        apply_delta(&base_buffer, &mut dest_buffer, &delta_stack_item.instructions);
    }

    Some((kind, dest_buffer))
}

/// Implements this bytecode type thing
pub fn apply_delta(
    base_buffer: &[u8],
    dest_buffer: &mut [u8],
    instructions: &[u8]
) -> Option<()> {
    // instruction pointer
    let mut ip = 0;
    // destination pointer
    let mut dp = 0;

    while ip < instructions.len() {
        if instructions[ip] & 0x80 != 0 {
            //
            // Copy instruction
            // +----------+---------+---------+---------+---------+-------+-------+-------+
            // | 1xxxxxxx | offset1 | offset2 | offset3 | offset4 | size1 | size2 | size3 |
            // +----------+---------+---------+---------+---------+-------+-------+-------+
            //
            let mut data_pointer = ip;
            let mut offset: u64 = 0;
            let mut size: u64 = 0;
            for field in 0u8..7 {
                let bitmask = 1 << field;
                if instructions[ip] & bitmask != 0 {
                    data_pointer += 1;
                    let field_data: u64 = instructions[data_pointer] as u64;
                    match field {
                        0..=3=> { offset |= field_data << (field * 8); }
                        4.. => { size |= field_data << (field - 4) * 8; }
                    };
                }
            }
            
            let offset = offset as usize;
            let size = size as usize;

            dest_buffer[dp..(dp + size)]
                .copy_from_slice(&base_buffer[offset..(offset + size)]);

            dp += size;
            ip = data_pointer + 1;
        } else {
            //
            // Data instruction
            // +----------+============+
            // | 0xxxxxxx |    data    |
            // +----------+============+
            //
            let size = instructions[ip] as usize;

            let data_start = ip + 1;
            dest_buffer[dp..(dp + size)]
                .copy_from_slice(&instructions[data_start..(data_start + size)]);

            ip += 1;
            ip += size;
            dp += size;
        }
    }

    Some(())
}

fn size_decode<R>(reader: &mut R) -> Option<u64>
where
    R: Read,
{
    let mut byte = reader.read_i8().ok()?;
    let mut decoded = (byte & 0x7f) as u64;
    let mut n = 7;

    while byte.is_negative() {
        byte = reader.read_i8().ok()?;
        decoded |= ((byte & 0x7f) as u64) << n;
        n += 7;
    }

    Some(decoded)
}
