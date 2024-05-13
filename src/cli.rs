use clap::{Parser, Subcommand, Args};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    CatFile(CatFileArgs)
}

#[derive(Args)]
pub struct CatFileArgs {
    #[command(flatten)]
    pub mode: CatFileMode,

    pub id: String,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct CatFileMode {
    #[arg(short)]
    pub print: bool,

    #[arg(short = 't')]
    pub kind: bool,
}
