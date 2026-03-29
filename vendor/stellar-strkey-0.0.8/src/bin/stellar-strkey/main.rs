mod decode;
mod version;

use clap::{CommandFactory, Parser, Subcommand};
use std::{error::Error, fmt::Debug};

#[derive(Parser, Debug, Clone)]
#[command(
    author,
    version,
    about,
    long_about = None,
    disable_help_subcommand = true,
    disable_version_flag = true,
    disable_colored_help = true,
    infer_subcommands = true,
)]
struct Root {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug, Clone)]
enum Cmd {
    /// Decode strkey
    Decode(decode::Cmd),
    /// Print version information
    Version,
}

fn run() -> Result<(), Box<dyn Error>> {
    let root = Root::parse();
    match root.cmd {
        Cmd::Decode(c) => c.run()?,
        Cmd::Version => version::Cmd::run(),
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        Root::command()
            .error(clap::error::ErrorKind::ValueValidation, e)
            .exit()
    }
}
