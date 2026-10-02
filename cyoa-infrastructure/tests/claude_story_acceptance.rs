//! Composed real-adapter acceptance for Claude. Frozen synthetic story (shared with
//! the Codex acceptance test), actual fixture children and supervisor; no installed
//! Claude, credentials or network. The Claude-shaped streams are built here from
//! hand-authored payloads; the helper does not call the production codec and is not
//! an oracle for story semantics.
#![cfg(target_os = "linux")]
use cyoa_application::{cancellation::CancellationSource, generation::*};
use cyoa_core::{
    game::{GameState, TurnCount},
    ids::CharacterId,
    limits::{
        Limits, MajorEventLimit, MaxGeneratedNpcs, MinPlayableCharacters, ProseBridgeTurns,
        RestoreLimits,
    },
    style::StoryStyle,
    text::{Brief, PlayerInput, WorldDescription},
    turn::QuickActionKind,
    world::{PlayablePosition, WorldOutline},
};
use cyoa_infrastructure::{
    backends::{
        claude_cli::{ClaudeCliBackend, ClaudeExecutable, ClaudeInvocationConfig},
        process::{MaxStderrBytes, MaxStdoutBytes, ProcessBounds},
    },
    generation::{
        backend_compat::claude_cli::ADVISOR_SUPPRESSION, engine::GenerationEngine,
        templates::GenerationTemplates,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

type Cases = StoryUseCases<GenerationEngine<ClaudeCliBackend>>;
const STDERR: &[u8] = b"fixture diagnostics\xff\r\n";
const STATUS: &str = r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty"}"#;
const STDOUT_CAP: usize = 65536;

fn data() -> Value {
    serde_json::from_str(include_str!("fixtures/codex/story.json")).unwrap()
}
fn turn(index: usize) -> String {
    data()["turns"][index].to_string()
}

/// Splits at char boundaries into small fragments, so previews arrive incrementally.
fn fragments(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut current = String::new();
    for c in text.chars() {
        current.push(c);
        if current.chars().count() == 24 {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

struct Shape<'a> {
    /// What the payload block streams (defaults to the final payload).
    preview: Option<&'a str>,
    /// Stream any previews at all.
    previews: bool,
    /// Prose in a text block of message 1, the CLI's enforce prompt, then the
    /// payload in message 2, as seen in a live capture.
    enforce_retry: bool,
}
const PLAIN: Shape<'static> = Shape {
    preview: None,
    previews: true,
    enforce_retry: false,
};

fn record(value: Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(&value).unwrap();
    line.extend_from_slice(b"\r\n");
    line
}

fn transcript_with(raw: &str, terminal: &str, shape: &Shape) -> Vec<u8> {
    let event = |event: Value| json!({"type":"stream_event","event":event});
    let mut out = vec![];
    for line in [
        json!({"type":"system","subtype":"init","apiKeySource":"none","model":"claude-sonnet-5-5"}),
        json!({"type":"system","subtype":"status","status":"requesting"}),
        event(json!({"type":"message_start"})),
    ] {
        out.extend(record(line));
    }
    let payload_index = if shape.enforce_retry {
        for line in [
            event(json!({"type":"content_block_start","index":0,"content_block":{"type":"text"}})),
            event(
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"WRONG PROSE: a story told as plain text"}}),
            ),
            event(json!({"type":"content_block_stop","index":0})),
            event(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}})),
            event(json!({"type":"message_stop"})),
            json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":"[structured-output-enforce] You MUST call the StructuredOutput tool to complete this request. Call this tool now."}]}}),
            event(json!({"type":"message_start"})),
        ] {
            out.extend(record(line));
        }
        0
    } else {
        for line in [
            event(
                json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking"}}),
            ),
            event(json!({"type":"content_block_stop","index":0})),
        ] {
            out.extend(record(line));
        }
        1
    };
    out.extend(record(event(
        json!({"type":"content_block_start","index":payload_index,
        "content_block":{"type":"tool_use","name":"StructuredOutput"}}),
    )));
    if shape.previews {
        out.extend(record(event(
            json!({"type":"content_block_delta","index":payload_index,
            "delta":{"type":"input_json_delta","partial_json":""}}),
        )));
        for fragment in fragments(shape.preview.unwrap_or(raw)) {
            out.extend(record(event(
                json!({"type":"content_block_delta","index":payload_index,
                "delta":{"type":"input_json_delta","partial_json":fragment}}),
            )));
        }
    }
    for line in [
        event(json!({"type":"content_block_stop","index":payload_index})),
        event(json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}})),
        event(json!({"type":"message_stop"})),
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"Structured output provided successfully"}]}}),
        json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}),
    ] {
        out.extend(record(line));
    }
    // The payload is spliced as text so the span reaching the codec is exactly `raw`.
    let tail = r#""usage":{"input_tokens":3,"cache_creation_input_tokens":40,"cache_read_input_tokens":7,"output_tokens":11},"total_cost_usd":0.01,"modelUsage":{"claude-sonnet-5-5":{"costBasis":"list","provider":"firstParty"}}"#;
    let result = |is_error: bool| {
        format!(
            r#"{{"type":"result","subtype":"success","is_error":{is_error},"structured_output":{raw},{tail}}}"#
        )
    };
    match terminal {
        "completed" => out.extend_from_slice(format!("{}\r\n", result(false)).as_bytes()),
        "is_error" => out.extend_from_slice(format!("{}\r\n", result(true)).as_bytes()),
        "truncated" => {
            let line = result(false);
            out.extend_from_slice(&line.as_bytes()[..line.len() - 20]);
        }
        "missing" => (),
        _ => panic!("unknown synthetic terminal"),
    }
    out
}
fn transcript(raw: &str, terminal: &str) -> Vec<u8> {
    transcript_with(raw, terminal, &PLAIN)
}

