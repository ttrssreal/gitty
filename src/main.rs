mod object;
mod cli;

use cli::{Cli, Commands};
use clap::Parser;
use std::io::Write;
use hex;

use object::GitObjectStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let cli = Cli::parse();

    match cli.command {
        Commands::CatFile(cat_file_args) => {
            let id = hex::decode(cat_file_args.id)?;

            let id = match id.try_into() {
                Ok(id) => id,
                Err(_) => return Err("Invalid id".into())
            };

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
