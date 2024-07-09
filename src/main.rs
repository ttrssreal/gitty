mod store;
mod cli;

use cli::{Cli, Commands};
use clap::Parser;
use std::io::Write;

use store::GitObjectStore;
use crate::cli::CatFileArgs;
use crate::store::util::resolve_id;

pub const MIN_USER_HASH_LEN: usize = 4;
pub const SHA1_HASH_SIZE: usize = 20;

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let cli = Cli::parse();

    match cli.command {
        Commands::CatFile(CatFileArgs {
            mode,
            id
        }) => {
            let id = resolve_id(&id).ok_or("Invalid Object Id")?;

            let obj = match GitObjectStore::get(id) {
                Some(obj) => obj,
                None => return Err("Unable to retrive object".into())
            };

            let mut stdout = std::io::stdout();

            if mode.print {
                print!("{}", obj);
            }

            if mode.kind {
                println!("{}", obj.type_str());
            }

            stdout.flush()?;
        }
    };

    Ok(())
}
