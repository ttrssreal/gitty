use std::fs::File;
use std::io::Read;
use crate::store::{
    object::{
        parse_blob,
        parse_commit,
        parse_tree,
        parse_tag,
    },
    GitObject,
    ObjectId
};

pub fn get_loose_object(id: ObjectId) -> Option<GitObject> {
    let id_str = id.to_string();

    let obj_path = format!(".git/objects/{}/{}", &id_str[..2], &id_str[2..]);
    let obj_stream = File::open(obj_path).ok()?;

    // Raw object
    let mut data = Vec::new();

    // Decompress
    compress::zlib::Decoder::new(obj_stream)
        .read_to_end(&mut data).ok()?;

    // Git object TLV encoding:
    //  <obj-type> ' ' <byte-size> '\0' <object-data>
    let [header, data] = data.splitn(2, |&b| b == b'\0')
        .by_ref().collect::<Vec<&[u8]>>()[..] else {
            return None;
        };

    let [kind, size] = header.splitn(2, |&b| b == b' ')
        .by_ref().collect::<Vec<&[u8]>>()[..] else {
            return None;
        };

    let size = String::from_utf8_lossy(size).parse::<usize>().ok()?;

    let data = match kind {
        b"blob" => parse_blob(data)?,
        b"commit" => parse_commit(data)?,
        b"tree" => parse_tree(data)?,
        b"tag" => parse_tag(data)?,
        _ => return None
    };

    Some(GitObject {
        id,
        size,
        data,
    })
}