#[derive(Debug)]
struct CapturedRequest {
    prompt: String,
    instructions: String,
    schema: Value,
}
struct Fixture {
    home: tempfile::TempDir,
    captures: Vec<CapturedRequest>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Backstop for an assertion panic in the cleanup-failure test. The
        // normal capture path asserts both the obstruction and its removal.
        if let Ok(bytes) = fs::read(self.report_path())
            && let Ok(report) = serde_json::from_slice::<Value>(&bytes)
            && let Some(cwd) = report["cwd"].as_str()
            && fs::read(cwd).is_ok_and(|b| b == b"fixture cleanup obstruction")
        {
            let _ = fs::remove_file(cwd);
        }
    }
}
impl Fixture {
    fn new() -> (Self, Cases) {
        let home = tempfile::tempdir().unwrap();
        let fixture = Self {
            home,
            captures: vec![],
        };
        fixture.respond(&data()["outline"].to_string());
        let executable = ClaudeExecutable::resolve(
            env!("CARGO_BIN_EXE_subprocess_fixture").as_ref(),
            "/bin".as_ref(),
            fixture.home.path(),
        )
        .unwrap();
        let config = ClaudeInvocationConfig::new(
            executable,
            fixture.home.path().into(),
            "/bin".into(),
            None,
            None,
        )
        .unwrap()
        .with_bounds(
            ProcessBounds::new(
                Duration::from_secs(5),
                MaxStdoutBytes::new(STDOUT_CAP).unwrap(),
                MaxStderrBytes::new(8192).unwrap(),
            )
            .unwrap(),
        );
        let backend =
            ClaudeCliBackend::connect(config, &CancellationSource::default().token()).unwrap();
        (
            fixture,
            StoryUseCases::new(GenerationEngine::new(
                backend,
                GenerationTemplates::bundled().unwrap(),
            )),
        )
    }
    fn report_path(&self) -> PathBuf {
        self.home.path().join("report.json")
    }
    fn scenario(&self, stdout: &[u8], exit: i32, extra: Value) {
        if self.report_path().exists() {
            fs::remove_file(self.report_path()).unwrap();
        }
        let mut scenario = json!({"stdout":[{"bytes":stdout}],"stderr":[{"bytes":STDERR}],"drain_stdin":true,"exit_code":exit,"report_path":self.report_path(),"auth_status":STATUS,"auth_stdout":true});
        scenario
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        fs::write(
            self.home.path().join("cyoa-fixture.json"),
            serde_json::to_vec(&scenario).unwrap(),
        )
        .unwrap();
    }
    fn respond(&self, raw: &str) {
        self.scenario(&transcript(raw, "completed"), 0, json!({}));
    }
    fn capture(&mut self, cleanup_failed: bool) {
        let report: Value = serde_json::from_slice(&fs::read(self.report_path()).unwrap()).unwrap();
        let pid = rustix::process::Pid::from_raw(report["pid"].as_i64().unwrap() as i32).unwrap();
        assert_eq!(
            rustix::process::test_kill_process(pid),
            Err(rustix::io::Errno::SRCH)
        );
        let cwd = PathBuf::from(report["cwd"].as_str().unwrap());
        if cleanup_failed {
            assert!(
                cwd.is_file(),
                "fixture deliberately replaced only its request directory"
            );
            assert_eq!(fs::read(&cwd).unwrap(), b"fixture cleanup obstruction");
            fs::remove_file(&cwd).unwrap();
        }
        assert!(!cwd.exists(), "request workspace survives");
        // Read the request from the actual child: argv carries instructions and
        // schema, stdin carries the prompt.
        let argv: Vec<String> = serde_json::from_value(report["argv"].clone()).unwrap();
        let value_after = |flag: &str| {
            let at = argv.iter().position(|a| a == flag).expect(flag);
            argv[at + 1].clone()
        };
        assert_eq!(value_after("--append-system-prompt"), ADVISOR_SUPPRESSION);
        assert!(!argv.iter().any(|a| a == "--bare" || a == "--model"));
        let stdin: Vec<u8> = serde_json::from_value(report["stdin"].clone()).unwrap();
        self.captures.push(CapturedRequest {
            prompt: String::from_utf8(stdin).unwrap(),
            instructions: value_after("--system-prompt"),
            schema: serde_json::from_str(&value_after("--json-schema")).unwrap(),
        });
    }
    fn assert_calls(&self, generations: usize) {
        let log = fs::read_to_string(self.home.path().join("launches.jsonl")).unwrap();
        let calls: Vec<Value> = log
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(calls.iter().filter(|c| c["command"] == "auth").count(), 1);
        assert_eq!(
            calls.iter().filter(|c| c["command"] == "-p").count(),
            generations
        );
        assert_eq!(calls.len(), generations + 1);
    }
}
fn start(limits: Limits) -> (Fixture, Cases, GameState) {
    let (mut fixture, mut cases) = Fixture::new();
    let source = CancellationSource::default();
    let brief = Brief::new("Two Ajaxes, a harbour and a voyage").unwrap();
    let outline = cases.generate_outline(&brief, &source.token()).unwrap();
    fixture.capture(false);
    assert_eq!(outline.value().title().as_str(), "Harbour");
    let edited = WorldOutline::new(
        outline.value().title().clone(),
        WorldDescription::new("Edited harbour with a locked gate").unwrap(),
    );
    fixture.respond(&data()["cast"].to_string());
    let world = cases
        .generate_world(&brief, edited, &limits, &source.token())
        .unwrap();
    fixture.capture(false);
    assert!(
        fixture.captures[1]
            .prompt
            .contains("Edited harbour with a locked gate")
    );
    assert!(!fixture.captures[1].prompt.contains("A sheltered harbour"));
    assert_eq!(world.cast().playable().len(), 2);
    assert!(world.clone().select(PlayablePosition::new(2)).is_err());
    let game = GameState::start(
        brief,
        world.select(PlayablePosition::new(1)).unwrap(),
        StoryStyle::default(),
        limits,
    );
    assert_eq!(game.protagonist().description().as_str(), "A mason");
    fixture.assert_calls(2);
    (fixture, cases, game)
}
fn advance(
    fixture: &mut Fixture,
    cases: &mut Cases,
    game: &mut GameState,
    index: usize,
    direction: TurnDirection,
) {
    let raw = turn(index);
    fixture.respond(&raw);
    let mut previews = vec![];
    cases
        .take_turn(
            game,
            direction,
            &CancellationSource::default().token(),
            &mut |s| previews.push(s.to_owned()),
        )
        .unwrap();
    fixture.capture(false);
    assert_eq!(
        previews.concat(),
        data()["turns"][index]["narrative"].as_str().unwrap()
    );
    assert!(previews.len() > 1, "previews arrive incrementally");
    let record = game.turns().last().unwrap();
    assert_eq!(record.raw_response().as_str(), raw, "exact result span");
    // Observed provenance only: what the stream reported, nothing configured.
    let provenance = record.provenance();
    assert_eq!(
        provenance.model.as_ref().unwrap().as_str(),
        "claude-sonnet-5-5"
    );
    assert_eq!(provenance.provider.as_ref().unwrap().as_str(), "firstParty");
    assert_eq!(provenance.cost.as_ref().unwrap().amount().get(), 0.01);
}
fn events(game: &GameState) -> Vec<String> {
    game.current_summary()
        .major_events()
        .events()
        .as_slice()
        .iter()
        .map(|s| s.as_str().into())
        .collect()
}
fn upcoming(game: &GameState) -> Vec<String> {
    game.current_summary()
        .upcoming_events()
        .as_slice()
        .iter()
        .map(|s| s.as_str().into())
        .collect()
}
fn limits(cap: usize) -> Limits {
    Limits {
        max_major_events: MajorEventLimit::new(cap).unwrap(),
        ..Limits::default()
    }
}

