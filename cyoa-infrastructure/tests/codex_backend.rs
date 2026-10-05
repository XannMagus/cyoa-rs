//! Actual Codex adapter, controlled fixture executable, real supervisor. No auth/network.
#![cfg(target_os = "linux")]
use cyoa_application::{cancellation::CancellationSource, diagnostics::TransportDiagnostics};
use cyoa_infrastructure::{
    backend::{Backend, BackendError, GenerationRequest},
    backends::{
        codex_cli::{CodexCliBackend, CodexExecutable, CodexInvocationConfig},
        process::{MaxStderrBytes, MaxStdoutBytes, ProcessBounds},
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

struct Fixture {
    home: tempfile::TempDir,
    config: CodexInvocationConfig,
}
impl Fixture {
    fn new(stdout: &[u8], exit: i32) -> Self {
        let home = tempfile::tempdir().unwrap();
        let exe = CodexExecutable::resolve(
            env!("CARGO_BIN_EXE_subprocess_fixture").as_ref(),
            "/bin".as_ref(),
            home.path(),
        )
        .unwrap();
        let config = CodexInvocationConfig::new(
            exe,
            home.path().into(),
            "/bin".into(),
            Some(home.path().join("auth")),
            None,
        )
        .unwrap()
        .with_bounds(bounds(3000, 1_048_576));
        let fixture = Self { home, config };
        fixture.set(json!({"stdout":[{"bytes":stdout}],"stderr":[{"bytes":b"diagnostic\xff\r\n"}],"drain_stdin":true,"report_path":fixture.report(),"exit_code":exit}));
        fixture
    }
    fn report(&self) -> PathBuf {
        self.home.path().join("report.json")
    }
    fn set(&self, scenario: Value) {
        fs::write(
            self.home.path().join("cyoa-fixture.json"),
            serde_json::to_vec(&scenario).unwrap(),
        )
        .unwrap();
    }
    fn update(&self, key: &str, value: Value) {
        let mut v: Value =
            serde_json::from_slice(&fs::read(self.home.path().join("cyoa-fixture.json")).unwrap())
                .unwrap();
        v[key] = value;
        self.set(v);
    }
    fn backend(&self) -> CodexCliBackend {
        CodexCliBackend::connect(self.config.clone(), &CancellationSource::default().token())
            .unwrap()
    }
    fn assert_cleanup(&self) -> Value {
        let report: Value = serde_json::from_slice(&fs::read(self.report()).unwrap()).unwrap();
        assert!(!PathBuf::from(report["cwd"].as_str().unwrap()).exists());
        let pid = rustix::process::Pid::from_raw(report["pid"].as_i64().unwrap() as i32).unwrap();
        assert_eq!(
            rustix::process::test_kill_process(pid),
            Err(rustix::io::Errno::SRCH)
        );
        report
    }
}
fn bounds(ms: u64, stdout: usize) -> ProcessBounds {
    ProcessBounds::new(
        Duration::from_millis(ms),
        MaxStdoutBytes::new(stdout).unwrap(),
        MaxStderrBytes::new(4096).unwrap(),
    )
    .unwrap()
}
fn capture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../reviews/2026-09-26-codex-profile/synthetic")
            .join(name),
    )
    .unwrap()
}
fn payload() -> String {
    " \r\n{\"title\":\"Harbour\",\"world_description\":\"A sheltered harbour — 灯\"}\n".into()
}
fn request(schema: &Value) -> GenerationRequest<'_> {
    GenerationRequest {
        instructions: "Instructions \"灯\"\r\n",
        prompt: "Player text\n😀",
        schema,
    }
}
fn evidence(error: &BackendError) -> (&str, &TransportDiagnostics) {
    match error {
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
        BackendError::Cancelled {
            raw_response,
            diagnostics,
        }
        | BackendError::Timeout {
            raw_response,
            diagnostics,
        } => (raw_response.as_deref().unwrap_or(""), diagnostics),
        BackendError::Unavailable { diagnostics, .. } => ("", diagnostics),
    }
}

#[test]
fn candidate_and_success_terminal_cannot_override_nonzero_process_exit() {
    let stdout = capture("success.jsonl");
    let fixture = Fixture::new(&stdout, 7);
    let mut seen = vec![];
    let error = fixture
        .backend()
        .generate(
            request(&json!({})),
            &CancellationSource::default().token(),
            &mut |s| seen.push(s.to_owned()),
        )
        .unwrap_err();
    assert!(matches!(error, BackendError::Generation { .. }));
    assert!(error.to_string().contains('7'));
    assert_eq!(evidence(&error).0, payload());
    assert_eq!(evidence(&error).1.stdout(), stdout);
    assert_eq!(evidence(&error).1.stderr(), b"diagnostic\xff\r\n");
    assert!(seen.is_empty());
    fixture.assert_cleanup();
}

