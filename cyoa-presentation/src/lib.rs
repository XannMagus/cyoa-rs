//! Driving terminal interface. Calls application use cases, never concrete adapters.

use clap::{CommandFactory, Parser};

pub mod runtime;
pub mod session;
pub mod worker;

/// A standalone choose-your-own-adventure game, ported from calibre.
#[derive(Debug, Parser)]
#[command(
    name = "cyoa",
    version,
    after_help = "Workspace scaffold only; gameplay is not implemented yet."
)]
struct Cli {}

pub fn run() -> std::io::Result<()> {
    Cli::parse();
    Cli::command().print_help()?;
    println!();
    Ok(())
}