#[test]
fn late_transport_protocol_and_domain_failures_leave_state_equal_and_retry_once() {
    let (mut fixture, mut cases, mut game) = start(limits(2));
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        0,
        TurnDirection::Continue,
    );
    let mut invalid = data()["turns"][1].clone();
    invalid["narrative"] = json!("  ");
    // (payload, terminal, exit, kind, candidate retained as raw response)
    for (raw, terminal, exit, kind, retained) in [
        (turn(1), "completed", 7, FailureKind::Transport, true),
        (turn(1), "is_error", 0, FailureKind::Transport, true),
        (turn(1), "truncated", 0, FailureKind::Transport, false),
        (
            invalid.to_string(),
            "completed",
            0,
            FailureKind::InvalidResponse,
            true,
        ),
        (turn(1), "missing", 0, FailureKind::Transport, false),
    ] {
        let before = game.clone();
        let stdout = transcript(&raw, terminal);
        fixture.scenario(&stdout, exit, json!({}));
        let mut preview = String::new();
        let error = cases
            .take_turn(
                &mut game,
                TurnDirection::Continue,
                &CancellationSource::default().token(),
                &mut |s| preview.push_str(s),
            )
            .unwrap_err();
        assert_eq!(error.kind(), kind, "{terminal}");
        assert_eq!(game, before, "{terminal}");
        assert_eq!(
            error.raw_response().as_str(),
            if retained { raw.as_str() } else { "" },
            "{terminal}: candidate evidence"
        );
        // Previews are tentative and may have reached the caller before the
        // failure, but never become a committed turn.
        let narrative = serde_json::from_str::<Value>(&raw).unwrap()["narrative"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(narrative.starts_with(&preview), "{terminal}: {preview:?}");
        assert_eq!(error.diagnostics().stdout(), stdout, "{terminal}");
        // The child is killed at a codec rejection, which can precede its stderr.
        if terminal != "is_error" {
            assert_eq!(error.diagnostics().stderr(), STDERR, "{terminal}");
        }
        fixture.capture(false);
        let failed = fixture.captures.len() - 1;
        fixture.assert_calls(failed + 1);
        advance(
            &mut fixture,
            &mut cases,
            &mut game,
            1,
            TurnDirection::Continue,
        );
        assert_eq!(game.turns().len(), before.turns().len() + 1);
        assert_eq!(
            fixture.captures[failed].prompt,
            fixture.captures[failed + 1].prompt
        );
        assert_eq!(
            fixture.captures[failed].instructions,
            fixture.captures[failed + 1].instructions
        );
        fixture.assert_calls(failed + 2);
    }
    fixture.assert_calls(13);
}

