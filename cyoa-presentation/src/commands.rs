//! CLI intent only; the composition root selects concrete adapters.
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "cyoa",
    version,
    about = "A choose-your-own-adventure game (memory-only headless play)."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}
impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}
#[derive(Debug, Subcommand)]
pub enum Command {
    Play(PlayOptions),
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BackendChoice {
    Claude,
    Codex,
}
#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("source").required(true).args(["backend", "demo"])), after_help = "During play: empty line continues; /action N, /event, /retry, /cancel, /inspect, /help, /quit. Prefix // to send an initial slash. Ctrl-C cancels generation; idle Ctrl-C or EOF exits. No saves or autosave.")]
pub struct PlayOptions {
    /// Only the headless interface is currently implemented.
    #[arg(long, required = true)]
    pub headless: bool,
    #[arg(long, value_enum)]
    pub backend: Option<BackendChoice>,
    /// Replay the fixed Phase 0 harbour story, without credentials or network.
    #[arg(long)]
    pub demo: bool,
    /// Override the selected vendor executable (resolved before request isolation).
    #[arg(long, requires = "backend", conflicts_with = "demo")]
    pub executable: Option<OsString>,
    /// Explicit authentication HOME; otherwise use the process HOME.
    #[arg(long, requires = "backend", conflicts_with = "demo")]
    pub home: Option<PathBuf>,
    /// Vendor auth/config directory; otherwise use CODEX_HOME/CLAUDE_CONFIG_DIR.
    #[arg(long, requires = "backend", conflicts_with = "demo")]
    pub config_dir: Option<PathBuf>,
    /// Optional vendor model selection; configured does not mean observed.
    #[arg(long, requires = "backend", conflicts_with = "demo")]
    pub model: Option<String>,
}
