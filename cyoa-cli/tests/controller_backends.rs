//! Cross-layer real-child fixtures. No vendor login, network or live-fiction claim.
#![cfg(target_os = "linux")]
use cyoa_application::{
    cancellation::{CancellationSource, CancellationToken},
    generation::{StoryUseCases, TurnDirection},
};
use cyoa_core::{limits::Limits, style::StoryStyle, text::Brief, world::PlayablePosition};
use cyoa_infrastructure::{
    backend::*,
    backends::{
        claude_cli::*,
        codex_cli::*,
        process::{MaxStderrBytes, MaxStdoutBytes, ProcessBounds},
    },
    generation::{engine::GenerationEngine, templates::GenerationTemplates},
};
use cyoa_presentation::{runtime::*, session::*, worker::*};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

enum Peer {
    Claude(ClaudeCliBackend),
    Codex(CodexCliBackend),
}
impl Backend for Peer {
    fn generate(
        &mut self,
        r: GenerationRequest<'_>,
        token: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        match self {
            Self::Claude(b) => b.generate(r, token, on_json),
            Self::Codex(b) => b.generate(r, token, on_json),
        }
    }
}
fn executable() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        escargot::CargoBuild::new()
            .manifest_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml"))
            .package("cyoa-infrastructure")
            .bin("subprocess_fixture")
            .args(["--locked", "--offline"])
            .run()
            .unwrap()
            .path()
            .to_path_buf()
    })
}
struct Fixture {
    home: tempfile::TempDir,
    claude: bool,
}
impl Fixture {
    fn new(claude: bool) -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            claude,
        }
    }
    fn report(&self) -> PathBuf {
        self.home.path().join("report.json")
    }
    fn set(&self, payload: &Value, exit: i32, hang: bool) {
        let lines = if self.claude {
            vec![
                json!({"type":"system","subtype":"init","apiKeySource":"none","model":"fixture"}),
                json!({"type":"result","subtype":"success","is_error":false,"structured_output":payload}),
            ]
        } else {
            vec![
                json!({"type":"thread.started","thread_id":"fixture"}),
                json!({"type":"turn.started"}),
                json!({"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":payload.to_string()}}),
                json!({"type":"turn.completed"}),
            ]
        };
        let stdout = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let mut scenario = json!({"stdout":[{"bytes":stdout.as_bytes()}],"stderr":[{"bytes":[255,13,10]}],"drain_stdin":true,"report_path":self.report(),"exit_code":exit,"auth_stdout":self.claude,"auth_status":if self.claude {r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty"}"#} else {"Logged in using ChatGPT\n"}});
        if hang {
            scenario["hang_ms"] = json!(10000);
        }
        let _ = fs::remove_file(self.report());
        fs::write(
            self.home.path().join("cyoa-fixture.json"),
            serde_json::to_vec(&scenario).unwrap(),
        )
        .unwrap();
    }
    fn backend(&self) -> Peer {
        let bounds = ProcessBounds::new(
            Duration::from_secs(3),
            MaxStdoutBytes::new(1_048_576).unwrap(),
            MaxStderrBytes::new(4096).unwrap(),
        )
        .unwrap();
        let token = CancellationSource::default().token();
        if self.claude {
            let exe = ClaudeExecutable::resolve(
                executable().as_os_str(),
                "/bin".as_ref(),
                self.home.path(),
            )
            .unwrap();
            Peer::Claude(
                ClaudeCliBackend::connect(
                    ClaudeInvocationConfig::new(
                        exe,
                        self.home.path().into(),
                        "/bin".into(),
                        None,
                        None,
                    )
                    .unwrap()
                    .with_bounds(bounds),
                    &token,
                )
                .unwrap(),
            )
        } else {
            let exe = CodexExecutable::resolve(
                executable().as_os_str(),
                "/bin".as_ref(),
                self.home.path(),
            )
            .unwrap();
            Peer::Codex(
                CodexCliBackend::connect(
                    CodexInvocationConfig::new(
                        exe,
                        self.home.path().into(),
                        "/bin".into(),
                        None,
                        None,
                    )
                    .unwrap()
                    .with_bounds(bounds),
                    &token,
                )
                .unwrap(),
            )
        }
    }
    fn wait_started(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.report().exists() {
            assert!(Instant::now() < deadline, "child start watchdog");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn cleanup(&self) -> Value {
        let report: Value = serde_json::from_slice(&fs::read(self.report()).unwrap()).unwrap();
        assert!(!PathBuf::from(report["cwd"].as_str().unwrap()).exists());
        let pid = rustix::process::Pid::from_raw(report["pid"].as_i64().unwrap() as i32).unwrap();
        assert_eq!(
            rustix::process::test_kill_process(pid),
            Err(rustix::io::Errno::SRCH)
        );
        report
    }
    fn launches(&self) -> Vec<String> {
        fs::read_to_string(self.home.path().join("launches.jsonl"))
            .unwrap()
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["command"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }
}
type Runtime = SessionRuntime<GenerationEngine<Peer>>;
fn settle(runtime: &mut Runtime) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(
        runtime.controller().phase(),
        Phase::Running | Phase::Cancelling | Phase::Closing
    ) {
        runtime.poll();
        assert!(Instant::now() < deadline, "runtime completion watchdog");
        thread::sleep(Duration::from_millis(1));
    }
}
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../cyoa-infrastructure/tests/fixtures/phase0_story.json"
    ))
    .unwrap()
}

