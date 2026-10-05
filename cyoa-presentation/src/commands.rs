//! CLI intent only; the composition root selects concrete adapters.
use clap::{Args, Parser, Subcommand, ValueEnum};
use cyoa_application::persistence::{
    DemoScenarioId, InspectSave, ListSaves, LoadGame, PageSize, SaveCopy, SaveId, SavePage,
    StorySource,
};
use cyoa_core::{
    game::TurnCount,
    limits::{Limits, RestoreLimits},
};
use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "cyoa",
    version,
    about = "A choose-your-own-adventure game with headless play and saves."
)]
pub struct Cli {
    /// App data directory; saves are stored in its saves subdirectory.
    #[arg(long, global = true, value_parser = absolute_path)]
    pub data_dir: Option<PathBuf>,
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
    List(ListOptions),
    Inspect(InspectOptions),
}
fn absolute_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("--data-dir must be an absolute path".into())
    }
}
fn page_size(value: &str) -> Result<PageSize, String> {
    value
        .parse::<u8>()
        .map_err(|e| e.to_string())
        .and_then(|size| PageSize::new(size).map_err(|e| e.to_string()))
}
fn save_id(value: &str) -> Result<SaveId, String> {
    SaveId::new(value).map_err(|e| e.to_string())
}
#[derive(Debug, Args)]
pub struct ListOptions {
    #[arg(long, value_parser = save_id)]
    pub after: Option<SaveId>,
    #[arg(long, default_value = "100", value_parser = page_size)]
    pub limit: PageSize,
}
#[derive(Debug, Args)]
pub struct InspectOptions {
    #[arg(value_parser = save_id)]
    pub id: SaveId,
    #[arg(long)]
    pub backup: bool,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LimitsChoice {
    Current,
    Original,
}
impl LimitsChoice {
    pub fn policy(self) -> RestoreLimits {
        match self {
            Self::Current => RestoreLimits::Current(Limits::default()),
            Self::Original => RestoreLimits::Original,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BackendChoice {
    Claude,
    Codex,
}
#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("source").required(true).args(["backend", "demo"])), after_help = "During play: empty line continues; /action N, /event, /retry, /cancel, /inspect, /save, /save-copy, /list, /load ID --limits current|original [--backup], /rewind N, /help, /quit. Prefix // to send an initial slash. Selection, accepted turns and rewinds autosave; quit saves the last canonical state. Ctrl-C cancels generation; idle Ctrl-C or EOF exits.")]
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
    #[arg(long, value_parser = save_id, requires = "limits")]
    pub load: Option<SaveId>,
    #[arg(long, value_enum, requires = "load")]
    pub limits: Option<LimitsChoice>,
    #[arg(long, requires = "load")]
    pub backup: bool,
}

/// A combination Clap's declared constraints should have rejected. Returned
/// instead of panicking, so a constraint drift is a usage error, not a crash.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UsageError(&'static str);

/// Where play takes its story from: the credential-free demo or one backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaySource {
    Demo,
    Backend(BackendChoice),
}
impl PlaySource {
    pub fn story_source(self) -> StorySource {
        match self {
            Self::Demo => StorySource::Demo {
                scenario: DemoScenarioId::HarbourV1,
            },
            Self::Backend(_) => StorySource::Live,
        }
    }
}
fn copy(backup: bool) -> SaveCopy {
    if backup {
        SaveCopy::Backup
    } else {
        SaveCopy::Primary
    }
}
impl ListOptions {
    pub fn query(&self) -> ListSaves {
        ListSaves {
            page: SavePage::new(self.after.clone(), self.limit),
        }
    }
}
impl InspectOptions {
    pub fn query(self) -> InspectSave {
        InspectSave {
            id: self.id,
            copy: copy(self.backup),
        }
    }
}
impl PlayOptions {
    pub fn source(&self) -> Result<PlaySource, UsageError> {
        match (self.demo, self.backend) {
            (true, None) => Ok(PlaySource::Demo),
            (false, Some(backend)) => Ok(PlaySource::Backend(backend)),
            _ => Err(UsageError("choose exactly one of --backend and --demo")),
        }
    }
    /// The explicit startup load, if one was requested.
    pub fn load(&self) -> Result<Option<LoadGame>, UsageError> {
        match (&self.load, self.limits) {
            (None, None) => Ok(None),
            (Some(id), Some(limits)) => Ok(Some(LoadGame {
                id: id.clone(),
                copy: copy(self.backup),
                limits: limits.policy(),
            })),
            _ => Err(UsageError("--load and --limits must be given together")),
        }
    }
}

pub enum PersistenceCommand {
    Save { copy: bool },
    List(SavePage),
    Load(LoadGame),
    Rewind(TurnCount),
}
#[derive(Parser)]
#[command(name = "/load", disable_help_flag = true, disable_version_flag = true)]
struct LoadLine {
    #[arg(value_parser = save_id)]
    id: SaveId,
    #[arg(long, value_enum, required = true)]
    limits: LimitsChoice,
    #[arg(long)]
    backup: bool,
}
/// Parse reserved persistence commands before any effect or lifecycle edit.
pub fn persistence_command(line: &str) -> Option<Result<PersistenceCommand, String>> {
    let words: Vec<_> = line.split_whitespace().collect();
    let command = *words.first()?;
    let result = match command {
        "/save" | "/save-copy" => {
            if words.len() == 1 {
                Ok(PersistenceCommand::Save {
                    copy: command == "/save-copy",
                })
            } else {
                Err("/save and /save-copy take no arguments".into())
            }
        }
        "/list" => match words.as_slice() {
            [_] => Ok(PersistenceCommand::List(SavePage::new(None, PageSize::MAX))),
            [_, id] => save_id(id)
                .map(|id| PersistenceCommand::List(SavePage::new(Some(id), PageSize::MAX))),
            _ => Err("Usage: /list [AFTER_ID]".into()),
        },
        "/rewind" => match words.as_slice() {
            [_, count] => count
                .parse::<usize>()
                .ok()
                .and_then(|v| TurnCount::new(v).ok())
                .map(PersistenceCommand::Rewind)
                .ok_or_else(|| "/rewind requires a positive turn count".into()),
            _ => Err("Usage: /rewind N".into()),
        },
        "/load" => LoadLine::try_parse_from(&words)
            .map(|v| {
                PersistenceCommand::Load(LoadGame {
                    id: v.id,
                    copy: copy(v.backup),
                    limits: v.limits.policy(),
                })
            })
            .map_err(|_| "Usage: /load SAVE_ID --limits current|original [--backup]".into()),
        _ => return None,
    };
    Some(result)
}
