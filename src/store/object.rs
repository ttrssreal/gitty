use std::iter::Peekable;
use std::collections::HashMap;
use crate::store::{
    GitObjectData, 
    GitObjectStore,
    TreeEntry,
    ObjectId
};

use crate::SHA1_HASH_SIZE;

/// Commit object format (general structure):
///   "tree " <tree-sha> \n
///   "parent " <parent-sha> \n (can have multiple parent headers)
///   "author " <user-info> \n
///   "committer " <user-info> \n
///   "gpgsig " <gpg-signature> \n (optional)
///   "encoding " <encoding> \n (optional)
///   \n
///   <commit-message>
///
/// Multiline header semantics are as follows, if a space precedes
/// a newline the space is treated as a continuation character
/// and discarded. the newline is considered part of the header.
///
/// eg.
/// "gpgsig -----BEGIN PGP SIGNATURE-----" \n
/// " iQGzBAABCAAdFiEEgJI70ezQ5DZnHcjpujNQaGyRhgYFAmWAUvoACgkQujNQaGyR" \n
/// " hgap0wv9Gn8gE8BPagd8txOwQuRtfOWXc1V1ovOubmt0Th2UPJDxpNDp/G+AN8kH" \n
///  ...
/// " -----END PGP SIGNATURE-----" \n
pub fn parse_commit(data: &[u8]) -> Option<GitObjectData> {
    let mut data = data.iter().peekable();

    let headers = parse_headers(&mut data)?;

    if !headers.contains_key("tree") || headers.get("tree")?.is_empty() {
        eprintln!("parse_commit(): tree or parent headers");
        return None;
    }

    // Decode the tree hash
    let tree_headers = headers.get("tree")?;
    let tree = hex::decode(tree_headers.first()?).ok()?;
    let tree: ObjectId = tree.as_slice().try_into().ok()?;

    // Decode all the parent hashes
    let mut parents = Vec::new();
    if let Some(pv) = headers.get("parent") {
        for p in pv {
            let decoded = hex::decode(p).ok()?;
            parents.push(decoded.as_slice().try_into().ok()?)
        }
    }
    
    let author = headers.get("author")?
        .first()?.to_string();

    let committer = headers.get("committer")?
        .first()?.to_string();

    let encoding = headers.get("encoding")
        .map(|e| e.first().map(String::to_string)).flatten();

    let gpgsig = headers.get("gpgsig")
        .map(|e| e.first().map(String::to_string)).flatten();

    // Eat final newline before message body
    if *data.next()? != b'\n' {
        eprintln!("parse_commit(): can't find commit message");
    }

    let message = data.map(|&b| b).collect();

    Some(GitObjectData::Commit {
        tree,
        parents,
        author,
        committer,
        encoding,
        gpgsig,
        message,
    })
}

/// Tree object format:
///   <tree-entry>
///
/// <tree-entry>:
///   <mode> ' ' <path> '\0' <sha>
///
/// mode is encoded as string of base-8 (octal) characters
pub fn parse_tree(data: &[u8]) -> Option<GitObjectData> {
    let mut data = data.iter().peekable();

    let mut entries = Vec::new();

    while !data.peek().is_none() {
        let entry = parse_tree_entry(&mut data)?;
        entries.push(entry);
    }

    Some(GitObjectData::Tree {
        entries,
    })
}

pub fn parse_tag(data: &[u8]) -> Option<GitObjectData> {
    let mut data = data.iter().peekable();

    let headers = parse_headers(&mut data)?;

    let object = headers.get("object")?
        .first()?.to_string();

    let kind = headers.get("type")?
        .first()?.to_string();

    let tag = headers.get("tag")?
        .first()?.to_string();

    let tagger = headers.get("tagger")?
        .first()?.to_string();

    let object: ObjectId = hex::decode(object)
        .ok()?.as_slice().try_into().ok()?;

    // Eat final newline before message body
    if *data.next()? != b'\n' {
        eprintln!("parse_commit(): can't find commit message");
    }

    let message = data.map(|&b| b).collect();

    Some(GitObjectData::Tag {
        object,
        kind,
        tag,
        tagger,
        message,
    })
}

fn parse_header<'a, I>(data: &mut Peekable<I>) -> Option<(String, String)>
where
    I: Iterator<Item = &'a u8>
{
    let header_key: Vec<u8> = data.take_while(|&b| *b != b' ')
        .map(|&b| b).collect();

    let mut header_value = Vec::new();

    loop {
        let line: Vec<u8> = data.take_while(|&b| *b != b'\n')
            .map(|&b| b).collect();

        header_value.extend(line);

        match *data.peek()? {
            // Do we encounter a continuation character?
            b' ' => {
                // Consume it, push a newline and continue parsing the header
                data.next();
                header_value.push(b'\n');
            },
            _ => break,
        }
    }

    let header_key = String::from_utf8_lossy(&header_key).to_string();
    let header_value = String::from_utf8_lossy(&header_value).to_string();

    Some((header_key, header_value))
}

pub fn parse_headers<'a, I>(data: &mut Peekable<I>) -> Option<HashMap<String, Vec<String>>>
where
    I: Iterator<Item = &'a u8>
{
    let mut headers: HashMap<String, Vec<String>> = HashMap::new();

    while **data.peek()? != b'\n' {
        let header = parse_header(data)?;
        let key = header.0;
        let values = header.1;
        headers.entry(key).or_default().push(values);
    }

    Some(headers)
}

fn parse_tree_entry<'a, I>(data: &mut Peekable<I>) -> Option<TreeEntry>
where
    I: Iterator<Item = &'a u8>
{
    let mode: Vec<u8> = data.take_while(|&&b| b != b' ')
        .map(|&b| b).collect();

    let path: Vec<u8> = data.take_while(|&&b| b != b'\0')
        .map(|&b| b).collect();

    let id: Vec<u8> = data.take(SHA1_HASH_SIZE)
        .map(|&b| b).collect();

    let mode = std::str::from_utf8(&mode[..]).ok()?;
    let path = std::str::from_utf8(&path[..]).ok()?;

    let path = path.to_string();
    let mode = u32::from_str_radix(mode, 8).ok()?;

    let id: ObjectId = id.as_slice().try_into().ok()?;
    let kind = GitObjectStore::get(id)?.type_str().to_string();

    Some(TreeEntry {
        mode,
        kind,
        path,
        id
    })
}

/// Blob object format:
/// <data>
pub fn parse_blob(data: &[u8]) -> Option<GitObjectData> {
    Some(GitObjectData::Blob {
        data: data.to_vec(),
    })
}