#[test]
fn both_adapters_preserve_controller_state_through_failure_cancel_retry_and_quit() {
    let data = data();
    for claude in [false, true] {
        let fixture = Fixture::new(claude);
        fixture.set(&data["outline"], 0, false);
        let mut runtime = SessionRuntime::new(
            SessionController::new(Limits::default(), StoryStyle::default()),
            StoryUseCases::new(GenerationEngine::new(
                fixture.backend(),
                GenerationTemplates::bundled().unwrap(),
            )),
            PreviewLimit::default(),
        );
        runtime
            .dispatch(Intent::SubmitBrief(Brief::new("bell").unwrap()))
            .unwrap();
        settle(&mut runtime);
        fixture.cleanup();
        assert_eq!(runtime.controller().phase(), Phase::Ready);
        fixture.set(&data["cast"], 0, false);
        runtime.dispatch(Intent::AcceptOutline).unwrap();
        settle(&mut runtime);
        fixture.cleanup();
        runtime
            .dispatch(Intent::Select(PlayablePosition::new(1)))
            .unwrap();
        fixture.set(&data["turns"][0], 0, false);
        runtime
            .dispatch(Intent::Turn(TurnDirection::Continue))
            .unwrap();
        settle(&mut runtime);
        fixture.cleanup();
        assert_eq!(
            runtime.controller().game().unwrap().turns().len(),
            1,
            "double commit regression"
        );
        let before = runtime.controller().game().unwrap().clone();
        fixture.set(&data["turns"][1], 7, false);
        runtime
            .dispatch(Intent::Turn(TurnDirection::Continue))
            .unwrap();
        settle(&mut runtime);
        let failed = fixture.cleanup();
        assert_eq!(runtime.controller().game().unwrap(), &before);
        let Some(Failure::Generation(error)) = runtime.controller().failure() else {
            panic!("candidate failure absent")
        };
        assert_eq!(error.raw_response().as_str(), data["turns"][1].to_string());
        assert_eq!(error.diagnostics().stderr(), [255, 13, 10]);
        fixture.set(&data["turns"][1], 0, false);
        runtime.dispatch(Intent::Retry).unwrap();
        settle(&mut runtime);
        let retried = fixture.cleanup();
        assert_eq!(failed["stdin"], retried["stdin"]);
        assert_eq!(runtime.controller().game().unwrap().turns().len(), 2);
        let before_cancel = runtime.controller().game().unwrap().clone();
        fixture.set(&data["turns"][2], 0, true);
        runtime
            .dispatch(Intent::Turn(TurnDirection::Continue))
            .unwrap();
        fixture.wait_started();
        runtime.dispatch(Intent::Cancel).unwrap();
        assert!(runtime.dispatch(Intent::Retry).is_err());
        settle(&mut runtime);
        let cancelled = fixture.cleanup();
        assert_eq!(runtime.controller().game().unwrap(), &before_cancel);
        let Some(Failure::Cancelled {
            rejected: Err(error),
        }) = runtime.controller().failure()
        else {
            panic!("cancelled child outcome absent")
        };
        assert_eq!(
            error.kind(),
            cyoa_application::generation::FailureKind::Cancelled
        );
        fixture.set(&data["turns"][2], 0, false);
        runtime.dispatch(Intent::Retry).unwrap();
        settle(&mut runtime);
        let retry = fixture.cleanup();
        assert_eq!(cancelled["stdin"], retry["stdin"]);
        assert_eq!(runtime.controller().game().unwrap().turns().len(), 3);
        fixture.set(&data["turns"][3], 0, true);
        runtime
            .dispatch(Intent::Turn(TurnDirection::Continue))
            .unwrap();
        fixture.wait_started();
        runtime.dispatch(Intent::Quit).unwrap();
        assert_eq!(runtime.controller().phase(), Phase::Closing);
        settle(&mut runtime);
        fixture.cleanup();
        assert_eq!(runtime.controller().phase(), Phase::Closed);
        assert_eq!(runtime.controller().game().unwrap().turns().len(), 3);
        let launches = fixture.launches();
        assert_eq!(launches.len(), 9);
        assert_eq!(launches[0], if claude { "auth" } else { "login" });
        assert!(
            launches[1..]
                .iter()
                .all(|command| command == if claude { "-p" } else { "exec" })
        );
    }
}

#[test]
fn both_joined_adapter_successes_are_rejected_if_cancel_wins_acceptance() {
    let data = data();
    for claude in [false, true] {
        let fixture = Fixture::new(claude);
        fixture.set(&data["outline"], 0, false);
        let mut controller = SessionController::new(Limits::default(), StoryStyle::default());
        let mut runner = WorkerRunner::new(
            StoryUseCases::new(GenerationEngine::new(
                fixture.backend(),
                GenerationTemplates::bundled().unwrap(),
            )),
            PreviewLimit::default(),
        );
        runner
            .start(
                controller
                    .submit_brief(Brief::new("bell").unwrap())
                    .unwrap(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let done = loop {
            let terminal = runner.poll().into_iter().find_map(|event| {
                if let WorkerEvent::Finished(c) = event {
                    Some(c)
                } else {
                    None
                }
            });
            if let Some(c) = terminal {
                break c;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        };
        assert!(done.outcome.is_ok());
        assert!(runner.is_ready());
        fixture.cleanup();
        controller.cancel();
        assert_eq!(
            controller.complete(done),
            Acceptance::Failed,
            "cancelled success regression"
        );
        assert!(matches!(controller.stage(), Stage::Brief));
    }
}