#[test]
fn real_workspace_cleanup_failure_preserves_the_game_and_candidate() {
    let (mut fixture, mut cases, mut game) = start(limits(2));
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        0,
        TurnDirection::Continue,
    );
    let before = game.clone();
    let raw = turn(1);
    fixture.scenario(
        &transcript(&raw, "completed"),
        0,
        json!({"replace_workspace_with_file":true}),
    );
    let mut previews = String::new();
    let error = cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |s| previews.push_str(s),
        )
        .unwrap_err();
    assert_eq!(game, before);
    assert_eq!(error.kind(), FailureKind::Transport);
    assert!(error.to_string().contains("WorkspaceCleanup"));
    assert_eq!(error.raw_response().as_str(), raw);
    assert_eq!(error.diagnostics().stdout(), transcript(&raw, "completed"));
    assert_eq!(error.diagnostics().stderr(), STDERR);
    fixture.capture(true);
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        1,
        TurnDirection::Continue,
    );
    assert_eq!(fixture.captures[3].prompt, fixture.captures[4].prompt);
    fixture.assert_calls(5);
}

#[test]
fn cancellation_and_output_cap_preserve_state_context_and_explicit_retry() {
    let (mut fixture, mut cases, mut game) = start(limits(2));
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        0,
        TurnDirection::Continue,
    );
    let before = game.clone();
    fixture.scenario(&[], 0, json!({"hang_ms":10000}));
    let source = CancellationSource::default();
    let token = source.token();
    let report = fixture.report_path();
    let canceller = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !report.exists() {
            if Instant::now() >= deadline {
                source.cancel();
                panic!("child-start handshake missing");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        source.cancel();
    });
    let error = cases
        .take_turn(&mut game, TurnDirection::Continue, &token, &mut |_| {
            panic!("silent child cannot preview")
        })
        .unwrap_err();
    canceller.join().unwrap();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert_eq!(game, before);
    assert_eq!(error.raw_response().as_str(), "");
    fixture.capture(false);
    fixture.assert_calls(4);
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        1,
        TurnDirection::Continue,
    );
    assert_eq!(fixture.captures[3].prompt, fixture.captures[4].prompt);

    // Cancel from the first live preview while the child is still running: the
    // application must not commit and the child must be killed, not awaited.
    let before = game.clone();
    fixture.scenario(
        &transcript(&turn(2), "completed"),
        0,
        json!({"hang_after_output_ms":10000}),
    );
    let source = CancellationSource::default();
    let started = Instant::now();
    let mut previews = vec![];
    let error = cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source.token(),
            &mut |s| {
                previews.push(s.to_owned());
                source.cancel();
            },
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(game, before);
    assert!(!previews.is_empty());
    assert!("Voyage: the ship sailed.".starts_with(&previews.concat()));
    assert!(turn(2).starts_with(error.raw_response().as_str()));
    fixture.capture(false);
    fixture.assert_calls(6);
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        2,
        TurnDirection::Continue,
    );
    assert_eq!(fixture.captures[5].prompt, fixture.captures[6].prompt);

    // A stream with no previews is complete-only: cancelling from the one prose
    // callback of the accepted candidate must not commit either.
    let before = game.clone();
    fixture.scenario(
        &transcript_with(
            &turn(3),
            "completed",
            &Shape {
                previews: false,
                ..PLAIN
            },
        ),
        0,
        json!({}),
    );
    let source = CancellationSource::default();
    let mut previews = vec![];
    let error = cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source.token(),
            &mut |s| {
                previews.push(s.to_owned());
                source.cancel();
            },
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert_eq!(game, before);
    assert_eq!(previews.len(), 1);
    assert_eq!(error.raw_response().as_str(), turn(3));
    fixture.capture(false);
    fixture.assert_calls(8);
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        3,
        TurnDirection::Continue,
    );
    assert_eq!(fixture.captures[7].prompt, fixture.captures[8].prompt);

    let before = game.clone();
    let mut capped = transcript(&turn(4), "completed");
    capped.extend_from_slice(&vec![b'x'; STDOUT_CAP + 8000]);
    fixture.scenario(&capped, 0, json!({}));
    let error = cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |_| {},
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Transport);
    assert_eq!(game, before);
    assert_eq!(error.diagnostics().stdout(), &capped[..STDOUT_CAP]);
    assert_eq!(
        error.diagnostics().stdout_capture().completeness(),
        cyoa_application::diagnostics::CaptureCompleteness::Prefix
    );
    fixture.capture(false);
    fixture.assert_calls(10);
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        4,
        TurnDirection::Continue,
    );
    assert_eq!(fixture.captures[9].prompt, fixture.captures[10].prompt);
    fixture.assert_calls(11);
}