#[test]
fn protocol_rejection_and_missing_completion_keep_candidate_without_emission() {
    for name in [
        "failed-after-candidate.jsonl",
        "duplicate-terminal.jsonl",
        "missing-terminal.jsonl",
        "truncated.jsonl",
    ] {
        let fixture = Fixture::new(&capture(name), 0);
        let mut seen = 0;
        let error = fixture
            .backend()
            .generate(
                request(&json!({})),
                &CancellationSource::default().token(),
                &mut |_| seen += 1,
            )
            .unwrap_err();
        assert_eq!(evidence(&error).0, payload(), "{name}");
        assert_eq!(seen, 0);
        fixture.assert_cleanup();
    }
}

// Cleanup is checked before the behavioral assertion so a surviving protocol
// mutant cannot hide a leaked child or workspace behind an early panic.
fn assert_protocol_rejected(name: &str) {
    assert_transcript_rejected(name, &capture(name));
}
fn assert_transcript_rejected(name: &str, transcript: &[u8]) {
    let fixture = Fixture::new(transcript, 0);
    let mut seen = 0;
    let result = fixture.backend().generate(
        request(&json!({})),
        &CancellationSource::default().token(),
        &mut |_| seen += 1,
    );
    fixture.assert_cleanup();
    if let Err(error) = &result {
        assert!(
            matches!(error, BackendError::Generation { .. }),
            "unexpected transport failure: {error}"
        );
    }
    assert!(
        result.is_err(),
        "protocol acceptance regression: {name}: {result:?}"
    );
    assert_eq!(seen, 0);
}

#[test]
fn duplicated_terminal_type_cannot_authorize_success_at_the_real_boundary() {
    // The last `type` reads as a successful completion; the first is a failure.
    let success = String::from_utf8(capture("success.jsonl")).unwrap();
    let mut records: Vec<&str> = success.lines().collect();
    records[3] = r#"{"type":"turn.failed","type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;
    assert_transcript_rejected("duplicated terminal type", records.join("\n").as_bytes());
}

#[test]
fn ineligible_message_is_rejected_after_cleanup() {
    assert_protocol_rejected("ineligible.jsonl");
}

#[test]
fn candidate_without_terminal_is_rejected_after_cleanup() {
    assert_protocol_rejected("missing-terminal.jsonl");
}

#[test]
fn conflicting_terminal_is_rejected_after_cleanup() {
    assert_protocol_rejected("conflicting-terminal.jsonl");
}

#[test]
fn preflight_rejects_non_subscription_unknown_and_conflicting_auth_without_generation() {
    for status in [
        "Logged in using an API key",
        "Not logged in",
        "unknown",
        "Logged in using ChatGPT\nLogged in using an API key\n",
    ] {
        let fixture = Fixture::new(&capture("success.jsonl"), 0);
        fixture.update("auth_status", json!(status));
        let error = CodexCliBackend::connect(
            fixture.config.clone(),
            &CancellationSource::default().token(),
        )
        .unwrap_err();
        assert!(matches!(error, BackendError::Unavailable { .. }));
        assert!(!fixture.report().exists());
        assert_eq!(evidence(&error).1.stderr(), status.as_bytes());
    }
}

#[test]
fn timeout_after_candidate_retains_unemitted_payload_and_reaps_child() {
    let mut fixture = Fixture::new(&capture("missing-terminal.jsonl"), 0);
    fixture.config = fixture.config.clone().with_bounds(bounds(500, 4096));
    fixture.update("hang_after_output_ms", json!(10000));
    let mut seen = 0;
    let error = fixture
        .backend()
        .generate(
            request(&json!({})),
            &CancellationSource::default().token(),
            &mut |_| seen += 1,
        )
        .unwrap_err();
    assert!(matches!(error, BackendError::Timeout { .. }));
    assert_eq!(evidence(&error).0, payload());
    assert_eq!(seen, 0);
    fixture.assert_cleanup();
}

