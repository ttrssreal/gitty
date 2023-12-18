use std::fmt::Display;
use std::fs::File;
use compress;
use std::io::Read;

#[derive(Debug, PartialEq)]
pub struct TreeEntry {
    pub mode: u32,
    pub kind: String,
    pub name: String,
    pub id: [u8; 20],
}

#[derive(Debug, PartialEq)]
pub struct ExtraHeader {
    pub name: String,
    pub value: String,
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
        tree: [u8; 20],
        parents: Vec<[u8; 20]>,
        author: String,
        committer: String,
        encoding: Option<String>,
        extra_headers: Vec<ExtraHeader>,
        message: Vec<u8>,
    },
    Tag {
        object: [u8; 20],
        kind: String,
        tag: String,
        tagger: String,
        message: String,
    },
    
}

#[derive(Debug)]
pub struct GitObject {
    pub id: [u8; 20],
    pub size: usize,
    pub content: GitObjectData,
}

impl GitObject {
    pub fn to_string(&self) -> String {
        match self.content {
            GitObjectData::Blob {..} => "blob",
            GitObjectData::Tree {..} => "tree",
            GitObjectData::Commit {..} => "commit",
            GitObjectData::Tag {..} => "tag",
        }
        .to_string()
    }
}

impl Display for TreeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:06o} {} {} {}", self.mode, self.kind, hex::encode(self.id), self.name)
    }
}

impl Display for GitObject {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.content {
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
                extra_headers,
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
                
                for extra_header in extra_headers {
                    writeln!(f, "{}: {}", extra_header.name, extra_header.value)?;
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
                write!(f, "{}", message)
            },
        }
    }
}

pub struct GitObjectStore;

impl GitObjectStore {
    pub fn get(id: [u8; 20]) -> Option<GitObject> {
        let id_str = hex::encode(id);

        let obj_path = format!(".git/objects/{}/{}", &id_str[..2], &id_str[2..]);
        let obj_stream = File::open(obj_path).ok()?;

        let mut contents = Vec::new();

        compress::zlib::Decoder::new(obj_stream)
            .read_to_end(&mut contents).ok()?;

        let mut header_content = contents
            .splitn(2, |&byte| byte == '\0' as u8);

        let mut kind_size = header_content.next()?
            .split(|&byte| byte == ' ' as u8);

        let kind = kind_size.next()?;
        
        let size = String::from_utf8_lossy(kind_size.next()?)
            .parse::<usize>().ok()?;
        
        let content = match kind {
            b"blob" => GitObjectData::Blob {
                data: header_content.next()?.to_vec(),
            },
            b"commit" => {
                let buffer = header_content.next()?;
                let mut lines = buffer
                    .split(|&byte| byte == '\n' as u8);                
                
                let tree = lines.next()?;

                if !tree.starts_with(b"tree ") {
                    println!("Invalid commit object. Expected tree");
                    return None;
                }

                let tree = &tree[5..];
                let tree = hex::decode(tree).ok()?;
                let tree: [u8; 20] = tree.try_into().ok()?;

                let mut author_line = Vec::new();
                let mut parents = Vec::new();
                
                while let Some(parent) = lines.next() {
                    if !parent.starts_with(b"parent ") {
                        author_line = parent.to_vec();
                        break;
                    }

                    let parent = &parent[7..];
                    let parent = hex::decode(parent).ok()?;

                    parents.push(parent.try_into().ok()?);
                }

                if !author_line.starts_with(b"author ") {
                    println!("Invalid commit object. Expected author");
                    return None;
                }
                let author = &author_line[7..];
                let author = String::from_utf8_lossy(author).to_string();

                let committer = lines.next()?;
                if !committer.starts_with(b"committer ") {
                    println!("Invalid commit object. Expected committer");
                    return None;
                }
                let committer = &committer[10..];
                let committer = String::from_utf8_lossy(committer).to_string();

                let mut encoding = None;
                let mut next_line = lines.clone().peekable();
                let next_line = next_line.peek()?;

                if next_line.starts_with(b"encoding ") {
                    let enc_line = &&next_line[10..];
                    encoding = Some(String::from_utf8_lossy(enc_line).to_string());
                }

                let mut extra_headers = Vec::new();
                let mut current_line = Vec::new();

                while let Some(line) = lines.next() {
                    /* continuation */
                    if line.starts_with(b" ") {
                        current_line.extend_from_slice(&line[1..]);
                        current_line.push(b'\n');
                        continue;
                    }
                    
                    if line.starts_with(b"gpgsig ") || line.len() == 0 {
                        if current_line.len() != 0 {
                            extra_headers.push(ExtraHeader {
                                name: String::from("gpgsig"),
                                value: String::from_utf8_lossy(&current_line[..current_line.len()]).to_string(),
                            });
                        }

                        if line.len() == 0 {
                            break;
                        }
                        
                        current_line.clear();
                        current_line.extend_from_slice(&line[7..]);
                        current_line.push(b'\n');
                        continue;
                    }
                }

                let mut message = Vec::new();

                let msg_start = buffer
                    .windows(2)
                    .position(|delim| delim.starts_with(b"\n\n"))? + 2;

                message.extend_from_slice(&buffer[msg_start..]);

                GitObjectData::Commit {
                    tree,
                    parents,
                    author,
                    committer,
                    encoding,
                    extra_headers,
                    message,
                }
            },
            b"tree" => GitObjectData::Tree {
                entries: Vec::new(),
            },
            b"tag" => GitObjectData::Tag {
                object: [0xa; 20],
                kind: String::from("unimplemented"),
                tag: String::from("unimplemented"),
                tagger: String::from("unimplemented"),
                message: String::from("unimplemented"),
            },
            _ => return None
        };

        Some(GitObject {
            id,
            size,
            content,
        })
    }
}