use std::fs::File;
use compress;
use std::io::Read;
use std::io::Write;

#[derive(Debug, PartialEq)]
pub enum GitObjectKind {
    Blob,
    Tree,
    Commit,
    Tag,
}

impl GitObjectKind {
    pub fn to_string(&self) -> String {
        match self {
            GitObjectKind::Blob => String::from("blob"),
            GitObjectKind::Tree => String::from("tree"),
            GitObjectKind::Commit => String::from("commit"),
            GitObjectKind::Tag => String::from("tag"),
        }
    }
}

#[derive(Debug)]
pub struct GitObject {
    pub id: [u8; 20],
    pub kind: GitObjectKind,
    pub size: usize,
    pub content: Vec<u8>,
}

impl GitObject {
    pub fn dump_content<W: Write>(&self, mut w: W) -> std::io::Result<()> {
        w.write_all(&self.content)?;
        Ok(())
    }

    pub fn dump_type<W: Write>(&self, mut w: W) -> std::io::Result<()> {
        w.write_all(&self.kind.to_string().as_bytes())?;
        w.write(b"\n")?;
        Ok(())
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
            .splitn(2, |byte| *byte == '\0' as u8);

        let mut header_split = header_content.next()?
            .split(|byte| *byte == ' ' as u8);
        
        let kind = match header_split.next()? {
            b"blob" => GitObjectKind::Blob,
            b"commit" => GitObjectKind::Commit,
            b"tree" => GitObjectKind::Tree,
            b"tag" => GitObjectKind::Tag,
            _ => return None
        };

        let size = String::from_utf8_lossy(header_split.next()?)
            .parse::<usize>().ok()?;
        
        match kind {
            GitObjectKind::Blob => {
                let obj_content = dbg!(header_content.next()?);

                return Some(GitObject {
                    id,
                    kind,
                    size,
                    content: obj_content.to_vec(),
                });
            },
            GitObjectKind::Commit => {},
            GitObjectKind::Tree => {},
            GitObjectKind::Tag => {},
        }

        None
    }
}