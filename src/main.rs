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
                None => return Err("Object not found".into())
            };

            let mut stdout = std::io::stdout();

            if cat_file_args.mode.print {
                obj.dump_content(stdout.lock())?;
            }

            if cat_file_args.mode.type_ {
                obj.dump_type(stdout.lock())?;
            }

            stdout.flush()?;
        }
    };

    Ok(())
}
