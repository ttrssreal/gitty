use std::fmt::Display;
use std::fs::File;
use compress;
use std::io::Read;
use std::iter::{Peekable, Iterator};
use std::collections::HashMap;
use std::option::Option;

pub const SHA1_HASH_SIZE: usize = 20;

// "Each entry has a sha1 identifier, pathname and mode."
#[derive(Debug, PartialEq)]
pub struct TreeEntry {
    pub mode: u32,
    pub kind: String,
    pub path: String,
    pub id: [u8; SHA1_HASH_SIZE],
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
        tree: [u8; SHA1_HASH_SIZE],
        parents: Vec<[u8; SHA1_HASH_SIZE]>,
        // https://docs.github.com/en/pull-requests/committing-changes-to-your-project/creating-and-editing-commits/creating-a-commit-with-multiple-authors
        // assuming git doesn't support multiple authors/committers
        author: String,
        committer: String,
        encoding: Option<String>,
        gpgsig: Option<String>,
        message: Vec<u8>,
    },
    Tag {
        object: [u8; SHA1_HASH_SIZE],
        kind: String,
        tag: String,
        tagger: String,
        // If signed the signature resides in the message itself
        message: Vec<u8>,
    },
    
}

#[derive(Debug)]
pub struct GitObject {
    pub id: [u8; SHA1_HASH_SIZE],
    pub size: usize,
    pub data: GitObjectData,
}

impl GitObject {
    pub fn type_string(&self) -> String {
        match self.data {
            GitObjectData::Blob {..} => "blob",
            GitObjectData::Tree {..} => "tree",
            GitObjectData::Commit {..} => "commit",
            GitObjectData::Tag {..} => "tag",
        }.to_string()
    }
}

impl Display for TreeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:06o} {} {} {}", self.mode, self.kind, hex::encode(self.id), self.path)
    }
}

impl Display for GitObject {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.data {
            GitObjectData::Blob { data } => {
                write!(f, "{}", String::from_utf8_lossy(&data))
            },
            GitObjectData::Tree { entries } => {
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
                writeln!(f, "tree {}", hex::encode(tree))?;

                for parent in parents {
                    writeln!(f, "parent {}", hex::encode(parent))?;
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
                write!(f, "object {}\n", hex::encode(object))?;
                write!(f, "type {}\n", kind)?;
                write!(f, "tag {}\n", tag)?;
                write!(f, "tagger {}\n", tagger)?;
                write!(f, "\n")?;
                write!(f, "{}", String::from_utf8_lossy(message))
            },
        }
    }
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

fn parse_headers<'a, I>(data: &mut Peekable<I>) -> Option<HashMap<String, Vec<String>>>
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
    let id: [u8; SHA1_HASH_SIZE] = id.try_into().ok()?;
    let kind = GitObjectStore::get(id)?.type_string();

    Some(TreeEntry {
        mode,
        kind,
        path,
        id
    })
}

/// Blob object format:
/// <data>
fn parse_blob(data: &[u8]) -> Option<GitObjectData> {
    Some(GitObjectData::Blob {
        data: data.to_vec(),
    })
}

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
fn parse_commit(data: &[u8]) -> Option<GitObjectData> {
    let mut data = data.iter().peekable();

    let headers = parse_headers(&mut data)?;

    if !headers.contains_key("tree") || headers.get("tree")?.is_empty() {
        eprintln!("parse_commit(): tree or parent headers");
        return None;
    }

    // Decode the tree hash
    let tree_headers = headers.get("tree")?;
    let tree = hex::decode(tree_headers.first()?).ok()?;
    let tree: [u8; SHA1_HASH_SIZE] = tree.try_into().ok()?;

    // Decode all the parent hashes
    let mut parents = Vec::new();
    if let Some(pv) = headers.get("parent") {
        for p in pv {
            let decoded = hex::decode(p).ok()?;
            parents.push(decoded.try_into().ok()?)
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
fn parse_tree(data: &[u8]) -> Option<GitObjectData> {
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

fn parse_tag(data: &[u8]) -> Option<GitObjectData> {
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

    let object: [u8; SHA1_HASH_SIZE] = hex::decode(object)
        .ok()?.try_into().ok()?;

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

pub struct GitObjectStore;

impl GitObjectStore {
    pub fn get(id: [u8; SHA1_HASH_SIZE]) -> Option<GitObject> {
        let id_str = hex::encode(id);

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
}
