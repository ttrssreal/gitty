mod object;
mod cli;

use std::fs::{self, DirEntry};
use cli::{Cli, Commands};
use clap::Parser;
use std::io::Write;
use hex;

use object::GitObjectStore;

pub const MIN_USER_HASH_LEN: usize = 4;

fn hash_from_str(id_str: &str) -> Option<[u8; 20]> {

    let id_len = id_str.len();

    if id_len < MIN_USER_HASH_LEN || id_len > 40 {
        eprintln!("Invalid hash length.");
        return None;
    }

    let obj_dir = format!(".git/objects/{}/", &id_str[..2]);

    let Ok(contents) = fs::read_dir(obj_dir) else {
        eprintln!("Invalid hash.");
        return None;
    };

    let matches: Vec<DirEntry> = contents
        .map(|o| o.expect("hash_from_str(): ReadDir"))
        .filter(|o| o
                .file_name()
                .into_string()
                .expect("hash_from_str(): ReadDir")
                .starts_with(&id_str[2..]))
        .collect();

    let matches_len = matches.len();

    if matches_len == 0 {
        eprintln!("Can't find hash.");
        return None;
    } else if matches_len > 1 {
        eprintln!("Can't disambiguate hash.");
        return None;
    }

    let found = &matches[0]
        .file_name()
        .into_string()
        .ok()?;

    let id = format!("{}{}", &id_str[..2], found);

    let id = hex::decode(id).ok()?;

    let id: [u8; 20] = id.try_into().ok()?;

    Some(id)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let cli = Cli::parse();

    match cli.command {
        Commands::CatFile(cat_file_args) => {
            let id = hash_from_str(&cat_file_args.id).ok_or("Failed.")?;

            let obj = match GitObjectStore::get(id) {
                Some(obj) => obj,
                None => return Err("Can't retrive object".into())
            };

            let mut stdout = std::io::stdout();

            if cat_file_args.mode.print {
                print!("{}", obj);
            }

            if cat_file_args.mode.kind {
                print!("{}\n", obj.type_string());
            }

            stdout.flush()?;
        }
    };

    Ok(())
}
