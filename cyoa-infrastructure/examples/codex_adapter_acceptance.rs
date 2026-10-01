//! Opt-in Linux live gate; never invoked by credential-free tests.
//! cargo run -p cyoa-infrastructure --example codex_adapter_acceptance -- NEW_DIR
//! Records synthetic story requests only. No credentials or environment dumps.
use cyoa_application::{
    cancellation::{CancellationSource, CancellationToken},
    diagnostics::TransportDiagnostics,
    generation::{FailureKind, StoryUseCases, TurnDirection},
};
use cyoa_core::{
    game::GameState, limits::Limits, style::StoryStyle, text::Brief, world::PlayablePosition,
};
use cyoa_infrastructure::{
    backend::{Backend, BackendError, GenerationRequest, GenerationResponse},
    backends::codex_cli::{CodexCliBackend, CodexExecutable, CodexInvocationConfig},
    generation::{engine::GenerationEngine, templates::GenerationTemplates},
};
use serde_json::json;
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn write_json(path: impl AsRef<Path>, value: &serde_json::Value) -> Result<(), Error> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

// Observation only: the production adapter still owns launch, signaling and reap.
// Read only this process's direct children; never inspect unrelated processes.
fn observe_child() -> Option<(u32, PathBuf, Vec<u8>, Vec<u8>)> {
    let tasks = fs::read_dir("/proc/self/task").ok()?;
    for task in tasks.flatten() {
        let children = fs::read_to_string(task.path().join("children")).unwrap_or_default();
        for pid in children
            .split_whitespace()
            .filter_map(|s| s.parse::<u32>().ok())
        {
            let root = PathBuf::from(format!("/proc/{pid}"));
            let Ok(argv) = fs::read(root.join("cmdline")) else {
                continue;
            };
            if !argv.split(|b| *b == 0).any(|a| a == b"--output-schema") {
                continue;
            }
            let Ok(cwd) = fs::read_link(root.join("cwd")) else {
                continue;
            };
            let Ok(schema) = fs::read(cwd.join("schema.json")) else {
                continue;
            };
            return Some((pid, cwd, argv, schema));
        }
    }
    None
}

