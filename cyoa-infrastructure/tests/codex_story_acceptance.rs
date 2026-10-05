//! Composed real-adapter acceptance. Frozen synthetic story, actual fixture
//! children and supervisor; no installed Codex, credentials or network.
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
        codex_cli::{CodexCliBackend, CodexExecutable, CodexInvocationConfig},
        process::{MaxStderrBytes, MaxStdoutBytes, ProcessBounds},
    },
    generation::{engine::GenerationEngine, templates::GenerationTemplates},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

type Cases = StoryUseCases<GenerationEngine<CodexCliBackend>>;
const STDERR: &[u8] = b"fixture diagnostics\xff\r\n";
fn data() -> Value {
    serde_json::from_str(include_str!("fixtures/codex/story.json")).unwrap()
}
fn turn(index: usize) -> String {
    data()["turns"][index].to_string()
}
fn transcript(raw: &str, terminal: &str) -> Vec<u8> {
    let mut lines = vec![
        json!({"type":"thread.started","thread_id":"story-thread"}),
        json!({"type":"turn.started"}),
        json!({"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":raw}}),
    ];
    match terminal {
        "completed" => lines.push(json!({"type":"turn.completed"})), // deliberately absent telemetry
        "failed" => lines
            .push(json!({"type":"turn.failed","error":{"message":"synthetic terminal failure"}})),
        "missing" => (),
        _ => panic!("unknown synthetic terminal"),
    }
    lines
        .into_iter()
        .flat_map(|v| {
            let mut line = serde_json::to_vec(&v).unwrap();
            line.extend_from_slice(b"\r\n");
            line
        })
        .collect()
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
        let executable = CodexExecutable::resolve(
            env!("CARGO_BIN_EXE_subprocess_fixture").as_ref(),
            "/bin".as_ref(),
            fixture.home.path(),
        )
        .unwrap();
        let config = CodexInvocationConfig::new(
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
                MaxStdoutBytes::new(32768).unwrap(),
                MaxStderrBytes::new(8192).unwrap(),
            )
            .unwrap(),
        );
        let backend =
            CodexCliBackend::connect(config, &CancellationSource::default().token()).unwrap();
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
        let mut scenario = json!({"stdout":[{"bytes":stdout}],"stderr":[{"bytes":STDERR}],"drain_stdin":true,"exit_code":exit,"report_path":self.report_path()});
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
        let stdin: Vec<u8> = serde_json::from_value(report["stdin"].clone()).unwrap();
        // Parse the actual child's stdin envelope, not a renderer/adapter expectation.
        let text = String::from_utf8(stdin).unwrap();
        let envelope: Value = serde_json::from_str(text.split_once('\n').unwrap().1).unwrap();
        let schema: Vec<u8> = serde_json::from_value(report["schema"].clone()).unwrap();
        self.captures.push(CapturedRequest {
            prompt: envelope["prompt"].as_str().unwrap().into(),
            instructions: envelope["instructions"].as_str().unwrap().into(),
            schema: serde_json::from_slice(&schema).unwrap(),
        });
    }
    fn assert_calls(&self, generations: usize) {
        let log = fs::read_to_string(self.home.path().join("launches.jsonl")).unwrap();
        let calls: Vec<Value> = log
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(calls.iter().filter(|c| c["command"] == "login").count(), 1);
        assert_eq!(
            calls.iter().filter(|c| c["command"] == "exec").count(),
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
    game: GameState,
    index: usize,
    direction: TurnDirection,
) -> GameState {
    let raw = turn(index);
    fixture.respond(&raw);
    let mut previews = vec![];
    let game = cases
        .take_turn(
            game,
            direction,
            &CancellationSource::default().token(),
            &mut |s| previews.push(s.to_owned()),
        )
        .unwrap();
    fixture.capture(false);
    assert_eq!(
        previews,
        [data()["turns"][index]["narrative"].as_str().unwrap()]
    );
    assert_eq!(game.turns().last().unwrap().raw_response().as_str(), raw);
    assert_eq!(
        game.turns().last().unwrap().provenance(),
        &Default::default()
    );
    game
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
    game = advance(&mut fixture, &mut cases, game, 0, TurnDirection::Continue);
    let malformed = " \r\n{\"narrative\":\"broken 🎭";
    let mut invalid = data()["turns"][1].clone();
    invalid["narrative"] = json!("  ");
    for (raw, terminal, exit, kind) in [
        (turn(1), "completed", 7, FailureKind::Transport),
        (turn(1), "failed", 0, FailureKind::Transport),
        (malformed.into(), "completed", 0, FailureKind::Transport),
        (
            invalid.to_string(),
            "completed",
            0,
            FailureKind::InvalidResponse,
        ),
        (turn(1), "missing", 0, FailureKind::Transport),
    ] {
        let before = game.clone();
        let stdout = transcript(&raw, terminal);
        fixture.scenario(&stdout, exit, json!({}));
        let mut preview = String::new();
        let failed = cases
            .take_turn(
                game,
                TurnDirection::Continue,
                &CancellationSource::default().token(),
                &mut |s| preview.push_str(s),
            )
            .unwrap_err();
        game = failed.state;
        let error = failed.failure;
        assert_eq!(error.kind(), kind);
        assert_eq!(game, before);
        assert_eq!(
            error.raw_response().expect("backend response").as_str(),
            raw
        );
        // Invalid domain text may be previewed, but never becomes a committed turn.
        if kind != FailureKind::InvalidResponse {
            assert!(preview.is_empty());
        }
        assert_eq!(error.diagnostics().stdout(), stdout);
        if terminal != "failed" {
            assert_eq!(error.diagnostics().stderr(), STDERR);
        }
        fixture.capture(false);
        let failed = fixture.captures.len() - 1;
        fixture.assert_calls(failed + 1);
        game = advance(&mut fixture, &mut cases, game, 1, TurnDirection::Continue);
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
    game = advance(&mut fixture, &mut cases, game, 0, TurnDirection::Continue);
    let before = game.clone();
    let raw = turn(1);
    fixture.scenario(
        &transcript(&raw, "completed"),
        0,
        json!({"replace_workspace_with_file":true}),
    );
    let failed = cases
        .take_turn(
            game,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |_| panic!("cleanup must precede emission"),
        )
        .unwrap_err();
    game = failed.state;
    let error = failed.failure;
    assert_eq!(game, before);
    assert_eq!(error.kind(), FailureKind::Transport);
    assert!(error.to_string().contains("WorkspaceCleanup"));
    assert_eq!(
        error.raw_response().expect("backend response").as_str(),
        raw
    );
    assert_eq!(error.diagnostics().stdout(), transcript(&raw, "completed"));
    assert_eq!(error.diagnostics().stderr(), STDERR);
    fixture.capture(true);
    advance(&mut fixture, &mut cases, game, 1, TurnDirection::Continue);
    assert_eq!(fixture.captures[3].prompt, fixture.captures[4].prompt);
    fixture.assert_calls(5);
}

#[test]
fn cancellation_and_output_cap_preserve_state_context_and_explicit_retry() {
    let (mut fixture, mut cases, mut game) = start(limits(2));
    game = advance(&mut fixture, &mut cases, game, 0, TurnDirection::Continue);
    let before = game.clone();
    fixture.scenario(&[], 0, json!({"hang_ms":10000}));
    let source = CancellationSource::default();
    let token = source.token();
    let report = fixture.report_path();
    let canceller = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !report.exists() {
            if Instant::now() >= deadline {
                source.cancel();
                panic!("child-start handshake missing");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        source.cancel();
    });
    let failed = cases
        .take_turn(game, TurnDirection::Continue, &token, &mut |_| {
            panic!("silent child cannot preview")
        })
        .unwrap_err();
    game = failed.state;
    let error = failed.failure;
    canceller.join().unwrap();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert_eq!(game, before);
    assert_eq!(error.raw_response().expect("backend response").as_str(), "");
    fixture.capture(false);
    fixture.assert_calls(4);
    game = advance(&mut fixture, &mut cases, game, 1, TurnDirection::Continue);
    assert_eq!(fixture.captures[3].prompt, fixture.captures[4].prompt);

    // Complete-only transport has no pre-terminal preview. Cancel from the
    // accepted candidate's one prose callback; the application must not commit.
    let before = game.clone();
    fixture.respond(&turn(2));
    let source = CancellationSource::default();
    let mut previews = vec![];
    let failed = cases
        .take_turn(game, TurnDirection::Continue, &source.token(), &mut |s| {
            previews.push(s.to_owned());
            source.cancel();
        })
        .unwrap_err();
    game = failed.state;
    let error = failed.failure;
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert_eq!(game, before);
    assert_eq!(previews, ["Voyage: the ship sailed."]);
    assert_eq!(
        error.raw_response().expect("backend response").as_str(),
        turn(2)
    );
    assert_eq!(
        error.diagnostics().stdout(),
        transcript(&turn(2), "completed")
    );
    assert_eq!(error.diagnostics().stderr(), STDERR);
    fixture.capture(false);
    fixture.assert_calls(6);
    game = advance(&mut fixture, &mut cases, game, 2, TurnDirection::Continue);
    assert_eq!(fixture.captures[5].prompt, fixture.captures[6].prompt);

    let before = game.clone();
    let mut capped = transcript(&turn(3), "completed");
    capped.extend_from_slice(&[b'x'; 40000]);
    fixture.scenario(&capped, 0, json!({}));
    let failed = cases
        .take_turn(
            game,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |_| panic!("cap cannot preview"),
        )
        .unwrap_err();
    game = failed.state;
    let error = failed.failure;
    assert_eq!(error.kind(), FailureKind::Transport);
    assert_eq!(game, before);
    assert_eq!(error.diagnostics().stdout(), &capped[..32768]);
    assert_eq!(
        error.diagnostics().stdout_capture().completeness(),
        cyoa_application::diagnostics::CaptureCompleteness::Prefix
    );
    fixture.capture(false);
    fixture.assert_calls(8);
    advance(&mut fixture, &mut cases, game, 3, TurnDirection::Continue);
    assert_eq!(fixture.captures[7].prompt, fixture.captures[8].prompt);
    fixture.assert_calls(9);
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
            game = advance(&mut fixture, &mut cases, game, i, TurnDirection::Continue);
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
        restored = advance(
            &mut fixture,
            &mut cases,
            restored,
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
        restored.rewind(TurnCount::new(1).unwrap()).unwrap();
        assert_eq!(restored, before);
        fixture.assert_calls(6);
        restored = advance(
            &mut fixture,
            &mut cases,
            restored,
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
    game = advance(&mut fixture, &mut cases, game, 0, TurnDirection::Continue);
    game = advance(&mut fixture, &mut cases, game, 1, TurnDirection::Continue);
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
    game = advance(&mut fixture, &mut cases, game, 0, TurnDirection::Continue);
    assert_eq!(game.current_chapter().unwrap().number().get(), 0);
    assert_eq!(
        game.turns()[0].turn().quick_actions().as_slice()[0].kind(),
        QuickActionKind::Other
    );
    game = advance(
        &mut fixture,
        &mut cases,
        game,
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
    game = advance(&mut fixture, &mut cases, game, 2, TurnDirection::Continue);
    assert_eq!(game.current_chapter().unwrap().number().get(), 1);
    assert!(upcoming(&game).is_empty());
    assert_eq!(game.prose_context().0.len(), 2);
    game = advance(&mut fixture, &mut cases, game, 3, TurnDirection::Continue);
    assert_eq!(
        game.current_chapter().unwrap().title().unwrap().as_str(),
        "At Sea"
    );
    assert_eq!(events(&game), ["D", "E"]);
    game.rewind(TurnCount::new(2).unwrap()).unwrap();
    assert_eq!(game, before_chapter);
    fixture.assert_calls(6);
    game = advance(&mut fixture, &mut cases, game, 4, TurnDirection::Continue);
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
            usize::from(index == 2)
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
        let required = request.schema["$defs"]["SummaryUpdate"]["required"]
            .as_array()
            .unwrap();
        assert!(required.contains(&json!("character_updates"))); // adaptation is present at actual child
        assert!(request.schema.get("$schema").is_some());
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
    assert!(
        !shared.schema()["$defs"]["SummaryUpdate"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("character_updates"))
    );
    let claude = cyoa_infrastructure::generation::backend_compat::claude_cli::adapt_schema(
        shared.schema().clone(),
    );
    assert!(claude.get("$schema").is_none());
    assert_eq!(
        claude["$defs"]["SummaryUpdate"]["required"],
        shared.schema()["$defs"]["SummaryUpdate"]["required"]
    );
}
