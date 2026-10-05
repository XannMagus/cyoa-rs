use cyoa_application::{
    cancellation::CancellationSource,
    generation::{StoryGenerator, StoryUseCases},
    persistence::*,
};
use cyoa_core::{limits::Limits, style::StoryStyle};
use cyoa_infrastructure::{
    backend::Backend,
    backends::{
        claude_cli::{ClaudeCliBackend, ClaudeExecutable, ClaudeInvocationConfig, ClaudeModel},
        codex_cli::{CodexCliBackend, CodexExecutable, CodexInvocationConfig, CodexModel},
    },
    generation::{demo, engine::GenerationEngine, templates::GenerationTemplates},
    persistence::{
        helper::{HelperConfig, SupervisedRepository},
        repository::data_directory,
    },
};
use cyoa_presentation::{
    commands::{BackendChoice, Cli, Command, PlayOptions},
    persistence::{PersistedSession, validate_loaded_source},
    runtime::SessionRuntime,
    session::SessionController,
    storage::StorageRunner,
    worker::PreviewLimit,
};
use std::{
    ffi::OsStr,
    io::{self, Write},
    path::PathBuf,
};

fn main() {
    if let Err(error) = execute() {
        report_error(error.as_ref());
        std::process::exit(1);
    }
}
fn report_error(error: &dyn std::fmt::Display) {
    #[cfg(target_os = "linux")]
    {
        // play() has already restored descriptor flags. Reacquire nonblocking
        // stderr for this best-effort report; a full pipe must never delay exit.
        if let Ok(mut stderr) = cyoa_presentation::terminal::Flags::new(io::stderr()) {
            let _ = writeln!(stderr.get_mut(), "cyoa: {error}");
            let _ = stderr.get_mut().flush();
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = writeln!(io::stderr(), "cyoa: {error}");
    }
}
fn execute() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().nth(1).as_deref()
        == Some(OsStr::new(
            cyoa_infrastructure::persistence::helper::INTERNAL_HELPER_ARG,
        ))
    {
        if std::env::args_os().len() != 2 {
            return Err("internal storage helper takes no further arguments".into());
        }
        cyoa_infrastructure::persistence::helper::run_internal(
            io::stdin().lock(),
            io::stdout().lock(),
        )?;
        return Ok(());
    }
    let cli = Cli::parse();
    #[cfg(not(target_os = "linux"))]
    {
        let _ = cli;
        Err(cyoa_presentation::terminal::unsupported().into())
    }
    #[cfg(target_os = "linux")]
    {
        let helper = HelperConfig::new(std::env::current_exe()?, data_directory(cli.data_dir)?)?;
        let options = match cli.command {
            Command::List(options) => {
                let page = SavePage::new(options.after, options.limit)?;
                let result = connect(move |token| {
                    PersistenceUseCases::new(SupervisedRepository::new(helper))
                        .list_saves(ListSaves { page }, token)
                })?;
                return query_output(|out| {
                    cyoa_presentation::headless::render_listing(&result, out)
                });
            }
            Command::Inspect(options) => {
                let stored = connect(move |token| {
                    PersistenceUseCases::new(SupervisedRepository::new(helper)).inspect_save(
                        InspectSave {
                            id: options.id,
                            copy: if options.backup {
                                SaveCopy::Backup
                            } else {
                                SaveCopy::Primary
                            },
                        },
                        token,
                    )
                })?;
                return query_output(|out| {
                    cyoa_presentation::headless::render_saved_game(&stored, out)
                });
            }
            Command::Play(options) => options,
        };
        let source = if options.demo {
            StorySource::Demo {
                scenario: DemoScenarioId::HarbourV1,
            }
        } else {
            StorySource::Live
        };
        let loaded = if let Some(id) = options.load.clone() {
            let config = helper.clone();
            let command = LoadGame {
                id,
                copy: if options.backup {
                    SaveCopy::Backup
                } else {
                    SaveCopy::Primary
                },
                limits: options
                    .limits
                    .expect("Clap requires explicit restore policy")
                    .policy(),
            };
            let loaded = connect(move |token| {
                PersistenceUseCases::new(SupervisedRepository::new(config))
                    .load_game(command, token)
            })?;
            validate_loaded_source(&loaded, source)?;
            Some(loaded)
        } else {
            None
        };
        // Auth preflight precedes the UI. No background stdin reader is spawned.
        if options.demo {
            play(demo::harbour_v1()?, source, helper, loaded)
        } else {
            let selected_path = std::env::var_os("PATH").unwrap_or_default();
            let home = options
                .home
                .clone()
                .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
                .ok_or("HOME is unset; provide --home")?;
            let base = std::env::current_dir()?;
            match options.backend.expect("Clap requires exactly one source") {
                BackendChoice::Codex => {
                    let executable = CodexExecutable::resolve(
                        options.executable.as_deref().unwrap_or(OsStr::new("codex")),
                        &selected_path,
                        &base,
                    )?;
                    let config = CodexInvocationConfig::new(
                        executable,
                        home,
                        selected_path,
                        config_dir(&options, "CODEX_HOME"),
                        options.model.map(CodexModel::new).transpose()?,
                    )?;
                    play_backend(
                        connect(move |token| CodexCliBackend::connect(config, token))?,
                        helper,
                        loaded,
                    )
                }
                BackendChoice::Claude => {
                    let executable = ClaudeExecutable::resolve(
                        options
                            .executable
                            .as_deref()
                            .unwrap_or(OsStr::new("claude")),
                        &selected_path,
                        &base,
                    )?;
                    let config = ClaudeInvocationConfig::new(
                        executable,
                        home,
                        selected_path,
                        config_dir(&options, "CLAUDE_CONFIG_DIR"),
                        options.model.map(ClaudeModel::new).transpose()?,
                    )?;
                    play_backend(
                        connect(move |token| ClaudeCliBackend::connect(config, token))?,
                        helper,
                        loaded,
                    )
                }
            }
        }
    }
}
#[cfg(target_os = "linux")]
fn connect<T: Send, E: std::error::Error + Send + 'static>(
    job: impl FnOnce(&cyoa_application::cancellation::CancellationToken) -> Result<T, E> + Send,
) -> Result<T, Box<dyn std::error::Error>> {
    let interrupt = cyoa_presentation::terminal::InterruptFlag::new()?;
    let source = CancellationSource::default();
    let token = source.token();
    // Auth precedes the UI, but SIGINT still cancels and joins its child.
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| job(&token));
        while !worker.is_finished() {
            if interrupt.take() {
                source.cancel();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let backend = worker
            .join()
            .map_err(|_| io::Error::other("authentication worker panicked"))??;
        if interrupt.take() || token.is_cancelled() {
            return Err(io::Error::other("authentication cancelled after cleanup").into());
        }
        Ok(backend)
    })
}
fn config_dir(options: &PlayOptions, variable: &str) -> Option<PathBuf> {
    options
        .config_dir
        .clone()
        .or_else(|| std::env::var_os(variable).map(PathBuf::from))
}
#[cfg(target_os = "linux")]
fn play_backend<B: Backend + Send + 'static>(
    backend: B,
    helper: HelperConfig,
    loaded: Option<LoadedGame>,
) -> Result<(), Box<dyn std::error::Error>> {
    play(
        GenerationEngine::new(backend, GenerationTemplates::bundled()?),
        StorySource::Live,
        helper,
        loaded,
    )
}
#[cfg(target_os = "linux")]
fn play<G: StoryGenerator + Send + 'static>(
    generator: G,
    source: StorySource,
    helper: HelperConfig,
    loaded: Option<LoadedGame>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cases = StoryUseCases::new(generator);
    let runtime = SessionRuntime::new(
        SessionController::new(Limits::default(), StoryStyle::default()),
        cases,
        PreviewLimit::default(),
    );
    let mut input = cyoa_presentation::terminal::TerminalInput::new()?;
    let mut story = cyoa_presentation::terminal::Flags::new(io::stdout())?;
    let mut control = cyoa_presentation::terminal::Flags::new(io::stderr())?;
    let mut runtime = PersistedSession::new(
        runtime,
        StorageRunner::new(move || SupervisedRepository::new(helper.clone())),
        source,
    );
    if let Some(loaded) = loaded {
        runtime.admit_loaded(loaded)?;
    }
    let demo = source != StorySource::Live;
    cyoa_presentation::headless::run(&mut runtime, &mut input, &mut story, &mut control, demo)?;
    Ok(())
}
#[cfg(target_os = "linux")]
fn query_output(
    render: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let interrupt = cyoa_presentation::terminal::InterruptFlag::new()?;
    let mut out = cyoa_presentation::terminal::Flags::new(io::stdout())?;
    cyoa_presentation::headless::query_output(out.get_mut(), render, || interrupt.take())?;
    Ok(())
}