#[test]
fn silent_cancellation_uses_a_child_handshake_and_pre_cancel_launches_nothing() {
    let fixture = Fixture::new(&[], 0);
    fixture.update("hang_ms", json!(10000));
    let mut backend = fixture.backend();
    let source = CancellationSource::default();
    let token = source.token();
    let report = fixture.report();
    let canceller = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !report.exists() {
            if Instant::now() >= deadline {
                source.cancel();
                panic!("child report missing");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        source.cancel();
    });
    let error = backend
        .generate(request(&json!({})), &token, &mut |_| panic!("no preview"))
        .unwrap_err();
    canceller.join().unwrap();
    assert!(matches!(error, BackendError::Cancelled { .. }));
    fixture.assert_cleanup();
    fs::remove_file(fixture.report()).unwrap();
    assert!(matches!(
        backend.generate(request(&json!({})), &token, &mut |_| {}),
        Err(BackendError::Cancelled { .. })
    ));
    assert!(!fixture.report().exists());
    assert!(matches!(
        CodexCliBackend::connect(fixture.config.clone(), &token),
        Err(BackendError::Cancelled { .. })
    ));
}

#[test]
fn capped_output_preserves_prefix_metadata_and_a_structured_cause() {
    use cyoa_application::diagnostics::CaptureCompleteness;
    use cyoa_infrastructure::backends::process::SupervisorError;
    let candidate = capture("missing-terminal.jsonl");
    let mut output = candidate.clone();
    output.extend_from_slice(&[b'x'; 8192]);
    let mut fixture = Fixture::new(&output, 0);
    fixture.config = fixture
        .config
        .clone()
        .with_bounds(bounds(3000, candidate.len() + 128));
    let error = fixture
        .backend()
        .generate(
            request(&json!({})),
            &CancellationSource::default().token(),
            &mut |_| panic!("no preview"),
        )
        .unwrap_err();
    let BackendError::Transport {
        diagnostics, cause, ..
    } = &error
    else {
        panic!("{error}")
    };
    assert!(matches!(
        cause.downcast_ref::<SupervisorError>(),
        Some(SupervisorError::OutputBoundExceeded { .. })
    ));
    assert_eq!(diagnostics.stdout().len(), candidate.len() + 128);
    assert_eq!(
        diagnostics.stdout_capture().completeness(),
        CaptureCompleteness::Prefix
    );
    // A cap may be detected before delivery of captured records. Retained
    // transcript is authoritative evidence even when no candidate was decoded.
    assert!(diagnostics.stdout().starts_with(&candidate));
    fixture.assert_cleanup();
}

#[test]
fn successful_response_emits_once_after_cleanup_and_retains_usage_and_exact_request() {
    let stdout = capture("crlf.jsonl");
    let fixture = Fixture::new(&stdout, 0);
    let mut backend = fixture.backend();
    let mut emitted = vec![];
    let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"narrative":{"type":"string"}},"additionalProperties":false});
    let response = backend
        .generate(
            request(&schema),
            &CancellationSource::default().token(),
            &mut |text| {
                fixture.assert_cleanup();
                emitted.push(text.to_owned());
            },
        )
        .unwrap();
    assert_eq!(emitted, [payload()]);
    assert_eq!(response.raw_response(), payload());
    assert_eq!(response.diagnostics().stdout(), stdout);
    assert_eq!(response.diagnostics().stderr(), b"diagnostic\xff\r\n");
    assert_eq!(response.usage().input.unwrap().total(), 10);
    assert_eq!(response.usage().input.unwrap().cached(), Some(0));
    assert_eq!(response.usage().output, Some(5));
    assert_eq!(response.provenance(), &Default::default());
    let report = fixture.assert_cleanup();
    let expected_args = [
        "exec",
        "-c",
        "project_doc_max_bytes=0",
        "--json",
        "--ephemeral",
        "--sandbox",
        "read-only",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--color",
        "never",
        "--output-schema",
        "schema.json",
        "-",
    ];
    let argv: Vec<String> = serde_json::from_value(report["argv"].clone()).unwrap();
    assert_eq!(&argv[1..], &expected_args);
    let stdin: Vec<u8> = serde_json::from_value(report["stdin"].clone()).unwrap();
    let expected = cyoa_infrastructure::generation::backend_compat::codex_cli::frame_stdin(
        request(&schema).instructions,
        request(&schema).prompt,
    );
    assert_eq!(stdin, expected);
    let schema_bytes: Vec<u8> = serde_json::from_value(report["schema"].clone()).unwrap();
    let adapted: Value = serde_json::from_slice(&schema_bytes).unwrap();
    assert_eq!(adapted["required"], json!(["narrative"]));
    assert_eq!(adapted["$schema"], schema["$schema"]);
    assert_eq!(report["env_keys"], json!(["CODEX_HOME", "HOME", "PATH"]));
    let calls = fs::read_to_string(fixture.home.path().join("launches.jsonl")).unwrap();
    let commands: Vec<Value> = calls
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["command"].clone())
        .collect();
    assert_eq!(
        commands,
        [json!("login"), json!("exec")],
        "one preflight and exactly one generation child"
    );
}