#[test]
fn both_restore_policies_control_actual_requests_snapshots_and_chapter_bridge() {
    for (policy, cap, bridge) in [
        (RestoreLimits::Original, 3, 2),
        (
            RestoreLimits::Current(Limits {
                max_major_events: MajorEventLimit::new(1).unwrap(),
                prose_bridge_turns: ProseBridgeTurns::new(0),
                max_generated_npcs: MaxGeneratedNpcs::new(0),
                min_playable_characters: MinPlayableCharacters::new(5).unwrap(),
            }),
            1,
            0,
        ),
    ] {
        let (mut fixture, mut cases, mut game) = start(limits(3));
        for i in 0..3 {
            advance(
                &mut fixture,
                &mut cases,
                &mut game,
                i,
                TurnDirection::Continue,
            );
        }
        let original_limits = game.original_limits();
        let mut restored = GameState::restore(
            game.brief().clone(),
            game.selected_world().clone(),
            game.style().clone(),
            original_limits,
            policy,
            game.turns().to_vec(),
        );
        assert_eq!(restored.world().cast().playable().len(), 2);
        assert_eq!(restored.world().cast().npcs().len(), 2);
        assert_eq!(restored.original_limits(), original_limits);
        assert_eq!(restored.prose_context().0.len(), bridge);
        assert_eq!(
            events(&restored),
            if cap == 1 {
                vec!["D"]
            } else {
                vec!["B", "C", "D"]
            }
        );
        for record in restored.turns() {
            assert!(record.summary().major_events().events().as_slice().len() <= cap);
        }
        let before = restored.clone();
        advance(
            &mut fixture,
            &mut cases,
            &mut restored,
            3,
            TurnDirection::Continue,
        );
        let request = &fixture.captures[5];
        assert!(
            request
                .instructions
                .contains(&format!("at most {cap} major events"))
        );
        assert!(request.schema["$defs"]["SummaryUpdate"]["properties"]["consolidated_major_events"]["description"].as_str().unwrap().contains(&format!("{cap} major events")));
        assert_eq!(
            request
                .prompt
                .contains("The closing prose of the previous chapter"),
            bridge > 0
        );
        assert_eq!(
            request.prompt.contains("Opening: the lantern flickered."),
            bridge > 0
        );
        assert!(request.prompt.contains("Voyage: the ship sailed."));
        assert_eq!(
            events(&restored),
            if cap == 1 {
                vec!["E"]
            } else {
                vec!["C", "D", "E"]
            }
        );
        cases
            .rewind(&mut restored, TurnCount::new(1).unwrap())
            .unwrap();
        assert_eq!(restored, before);
        fixture.assert_calls(6);
        advance(
            &mut fixture,
            &mut cases,
            &mut restored,
            4,
            TurnDirection::Continue,
        );
        assert_eq!(fixture.captures[5].prompt, fixture.captures[6].prompt);
        assert_eq!(
            events(&restored),
            if cap == 1 {
                vec!["F"]
            } else {
                vec!["C", "D", "F"]
            }
        );
        assert_eq!(restored.original_limits(), original_limits);
        fixture.assert_calls(7);
    }
}

