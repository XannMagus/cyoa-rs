use cyoa_application::{
    cancellation::CancellationSource,
    generation::{StoryGenerator, StoryUseCases},
};
use cyoa_core::{limits::Limits, style::StoryStyle};
use cyoa_infrastructure::{
    backend::Backend,
    backends::{
        claude_cli::{ClaudeCliBackend, ClaudeExecutable, ClaudeInvocationConfig, ClaudeModel},
        codex_cli::{CodexCliBackend, CodexExecutable, CodexInvocationConfig, CodexModel},
    },
    generation::{demo, engine::GenerationEngine, templates::GenerationTemplates},
};
use cyoa_presentation::{
    commands::{BackendChoice, Cli, Command, PlayOptions},
    runtime::SessionRuntime,
    session::SessionController,
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
    let Command::Play(options) = Cli::parse().command;
    #[cfg(not(target_os = "linux"))]
    {
        let _ = options;
        Err(cyoa_presentation::terminal::unsupported().into())
    }
    #[cfg(target_os = "linux")]
    {
        // Auth preflight precedes the UI. No background stdin reader is spawned.
        if options.demo {
            play(demo::harbour_v1()?, true)
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
                    play_backend(connect(move |token| {
                        CodexCliBackend::connect(config, token)
                    })?)
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
                    play_backend(connect(move |token| {
                        ClaudeCliBackend::connect(config, token)
                    })?)
                }
            }
        }
    }
}
#[cfg(target_os = "linux")]
fn connect<T: Send>(
    job: impl FnOnce(
        &cyoa_application::cancellation::CancellationToken,
    ) -> Result<T, cyoa_infrastructure::backend::BackendError>
    + Send,
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
fn play_backend<B: Backend + Send + 'static>(backend: B) -> Result<(), Box<dyn std::error::Error>> {
    play(
        GenerationEngine::new(backend, GenerationTemplates::bundled()?),
        false,
    )
}
#[cfg(target_os = "linux")]
fn play<G: StoryGenerator + Send + 'static>(
    generator: G,
    demo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let cases = StoryUseCases::new(generator);
    let mut runtime = SessionRuntime::new(
        SessionController::new(Limits::default(), StoryStyle::default()),
        cases,
        PreviewLimit::default(),
    );
    let mut input = cyoa_presentation::terminal::TerminalInput::new()?;
    let mut story = cyoa_presentation::terminal::Flags::new(io::stdout())?;
    let mut control = cyoa_presentation::terminal::Flags::new(io::stderr())?;
    cyoa_presentation::headless::run(&mut runtime, &mut input, &mut story, &mut control, demo)?;
    Ok(())
}