#[test]
fn cancellation_from_the_complete_callback_cannot_return_success() {
    let fixture = Fixture::new(&capture("success.jsonl"), 0);
    let source = CancellationSource::default();
    let mut seen = 0;
    let error = fixture
        .backend()
        .generate(request(&json!({})), &source.token(), &mut |_| {
            seen += 1;
            source.cancel();
        })
        .unwrap_err();
    assert!(matches!(error, BackendError::Cancelled { .. }));
    assert_eq!(evidence(&error).0, payload());
    assert_eq!(seen, 1);
    fixture.assert_cleanup();
}

#[test]
fn preparation_failure_and_removed_executable_never_launch_a_generation() {
    let fixture = Fixture::new(&[], 0);
    let mut backend = fixture.backend();
    let error = backend
        .generate(
            request(&json!({"properties":3})),
            &CancellationSource::default().token(),
            &mut |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, BackendError::Unavailable { .. }));
    assert!(!fixture.report().exists());
    // Resolve a real executable, then remove that temporary copy after auth.
    let copied = fixture.home.path().join("codex-fixture");
    fs::copy(env!("CARGO_BIN_EXE_subprocess_fixture"), &copied).unwrap();
    let exe =
        CodexExecutable::resolve(copied.as_os_str(), "/bin".as_ref(), fixture.home.path()).unwrap();
    let config =
        CodexInvocationConfig::new(exe, fixture.home.path().into(), "/bin".into(), None, None)
            .unwrap();
    let mut backend =
        CodexCliBackend::connect(config, &CancellationSource::default().token()).unwrap();
    fs::remove_file(copied).unwrap();
    let error = backend
        .generate(
            request(&json!({})),
            &CancellationSource::default().token(),
            &mut |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, BackendError::Unavailable { .. }));
    assert!(!fixture.report().exists());
}

#[test]
fn no_stdout_failure_and_invalid_protocol_preserve_bytes_with_distinct_categories() {
    for (stdout, exit, unavailable) in [
        (vec![], 1, true),
        (vec![0xff, b'\n'], 0, false),
        (vec![], 0, false),
    ] {
        let fixture = Fixture::new(&stdout, exit);
        let error = fixture
            .backend()
            .generate(
                request(&json!({})),
                &CancellationSource::default().token(),
                &mut |_| panic!("no preview"),
            )
            .unwrap_err();
        assert_eq!(
            matches!(error, BackendError::Unavailable { .. }),
            unavailable
        );
        assert_eq!(evidence(&error).0, "");
        assert_eq!(evidence(&error).1.stdout(), stdout);
        fixture.assert_cleanup();
    }
}

#[test]
fn application_receives_complete_candidate_and_diagnostics_on_adapter_timeout() {
    use cyoa_application::generation::{FailureKind, StoryUseCases};
    use cyoa_core::text::Brief;
    use cyoa_infrastructure::generation::{
        engine::GenerationEngine, templates::GenerationTemplates,
    };
    let mut fixture = Fixture::new(&capture("missing-terminal.jsonl"), 0);
    fixture.config = fixture.config.clone().with_bounds(bounds(500, 4096));
    fixture.update("hang_after_output_ms", json!(10000));
    let mut cases = StoryUseCases::new(GenerationEngine::new(
        fixture.backend(),
        GenerationTemplates::bundled().unwrap(),
    ));
    let error = cases
        .generate_outline(
            &Brief::new("Harbour").unwrap(),
            &CancellationSource::default().token(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Timeout);
    fixture.assert_cleanup();
    assert_eq!(
        error.raw_response().expect("backend response").as_str(),
        payload(),
        "candidate evidence regression"
    );
    assert_eq!(
        error.diagnostics().stdout(),
        capture("missing-terminal.jsonl"),
        "diagnostic evidence regression"
    );
    assert_eq!(error.diagnostics().stderr(), b"diagnostic\xff\r\n");
    fixture.assert_cleanup();
}

#[test]
fn failed_auth_exit_is_unavailable_even_with_a_success_looking_status_on_stdout() {
    let fixture = Fixture::new(&[], 0);
    fixture.update("auth_stdout", json!(true));
    fixture.update("auth_exit_code", json!(1));
    let error = CodexCliBackend::connect(
        fixture.config.clone(),
        &CancellationSource::default().token(),
    )
    .unwrap_err();
    assert!(matches!(error, BackendError::Unavailable { .. }), "{error}");
    assert_eq!(evidence(&error).1.stdout(), b"Logged in using ChatGPT\n");
    assert!(!fixture.report().exists());
}