struct Recorder {
    backend: CodexCliBackend,
    output: PathBuf,
    call: usize,
    cancel_source: Arc<CancellationSource>,
}
impl Recorder {
    fn record(
        &mut self,
        request: GenerationRequest<'_>,
        cancel: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<Result<GenerationResponse, BackendError>, Error> {
        self.call += 1;
        let output = self.output.join(format!("{:02}", self.call));
        fs::create_dir(&output)?;
        fs::write(output.join("instructions.txt"), request.instructions)?;
        fs::write(output.join("prompt.txt"), request.prompt)?;
        write_json(output.join("shared-schema.json"), request.schema)?;
        let controlled_cancel = self.call == 5;
        let source = Arc::clone(&self.cancel_source);
        let (stop, stopped) = mpsc::channel();
        let observer = thread::spawn(move || {
            let mut observed = None;
            let mut seen_at = None;
            let mut cancelled_at = None;
            loop {
                if observed.is_none() {
                    observed = observe_child();
                    if observed.is_some() {
                        seen_at = Some(Instant::now());
                    }
                }
                if controlled_cancel
                    && cancelled_at.is_none()
                    && seen_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(1))
                {
                    source.cancel();
                    cancelled_at = Some(Instant::now());
                }
                match stopped.recv_timeout(Duration::from_millis(5)) {
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                    _ => return (observed, cancelled_at),
                }
            }
        });
        let started = Instant::now();
        let mut emissions = Vec::new();
        let result = self.backend.generate(request, cancel, &mut |text| {
            emissions.push(json!({"elapsed_ms":started.elapsed().as_millis(), "text":text}));
            on_json(text);
        });
        let elapsed = started.elapsed().as_millis();
        let _ = stop.send(());
        let (observed, cancelled_at) = observer.join().map_err(|_| "observer panicked")?;
        let (raw, diagnostics, outcome, usage, observed_model) = match &result {
            Ok(response) => (
                response.raw_response(),
                response.diagnostics(),
                "accepted".to_string(),
                Some(response.usage()),
                format!("{:?}", response.provenance()),
            ),
            Err(error) => {
                let (raw, diagnostics) = error_evidence(error);
                (
                    raw,
                    diagnostics,
                    error.to_string(),
                    None,
                    "unreported".into(),
                )
            }
        };
        fs::write(output.join("payload.json"), raw)?;
        fs::write(output.join("stdout.jsonl"), diagnostics.stdout())?;
        fs::write(output.join("stderr.bin"), diagnostics.stderr())?;
        write_json(output.join("emissions.json"), &json!(emissions))?;
        let cleanup = if let Some((pid, cwd, argv, schema)) = observed {
            fs::write(output.join("argv.bin"), argv)?;
            fs::write(output.join("adapted-schema.json"), schema)?;
            json!({"pid":pid, "workspace":cwd, "pid_absent":!Path::new(&format!("/proc/{pid}")).exists(), "workspace_absent":!cwd.exists()})
        } else {
            json!(null)
        };
        write_json(
            output.join("result.json"),
            &json!({
                "outcome":outcome, "elapsed_ms":elapsed, "requested_model":null, "observed_provenance":observed_model,
                "input_tokens":usage.and_then(|u|u.input).map(|i|i.total()), "cached_input_tokens":usage.and_then(|u|u.input).and_then(|i|i.cached()), "output_tokens":usage.and_then(|u|u.output),
                "emissions":emissions.len(), "exact_single_emission":emissions.len()==1 && emissions[0]["text"]==raw,
                "cleanup":cleanup, "controlled_cancel":controlled_cancel, "cancel_to_return_ms":cancelled_at.map(|t|t.elapsed().as_millis()),
                "exit_evidence":"Accepted implies zero exit and reap through production supervisor; cancellation has no public exit-code field. Raw terminal is in stdout.jsonl."
            }),
        )?;
        println!("call {}: {outcome} ({elapsed} ms)", self.call);
        if result.is_ok() || controlled_cancel {
            if cleanup["pid_absent"] != true || cleanup["workspace_absent"] != true {
                return Err("missing or failed direct PID/workspace cleanup evidence".into());
            }
            if controlled_cancel && cancelled_at.is_none() {
                return Err("controlled cancellation was not observed".into());
            }
        }
        Ok(result)
    }
}
impl Backend for Recorder {
    fn generate(
        &mut self,
        request: GenerationRequest<'_>,
        cancel: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        self.record(request, cancel, on_json)
            .map_err(|e| BackendError::Unavailable {
                message: format!("live evidence harness: {e}"),
                diagnostics: TransportDiagnostics::empty(),
            })?
    }
}
fn error_evidence(error: &BackendError) -> (&str, &TransportDiagnostics) {
    match error {
        BackendError::Cancelled {
            raw_response,
            diagnostics,
        }
        | BackendError::Timeout {
            raw_response,
            diagnostics,
        } => (raw_response.as_deref().unwrap_or(""), diagnostics),
        BackendError::Generation {
            raw_response,
            diagnostics,
            ..
        } => (raw_response, diagnostics),
        BackendError::Transport {
            raw_response,
            diagnostics,
            ..
        } => (raw_response, diagnostics),
        BackendError::Unavailable { diagnostics, .. } => ("", diagnostics),
    }
}
fn run() -> Result<(), Error> {
    if !cfg!(target_os = "linux") {
        return Err("live process observation requires Linux /proc".into());
    }
    let output = PathBuf::from(
        env::args_os()
            .nth(1)
            .ok_or("supply a new output directory")?,
    );
    fs::create_dir(&output)?;
    let path = env::var_os("PATH").ok_or("PATH missing")?;
    let executable = CodexExecutable::resolve("codex".as_ref(), &path, &env::current_dir()?)?;
    let config = CodexInvocationConfig::new(
        executable,
        env::var_os("HOME").ok_or("HOME missing")?.into(),
        path,
        env::var_os("CODEX_HOME").map(PathBuf::from),
        None,
    )?;
    let source = Arc::new(CancellationSource::default());
    let backend = match CodexCliBackend::connect(config, &source.token()) {
        Ok(backend) => backend,
        Err(error) => {
            let (_, diagnostics) = error_evidence(&error);
            fs::write(output.join("auth-stdout.bin"), diagnostics.stdout())?;
            fs::write(output.join("auth-stderr.bin"), diagnostics.stderr())?;
            write_json(
                output.join("result.json"),
                &json!({"accepted":false,"stage":"auth","error":error.to_string()}),
            )?;
            return Err(error.into());
        }
    };
    let recorder = Recorder {
        backend,
        output: output.clone(),
        call: 0,
        cancel_source: Arc::clone(&source),
    };
    let mut cases = StoryUseCases::new(GenerationEngine::new(
        recorder,
        GenerationTemplates::bundled()?,
    ));
    let brief = Brief::new(
        "A quiet harbour where two unrelated lighthouse keepers both named Ajax uncover a missing bell. Both Ajaxes are distinct playable characters with different histories.",
    )?;
    let limits = Limits::default();
    let outline = cases
        .generate_outline(&brief, &source.token())?
        .into_parts()
        .0;
    let world = cases.generate_world(&brief, outline, &limits, &source.token())?;
    let mut state = GameState::start(
        brief,
        world.select(PlayablePosition::new(0))?,
        StoryStyle::default(),
        limits,
    );
    fs::write(output.join("initial-state.txt"), format!("{state:#?}"))?;
    for label in ["opening", "continuation"] {
        let mut progress = Vec::new();
        cases.take_turn(
            &mut state,
            TurnDirection::Continue,
            &source.token(),
            &mut |s| progress.push(s.to_owned()),
        )?;
        write_json(
            output.join(format!("{label}-progress.json")),
            &json!(progress),
        )?;
        fs::write(
            output.join(format!("{label}-state.txt")),
            format!("{state:#?}"),
        )?;
    }
    let before = state.clone();
    let mut progress = Vec::new();
    let result = cases.take_turn(
        &mut state,
        TurnDirection::Continue,
        &source.token(),
        &mut |s| progress.push(s.to_owned()),
    );
    let cancelled = result
        .as_ref()
        .is_err_and(|e| e.kind() == FailureKind::Cancelled);
    let calls = cases.into_generator().into_backend().call;
    write_json(
        output.join("result.json"),
        &json!({"accepted":cancelled && state==before && calls==5, "generation_calls":calls,"committed_turns":state.turns().len(),"cancelled":cancelled,"state_unchanged":state==before,"cancelled_progress":progress,"error":result.err().map(|e|e.to_string())}),
    )?;
    if !cancelled || state != before || calls != 5 {
        return Err("live acceptance assertions failed".into());
    }
    Ok(())
}