#[test]
fn zero_npc_limit_survives_cast_selection_opening_and_continuation() {
    let zero = Limits {
        max_generated_npcs: MaxGeneratedNpcs::new(0),
        ..limits(2)
    };
    let (mut fixture, mut cases, mut game) = start(zero);
    assert!(game.world().cast().npcs().is_empty());
    assert_eq!(game.current_summary().characters().len(), 1);
    assert!(
        fixture.captures[1]
            .instructions
            .contains("Create no NPCs; return an empty npcs list.")
    );
    assert_eq!(
        fixture.captures[1].schema["properties"]["npcs"]["description"],
        "No NPCs; return an empty list."
    );
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        0,
        TurnDirection::Continue,
    );
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        1,
        TurnDirection::Continue,
    );
    assert_eq!(game.current_summary().characters().len(), 1);
    assert_eq!(events(&game), ["B", "C"]);
    assert_eq!(upcoming(&game), ["Depart"]);
    for request in &fixture.captures[2..] {
        assert!(request.prompt.contains("\"id\": \"protagonist\""));
        assert!(!request.prompt.contains("\"id\": \"ajax\""));
        assert!(!request.prompt.contains("\"id\": \"ajax-2\""));
    }
    fixture.assert_calls(4);
}

#[test]
fn composed_story_preserves_namesakes_repaired_ids_retitling_and_rewind_context() {
    let (mut fixture, mut cases, mut game) = start(limits(2));
    assert_eq!(game.world().cast().npcs().len(), 2); // exact duplicate merchant removed
    assert_eq!(game.current_summary().characters().len(), 3); // namesakes survive across roles
    let opening_state = game.clone();
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        0,
        TurnDirection::Continue,
    );
    assert_eq!(game.current_chapter().unwrap().number().get(), 0);
    assert_eq!(
        game.turns()[0].turn().quick_actions().as_slice()[0].kind(),
        QuickActionKind::Other
    );
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        1,
        TurnDirection::Player(PlayerInput::new("Go to the gate").unwrap()),
    );
    assert_eq!(game.turns()[1].input().unwrap().as_str(), "Go to the gate");
    assert_eq!(
        game.current_chapter().unwrap().title().unwrap().as_str(),
        "Retitled"
    );
    assert_eq!(events(&game), ["B", "C"]);
    assert_eq!(upcoming(&game), ["Depart"]);
    let summary = game.current_summary();
    for (id, name, state) in [
        ("ajax", "Ajax the sailmaker", "On the quay"),
        ("ajax-2", "Ajax", "Still guarding"),
    ] {
        let character = summary
            .characters()
            .get(&CharacterId::new(id).unwrap())
            .unwrap();
        assert_eq!(character.name().as_str(), name);
        assert_eq!(character.details().current_state().unwrap().as_str(), state);
    }
    assert_eq!(summary.characters().len(), 3);
    let before_chapter = game.clone();
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        2,
        TurnDirection::Continue,
    );
    assert_eq!(game.current_chapter().unwrap().number().get(), 1);
    assert!(upcoming(&game).is_empty());
    assert_eq!(game.prose_context().0.len(), 2);
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        3,
        TurnDirection::Continue,
    );
    assert_eq!(
        game.current_chapter().unwrap().title().unwrap().as_str(),
        "At Sea"
    );
    assert_eq!(events(&game), ["D", "E"]);
    cases.rewind(&mut game, TurnCount::new(2).unwrap()).unwrap();
    assert_eq!(game, before_chapter);
    fixture.assert_calls(6);
    advance(
        &mut fixture,
        &mut cases,
        &mut game,
        4,
        TurnDirection::Continue,
    );
    assert_eq!(
        game.current_chapter().unwrap().title().unwrap().as_str(),
        "Retitled"
    );
    assert_eq!(events(&game), ["C", "F"]);
    assert_eq!(upcoming(&game), ["Depart"]);
    fixture.assert_calls(7);
    assert_eq!(fixture.captures[4].prompt, fixture.captures[6].prompt);
    assert!(
        !fixture.captures[6]
            .prompt
            .contains("Voyage: the ship sailed.")
    );
    assert!(
        !fixture.captures[6]
            .prompt
            .contains("Fourth: the waves rose.")
    );
    for prose in [
        "Opening: the lantern flickered.",
        "Second: the bell rang.",
        "Voyage: the ship sailed.",
    ] {
        assert!(fixture.captures[5].prompt.contains(prose));
    }
    for (index, request) in fixture.captures.iter().enumerate().skip(2) {
        assert_eq!(
            request
                .prompt
                .matches("Before writing the opening scene")
                .count(),
            usize::from(index == 2),
            "one opening identity instruction, no extra review call"
        );
        for id in ["protagonist", "ajax", "ajax-2"] {
            assert!(request.prompt.contains(&format!("\"id\": \"{id}\"")));
        }
        assert!(request.instructions.contains("at most 2 major events"));
        assert_eq!(
            request.schema["properties"]
                .as_object()
                .unwrap()
                .keys()
                .next()
                .unwrap(),
            "narrative"
        );
        // Claude's own adaptation only: the root $schema is gone and nothing
        // Codex needs (all-properties-required, annotated-ref unions) is applied.
        assert!(request.schema.get("$schema").is_none());
        let required = request.schema["$defs"]["SummaryUpdate"]["required"]
            .as_array()
            .unwrap();
        assert!(!required.contains(&json!("character_updates")));
        assert!(!request.instructions.to_lowercase().contains("advisor"));
        assert!(!request.prompt.to_lowercase().contains("advisor"));
    }
    assert!(fixture.captures[3].prompt.contains("Ajax the sailmaker"));
    assert!(fixture.captures[4].prompt.contains("Still guarding"));
    assert!(fixture.captures[4].prompt.contains("Depart")); // null kept the thread
    assert!(
        fixture.captures[5]
            .prompt
            .contains("\"upcoming_events\": []")
    ); // [] cleared it
    assert!(fixture.captures[6].prompt.contains("Depart")); // rewind restored it
    let shared = GenerationTemplates::bundled()
        .unwrap()
        .turn_request(&opening_state, None, false)
        .unwrap();
    assert!(shared.schema().get("$schema").is_some());
    assert_eq!(
        fixture.captures[2].schema["$defs"]["SummaryUpdate"]["required"],
        shared.schema()["$defs"]["SummaryUpdate"]["required"]
    );
}

