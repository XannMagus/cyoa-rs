use cyoa_application::{
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
    commands::{BackendChoice, Cli, Command, PlayOptions, PlaySource},
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
        use cyoa_presentation::terminal::run_interruptible;
        let helper = HelperConfig::new(std::env::current_exe()?, data_directory(cli.data_dir)?)?;
        let repository = |helper| PersistenceUseCases::new(SupervisedRepository::new(helper));
        let options = match cli.command {
            Command::List(options) => {
                let query = options.query();
                let result = run_interruptible("save listing", move |token| {
                    repository(helper).list_saves(query, token)
                })?;
                return query_output(|out| {
                    cyoa_presentation::headless::render_listing(&result, out)
                });
            }
            Command::Inspect(options) => {
                let query = options.query();
                let stored = run_interruptible("save inspection", move |token| {
                    repository(helper).inspect_save(query, token)
                })?;
                return query_output(|out| {
                    cyoa_presentation::headless::render_saved_game(&stored, out)
                });
            }
            Command::Play(options) => options,
        };
        let play_source = options.source()?;
        let source = play_source.story_source();
        let loaded = match options.load()? {
            Some(command) => {
                let config = helper.clone();
                let loaded = run_interruptible("startup load", move |token| {
                    repository(config).load_game(command, token)
                })?;
                validate_loaded_source(&loaded, source)?;
                Some(loaded)
            }
            None => None,
        };
        // Auth preflight precedes the UI. No background stdin reader is spawned.
        match play_source {
            PlaySource::Demo => play(demo::harbour_v1()?, source, helper, loaded),
            PlaySource::Backend(backend) => play_live(backend, &options, helper, loaded),
        }
    }
}
#[cfg(target_os = "linux")]
fn play_live(
    backend: BackendChoice,
    options: &PlayOptions,
    helper: HelperConfig,
    loaded: Option<LoadedGame>,
) -> Result<(), Box<dyn std::error::Error>> {
    use cyoa_presentation::terminal::run_interruptible;
    let selected_path = std::env::var_os("PATH").unwrap_or_default();
    let home = options
        .home
        .clone()
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .ok_or("HOME is unset; provide --home")?;
    let base = std::env::current_dir()?;
    let executable =
        |default: &'static str| options.executable.clone().unwrap_or_else(|| default.into());
    match backend {
        BackendChoice::Codex => {
            let config = CodexInvocationConfig::new(
                CodexExecutable::resolve(
                    &executable(CodexExecutable::DEFAULT_NAME),
                    &selected_path,
                    &base,
                )?,
                home,
                selected_path,
                config_dir(options, CodexInvocationConfig::CONFIG_DIR_VARIABLE),
                options.model.clone().map(CodexModel::new).transpose()?,
            )?;
            play_backend(
                run_interruptible("authentication", move |token| {
                    CodexCliBackend::connect(config, token)
                })?,
                helper,
                loaded,
            )
        }
        BackendChoice::Claude => {
            let config = ClaudeInvocationConfig::new(
                ClaudeExecutable::resolve(
                    &executable(ClaudeExecutable::DEFAULT_NAME),
                    &selected_path,
                    &base,
                )?,
                home,
                selected_path,
                config_dir(options, ClaudeInvocationConfig::CONFIG_DIR_VARIABLE),
                options.model.clone().map(ClaudeModel::new).transpose()?,
            )?;
            play_backend(
                run_interruptible("authentication", move |token| {
                    ClaudeCliBackend::connect(config, token)
                })?,
                helper,
                loaded,
            )
        }
    }
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
