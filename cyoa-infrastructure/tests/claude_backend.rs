//! Actual Claude adapter, controlled fixture executable, real supervisor. No auth/network.
#![cfg(target_os = "linux")]
use cyoa_application::{cancellation::CancellationSource, diagnostics::TransportDiagnostics};
use cyoa_infrastructure::{
    backend::{Backend, BackendError, GenerationRequest},
    backends::{
        claude_cli::{ClaudeCliBackend, ClaudeExecutable, ClaudeInvocationConfig},
        process::{MaxStderrBytes, MaxStdoutBytes, ProcessBounds},
    },
    generation::backend_compat::claude_cli::{ADVISOR_SUPPRESSION, adapt_schema},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

const STATUS: &str = r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","subscriptionType":"team","email":"someone@example.invalid"}"#;

struct Fixture {
    home: tempfile::TempDir,
    config: ClaudeInvocationConfig,
}
impl Fixture {
    fn new(stdout: &[u8], exit: i32) -> Self {
        let home = tempfile::tempdir().unwrap();
        let exe = ClaudeExecutable::resolve(
            env!("CARGO_BIN_EXE_subprocess_fixture").as_ref(),
            "/bin".as_ref(),
            home.path(),
        )
        .unwrap();
        let config = ClaudeInvocationConfig::new(
            exe,
            home.path().into(),
            "/bin".into(),
            Some(home.path().join("auth")),
            None,
        )
        .unwrap()
        .with_bounds(bounds(3000, 1_048_576));
        let fixture = Self { home, config };
        fixture.set(json!({
            "stdout": [{"bytes": stdout}],
            "stderr": [{"bytes": b"diagnostic\xff\r\n"}],
            "drain_stdin": true,
            "report_path": fixture.report(),
            "auth_status": STATUS,
            "auth_stdout": true,
            "exit_code": exit
        }));
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
    fn backend(&self) -> ClaudeCliBackend {
        ClaudeCliBackend::connect(self.config.clone(), &CancellationSource::default().token())
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
    fn launches(&self) -> Vec<String> {
        fs::read_to_string(self.home.path().join("launches.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| {
                serde_json::from_str::<Value>(l).unwrap()["command"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
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
fn review() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reviews/2026-10-02-claude-step1")
}
fn capture(name: &str) -> Vec<u8> {
    fs::read(review().join(format!("synthetic/{name}.jsonl"))).unwrap()
}
fn expectations() -> Value {
    serde_json::from_slice(&fs::read(review().join("synthetic/expectations.json")).unwrap())
        .unwrap()
}
fn payload() -> String {
    expectations()["_payload"].as_str().unwrap().to_owned()
}
fn previews() -> Vec<String> {
    expectations()["_preview"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
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

// A complete-only stream: init then a result, with no stream events at all.
fn complete_only(payload: &str) -> Vec<u8> {
    format!(
        "{}\n{}\n",
        r#"{"type":"system","subtype":"init","apiKeySource":"none","model":"claude-sonnet-5-5"}"#,
        format_args!(
            r#"{{"type":"result","subtype":"success","is_error":false,"structured_output":{payload}}}"#
        )
    )
    .into_bytes()
}

#[test]
fn result_cannot_override_nonzero_exit_and_previews_are_tentative_evidence() {
    let stdout = capture("success-minimal");
    let fixture = Fixture::new(&stdout, 7);
    let mut seen = vec![];
    let error = fixture
        .backend()
        .generate(
            request(&json!({})),
            &CancellationSource::default().token(),
            &mut |s| seen.push(s.to_owned()),
        )
        .expect_err("process failure override regression");
    assert!(matches!(error, BackendError::Generation { .. }), "{error}");
    assert!(error.to_string().contains('7'));
    assert_eq!(evidence(&error).0, payload(), "the candidate is retained");
    assert_eq!(evidence(&error).1.stdout(), stdout);
    assert_eq!(evidence(&error).1.stderr(), b"diagnostic\xff\r\n");
    assert_eq!(seen, previews(), "previews preceded the failure");
    fixture.assert_cleanup();
}

#[test]
fn protocol_rejections_keep_candidate_evidence_and_clean_up() {
    let expectations = expectations();
    for name in [
        "is-error-true-with-payload",
        "subtype-not-success",
        "duplicate-result-identical",
        "record-after-result",
        "missing-result",
        "truncated-final-record",
        "result-without-structured-output",
        "structured-output-duplicate-keys",
        "two-payload-blocks-same-message",
        "metered-api-key-source",
        "unknown-top-level-type",
    ] {
        let fixture = Fixture::new(&capture(name), 0);
        let error = fixture
            .backend()
            .generate(
                request(&json!({})),
                &CancellationSource::default().token(),
                &mut |_| {},
            )
            .unwrap_err();
        fixture.assert_cleanup();
        assert!(
            matches!(error, BackendError::Generation { .. }),
            "{name}: {error}"
        );
        let want = &expectations["variants"][name];
        let retained = match want["candidate"].as_str().unwrap() {
            "retained" if name == "structured-output-duplicate-keys" => {
                r#"{"narrative":"a","narrative":"b"}"#.to_owned()
            }
            "retained" => payload(),
            _ => String::new(),
        };
        assert_eq!(evidence(&error).0, retained, "{name}");
        assert_eq!(
            evidence(&error).1.stdout(),
            capture(name).as_slice(),
            "{name}"
        );
    }
}

// Cleanup is checked before the behavioral assertion so a surviving protocol
// mutant cannot hide a leaked child or workspace behind an early panic.
fn assert_protocol_rejected(name: &str) {
    let fixture = Fixture::new(&capture(name), 0);
    let result = fixture.backend().generate(
        request(&json!({})),
        &CancellationSource::default().token(),
        &mut |_| {},
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
}

#[test]
fn is_error_result_is_rejected_after_cleanup() {
    assert_protocol_rejected("is-error-true-with-payload");
}

#[test]
fn stream_without_a_result_is_rejected_after_cleanup() {
    assert_protocol_rejected("missing-result");
}

#[test]
fn duplicate_result_is_rejected_after_cleanup() {
    assert_protocol_rejected("duplicate-result-identical");
}

#[test]
fn ambiguous_payload_blocks_are_rejected_after_cleanup() {
    assert_protocol_rejected("two-payload-blocks-same-message");
}

#[test]
fn preflight_rejects_non_subscription_unknown_and_conflicting_auth_without_generation() {
    for status in [
        r#"{"loggedIn":false,"authMethod":"none","apiProvider":"firstParty"}"#,
        r#"{"loggedIn":true,"authMethod":"api_key","apiProvider":"firstParty"}"#,
        r#"{"loggedIn":true,"authMethod":"oauth_token","apiProvider":"firstParty"}"#,
        r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"bedrock"}"#,
        r#"{"loggedIn":"true","authMethod":"claude.ai","apiProvider":"firstParty"}"#,
        r#"{"authMethod":"claude.ai","apiProvider":"firstParty"}"#,
        r#"{}"#,
        "Not logged in",
        "",
    ] {
        let fixture = Fixture::new(&capture("success-minimal"), 0);
        fixture.update("auth_status", json!(status));
        let error = ClaudeCliBackend::connect(
            fixture.config.clone(),
            &CancellationSource::default().token(),
        )
        .expect_err(&format!("auth acceptance regression: {status}"));
        assert!(
            matches!(error, BackendError::Unavailable { .. }),
            "{status}"
        );
        assert!(!fixture.report().exists(), "{status}: no generation child");
        assert_eq!(evidence(&error).1.stdout(), status.as_bytes());
        assert_eq!(fixture.launches(), ["auth"]);
    }
}

#[test]
fn failed_auth_exit_is_unavailable_even_with_a_success_looking_status_on_stdout() {
    let fixture = Fixture::new(&[], 0);
    fixture.update("auth_exit_code", json!(1));
    let error = ClaudeCliBackend::connect(
        fixture.config.clone(),
        &CancellationSource::default().token(),
    )
    .unwrap_err();
    assert!(matches!(error, BackendError::Unavailable { .. }), "{error}");
    assert_eq!(evidence(&error).1.stdout(), STATUS.as_bytes());
    assert!(!fixture.report().exists());
}

#[test]
fn timeout_after_result_retains_unemitted_candidate_and_reaps_child() {
    let mut fixture = Fixture::new(&complete_only(&payload()), 0);
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
    assert!(matches!(error, BackendError::Timeout { .. }), "{error}");
    assert_eq!(evidence(&error).0, payload());
    assert_eq!(seen, 0, "a candidate is not emitted before reconciliation");
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
        ClaudeCliBackend::connect(fixture.config.clone(), &token),
        Err(BackendError::Cancelled { .. })
    ));
}

#[test]
fn cancellation_from_a_preview_stops_the_live_child_and_keeps_the_evidence() {
    let fixture = Fixture::new(&capture("success-minimal"), 0);
    fixture.update("hang_after_output_ms", json!(60000));
    let source = CancellationSource::default();
    let started = Instant::now();
    let mut seen = vec![];
    let error = fixture
        .backend()
        .generate(request(&json!({})), &source.token(), &mut |fragment| {
            seen.push(fragment.to_owned());
            source.cancel();
        })
        .unwrap_err();
    assert!(matches!(error, BackendError::Cancelled { .. }), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "child was killed, not awaited (it would run 60 s)"
    );
    assert_eq!(seen.first(), previews().first());
    assert!(!evidence(&error).1.stdout().is_empty());
    fixture.assert_cleanup();
}

#[test]
fn capped_output_preserves_prefix_metadata_and_a_structured_cause() {
    use cyoa_application::diagnostics::CaptureCompleteness;
    use cyoa_infrastructure::backends::process::SupervisorError;
    let candidate = capture("success-minimal");
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
            &mut |_| {},
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
    assert!(diagnostics.stdout().starts_with(&candidate));
    fixture.assert_cleanup();
}

#[test]
fn successful_response_forwards_previews_then_returns_exact_payload_usage_and_provenance() {
    let stdout = capture("success-minimal");
    let fixture = Fixture::new(&stdout, 0);
    let mut backend = fixture.backend();
    let mut emitted = vec![];
    let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"narrative":{"type":"string"}},"additionalProperties":false});
    let response = backend
        .generate(
            request(&schema),
            &CancellationSource::default().token(),
            &mut |text| emitted.push(text.to_owned()),
        )
        .unwrap();
    assert_eq!(
        emitted,
        previews(),
        "previews only; the final payload is not re-emitted"
    );
    assert_eq!(response.raw_response(), payload());
    assert_eq!(response.diagnostics().stdout(), stdout);
    assert_eq!(response.diagnostics().stderr(), b"diagnostic\xff\r\n");
    let input = response.usage().input.unwrap();
    assert_eq!(
        (input.total(), input.cached(), response.usage().output),
        (17, Some(5), Some(7))
    );
    let provenance = response.provenance();
    assert_eq!(
        provenance.model.as_ref().unwrap().as_str(),
        "claude-sonnet-5-5"
    );
    assert_eq!(provenance.provider.as_ref().unwrap().as_str(), "firstParty");
    assert_eq!(provenance.cost.as_ref().unwrap().amount().get(), 0.0125);
    let report = fixture.assert_cleanup();
    let adapted = serde_json::to_string(&adapt_schema(schema.clone())).unwrap();
    let expected_args = [
        "-p",
        "--safe-mode",
        "--tools",
        "",
        "--permission-prompts",
        "none",
        "--no-session-persistence",
        "--system-prompt",
        request(&schema).instructions,
        "--append-system-prompt",
        ADVISOR_SUPPRESSION,
        "--json-schema",
        &adapted,
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
    ];
    let argv: Vec<String> = serde_json::from_value(report["argv"].clone()).unwrap();
    assert_eq!(&argv[1..], &expected_args);
    let stdin: Vec<u8> = serde_json::from_value(report["stdin"].clone()).unwrap();
    assert_eq!(stdin, request(&schema).prompt.as_bytes());
    assert!(
        report["schema"].is_null(),
        "no schema file: it travels in argv"
    );
    assert_eq!(
        report["env_keys"],
        json!(["CLAUDE_CONFIG_DIR", "HOME", "PATH"])
    );
    assert_eq!(
        fixture.launches(),
        ["auth", "-p"],
        "one preflight and exactly one generation child"
    );
}

#[test]
fn complete_only_stream_emits_the_payload_once_after_cleanup() {
    let stdout = complete_only(r#"{"narrative":"All at once"}"#);
    let fixture = Fixture::new(&stdout, 0);
    let mut emitted = vec![];
    let response = fixture
        .backend()
        .generate(
            request(&json!({})),
            &CancellationSource::default().token(),
            &mut |text| {
                fixture.assert_cleanup();
                emitted.push(text.to_owned());
            },
        )
        .unwrap();
    assert_eq!(emitted, [r#"{"narrative":"All at once"}"#]);
    assert_eq!(response.raw_response(), r#"{"narrative":"All at once"}"#);
}

#[test]
fn final_payload_wins_when_it_disagrees_with_the_preview() {
    let line = |text: &str| format!("{text}\n");
    let mut stdout = String::new();
    for record in [
        r#"{"type":"system","subtype":"init","apiKeySource":"none","model":"claude-sonnet-5-5"}"#,
        r#"{"type":"stream_event","event":{"type":"message_start"}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","name":"StructuredOutput"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"narrative\": \"WRONG\"}"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"structured_output":{"narrative":"RIGHT"}}"#,
    ] {
        stdout.push_str(&line(record));
    }
    let fixture = Fixture::new(stdout.as_bytes(), 0);
    let mut emitted = vec![];
    let response = fixture
        .backend()
        .generate(
            request(&json!({})),
            &CancellationSource::default().token(),
            &mut |text| emitted.push(text.to_owned()),
        )
        .unwrap();
    assert_eq!(emitted, [r#"{"narrative": "WRONG"}"#]);
    assert_eq!(response.raw_response(), r#"{"narrative":"RIGHT"}"#);
    assert_eq!(response.value()["narrative"], "RIGHT");
    fixture.assert_cleanup();
}

#[test]
fn cancellation_from_the_complete_callback_cannot_return_success() {
    let fixture = Fixture::new(&complete_only(&payload()), 0);
    let source = CancellationSource::default();
    let mut seen = 0;
    let error = fixture
        .backend()
        .generate(request(&json!({})), &source.token(), &mut |_| {
            seen += 1;
            source.cancel();
        })
        .unwrap_err();
    assert!(matches!(error, BackendError::Cancelled { .. }), "{error}");
    assert_eq!(evidence(&error).0, payload());
    assert_eq!(seen, 1);
    fixture.assert_cleanup();
}

#[test]
fn preparation_failure_and_removed_executable_never_launch_a_generation() {
    let fixture = Fixture::new(&[], 0);
    let mut backend = fixture.backend();
    let oversize = "x".repeat(131_072);
    let error = backend
        .generate(
            GenerationRequest {
                instructions: &oversize,
                prompt: "p",
                schema: &json!({}),
            },
            &CancellationSource::default().token(),
            &mut |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, BackendError::Unavailable { .. }), "{error}");
    assert!(error.to_string().contains("--system-prompt"), "{error}");
    assert!(!fixture.report().exists());
    assert_eq!(fixture.launches(), ["auth"], "oversize argv never launches");
    // Resolve a real executable, then remove that temporary copy after auth.
    let copied = fixture.home.path().join("claude-fixture");
    fs::copy(env!("CARGO_BIN_EXE_subprocess_fixture"), &copied).unwrap();
    let exe = ClaudeExecutable::resolve(copied.as_os_str(), "/bin".as_ref(), fixture.home.path())
        .unwrap();
    let config =
        ClaudeInvocationConfig::new(exe, fixture.home.path().into(), "/bin".into(), None, None)
            .unwrap();
    let mut backend =
        ClaudeCliBackend::connect(config, &CancellationSource::default().token()).unwrap();
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
fn application_receives_candidate_and_diagnostics_on_adapter_timeout() {
    use cyoa_application::generation::{FailureKind, StoryUseCases};
    use cyoa_core::text::Brief;
    use cyoa_infrastructure::generation::{
        engine::GenerationEngine, templates::GenerationTemplates,
    };
    let stdout = complete_only(&payload());
    let mut fixture = Fixture::new(&stdout, 0);
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
        stdout,
        "diagnostic evidence regression"
    );
    assert_eq!(error.diagnostics().stderr(), b"diagnostic\xff\r\n");
}

#[test]
fn malformed_result_metadata_keeps_the_candidate_visible_to_the_application() {
    let payload = r#"{"narrative":"kept","n":1}"#;
    let init =
        r#"{"type":"system","subtype":"init","apiKeySource":"none","model":"claude-sonnet-5-5"}"#;
    // Valid payload, but the terminal metadata is malformed (no is_error).
    let result =
        format!(r#"{{"type":"result","subtype":"success","structured_output":{payload}}}"#);
    let stdout = format!("{init}\n{result}\n").into_bytes();
    let fixture = Fixture::new(&stdout, 0);
    let outcome = fixture.backend().generate(
        request(&json!({})),
        &CancellationSource::default().token(),
        &mut |_| {},
    );
    fixture.assert_cleanup();
    let error = outcome.expect_err("malformed terminal metadata must not succeed");
    assert!(matches!(error, BackendError::Generation { .. }), "{error}");
    assert_eq!(
        evidence(&error).0,
        payload,
        "metadata candidate regression: the separate candidate field was lost"
    );
    assert_eq!(evidence(&error).1.stdout(), stdout);
}

#[test]
fn ambiguous_control_fields_cannot_authorize_success_at_the_real_boundary() {
    let init =
        r#"{"type":"system","subtype":"init","apiKeySource":"none","model":"claude-sonnet-5-5"}"#;
    let metered = r#"{"type":"system","subtype":"init","apiKeySource":"ANTHROPIC_API_KEY","apiKeySource":"none"}"#;
    let payload = r#"{"narrative":"kept"}"#;
    let ambiguous_result = format!(
        r#"{{"type":"result","subtype":"success","is_error":true,"is_error":false,"structured_output":{payload}}}"#
    );
    for (label, stdout, retained) in [
        (
            "duplicate is_error",
            format!("{init}\n{ambiguous_result}\n"),
            payload,
        ),
        ("duplicate apiKeySource", format!("{metered}\n"), ""),
    ] {
        let fixture = Fixture::new(stdout.as_bytes(), 0);
        let outcome = fixture.backend().generate(
            request(&json!({})),
            &CancellationSource::default().token(),
            &mut |_| {},
        );
        fixture.assert_cleanup();
        let error = outcome.expect_err(&format!("ambiguous result regression: {label}"));
        assert!(
            matches!(error, BackendError::Generation { .. }),
            "{label}: {error}"
        );
        assert_eq!(evidence(&error).0, retained, "{label}");
        assert_eq!(evidence(&error).1.stdout(), stdout.as_bytes(), "{label}");
    }
}