#[test]
fn a_disagreeing_preview_is_shown_but_only_the_final_payload_is_committed() {
    let (mut fixture, mut cases, mut game) = start(limits(2));
    let raw = turn(0);
    let tentative = r#"{"narrative": "Tentative words the final payload does not contain"}"#;
    fixture.scenario(
        &transcript_with(
            &raw,
            "completed",
            &Shape {
                preview: Some(tentative),
                ..PLAIN
            },
        ),
        0,
        json!({}),
    );
    let mut previews = String::new();
    cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |s| previews.push_str(s),
        )
        .unwrap();
    fixture.capture(false);
    assert_eq!(
        previews,
        "Tentative words the final payload does not contain"
    );
    let record = game.turns().last().unwrap();
    assert_eq!(record.raw_response().as_str(), raw);
    assert_eq!(
        record.turn().narrative().as_str(),
        data()["turns"][0]["narrative"].as_str().unwrap().trim()
    );
    assert!(!record.turn().narrative().as_str().contains("Tentative"));
    fixture.assert_calls(3);
}

#[test]
fn an_enforce_retry_stream_commits_only_the_structured_payload() {
    let (mut fixture, mut cases, mut game) = start(limits(2));
    let raw = turn(0);
    fixture.scenario(
        &transcript_with(
            &raw,
            "completed",
            &Shape {
                enforce_retry: true,
                ..PLAIN
            },
        ),
        0,
        json!({}),
    );
    let mut previews = String::new();
    cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |s| previews.push_str(s),
        )
        .unwrap();
    fixture.capture(false);
    assert!(
        !previews.contains("WRONG PROSE"),
        "text blocks are never narrative"
    );
    assert_eq!(previews, data()["turns"][0]["narrative"].as_str().unwrap());
    let record = game.turns().last().unwrap();
    assert_eq!(record.raw_response().as_str(), raw);
    assert!(!record.turn().narrative().as_str().contains("WRONG PROSE"));
    fixture.assert_calls(3);
}
