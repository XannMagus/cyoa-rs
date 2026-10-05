use cyoa_application::{
    cancellation::CancellationToken,
    generation::{FailureKind, StoryUseCases, TurnDirection},
};
use cyoa_core::{
    game::{GameState, TurnCount},
    ids::CharacterId,
    limits::{Limits, MajorEventLimit, MaxGeneratedNpcs, RestoreLimits},
    style::StoryStyle,
    text::{Brief, WorldDescription, WorldTitle},
    turn::QuickActionKind,
    world::{PlayablePosition, WorldOutline},
};
use cyoa_infrastructure::{
    backend::*,
    generation::{
        engine::GenerationEngine, scripted::ChunkedBackend, templates::GenerationTemplates,
    },
};
use cyoa_presentation::{runtime::*, session::*, worker::PreviewLimit};
use serde_json::Value;
use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Capture {
    prompt: String,
    instructions: String,
    schema: Value,
}
struct Recorder<B> {
    backend: B,
    captures: Arc<Mutex<Vec<Capture>>>,
    pause: Option<(usize, mpsc::Sender<()>)>,
}
impl<B: Backend> Backend for Recorder<B> {
    fn generate(
        &mut self,
        r: GenerationRequest<'_>,
        cancel: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        let n = {
            let mut captures = self.captures.lock().unwrap();
            captures.push(Capture {
                prompt: r.prompt.into(),
                instructions: r.instructions.into(),
                schema: r.schema.clone(),
            });
            captures.len()
        };
        let pause = self
            .pause
            .as_ref()
            .filter(|(index, _)| *index == n)
            .map(|(_, tx)| tx.clone());
        let mut paused = false;
        let mut prefix = String::new();
        self.backend.generate(r, cancel, &mut |part| {
            on_json(part);
            if pause.is_some() {
                prefix.push_str(part);
            }
            if !paused
                && prefix.contains("Fourth:")
                && let Some(tx) = &pause
            {
                paused = true;
                let (wake, rx) = mpsc::channel();
                let _guard = cancel.subscribe(move || {
                    let _ = wake.send(());
                });
                tx.send(()).unwrap();
                rx.recv_timeout(Duration::from_secs(5))
                    .expect("controlled cancellation watchdog");
            }
        })
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../cyoa-infrastructure/tests/fixtures/phase0_story.json"
    ))
    .unwrap()
}
type Runtime = SessionRuntime<GenerationEngine<Recorder<ChunkedBackend>>>;
fn runtime(
    responses: Vec<String>,
    limits: Limits,
    pause: Option<(usize, mpsc::Sender<()>)>,
) -> (Runtime, Arc<Mutex<Vec<Capture>>>) {
    let captures = Arc::new(Mutex::new(Vec::new()));
    let backend = Recorder {
        backend: ChunkedBackend::new(responses.into_iter().map(Ok), 42),
        captures: Arc::clone(&captures),
        pause,
    };
    (
        SessionRuntime::new(
            SessionController::new(limits, StoryStyle::default()),
            StoryUseCases::new(GenerationEngine::new(
                backend,
                GenerationTemplates::bundled().unwrap(),
            )),
            PreviewLimit::default(),
        ),
        captures,
    )
}
fn settle(runtime: &mut Runtime) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while matches!(
        runtime.controller().phase(),
        Phase::Running | Phase::Cancelling | Phase::Closing
    ) {
        runtime.poll();
        assert!(Instant::now() < deadline, "controller story watchdog");
        thread::sleep(Duration::from_millis(1));
    }
}
fn dispatch(runtime: &mut Runtime, intent: Intent) {
    runtime.dispatch(intent).unwrap();
    settle(runtime);
}
fn advance(runtime: &mut Runtime) {
    dispatch(runtime, Intent::Turn(TurnDirection::Continue));
    assert_eq!(runtime.controller().phase(), Phase::Ready);
}
fn events(game: &GameState) -> Vec<String> {
    game.current_summary()
        .major_events()
        .events()
        .iter()
        .map(|e| e.as_str().to_owned())
        .collect()
}

#[test]
fn complete_controller_story_preserves_context_retry_identity_chapters_and_rewind() {
    let data = fixture();
    let t: Vec<_> = data["turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(Value::to_string)
        .collect();
    let malformed = " \r\n{\"narrative\":\"Broken preview 🎭";
    let responses = vec![
        data["outline"].to_string(),
        data["cast"].to_string(),
        t[0].clone(),
        t[1].clone(),
        malformed.into(),
        t[2].clone(),
        t[3].clone(),
        t[3].clone(),
        t[4].clone(),
    ];
    let limits = Limits {
        max_major_events: MajorEventLimit::new(2).unwrap(),
        ..Limits::default()
    };
    let (tx, rx) = mpsc::channel();
    let (mut runtime, captures) = runtime(responses, limits, Some((7, tx)));
    dispatch(
        &mut runtime,
        Intent::SubmitBrief(Brief::new("Two Ajaxes and a bell").unwrap()),
    );
    dispatch(
        &mut runtime,
        Intent::ReplaceOutline(WorldOutline::new(
            WorldTitle::new("Edited harbour").unwrap(),
            WorldDescription::new("Edited coastline").unwrap(),
        )),
    );
    dispatch(&mut runtime, Intent::AcceptOutline);
    assert!(
        runtime
            .dispatch(Intent::Select(PlayablePosition::new(99)))
            .is_err()
    );
    dispatch(&mut runtime, Intent::Select(PlayablePosition::new(1)));
    assert_eq!(
        runtime
            .controller()
            .game()
            .unwrap()
            .world()
            .cast()
            .npcs()
            .len(),
        2
    );
    advance(&mut runtime);
    advance(&mut runtime);
    let before = runtime.controller().game().unwrap().clone();
    assert_eq!(before.turns().len(), 2, "double commit regression");
    assert_eq!(events(&before), ["B", "C"]);
    assert_eq!(
        before.turns()[0].turn().quick_actions().as_slice()[0].kind(),
        QuickActionKind::Other
    );
    assert_eq!(
        before.current_chapter().unwrap().title().unwrap().as_str(),
        "Retitled"
    );
    for (id, state) in [("ajax", "At the shop"), ("ajax-2", "At the gate")] {
        assert_eq!(
            before
                .current_summary()
                .characters()
                .get(&CharacterId::new(id).unwrap())
                .unwrap()
                .details()
                .current_state()
                .unwrap()
                .as_str(),
            state
        );
    }
    dispatch(&mut runtime, Intent::Turn(TurnDirection::Continue));
    assert_eq!(runtime.controller().game().unwrap(), &before);
    let Some(Failure::Generation(error)) = runtime.controller().failure() else {
        panic!("expected failure")
    };
    assert_eq!(error.kind(), FailureKind::Transport);
    assert_eq!(error.raw_response().as_str(), malformed);
    dispatch(&mut runtime, Intent::Retry);
    assert_eq!(
        runtime
            .controller()
            .game()
            .unwrap()
            .current_chapter()
            .unwrap()
            .number()
            .get(),
        1
    );
    assert_eq!(
        runtime.controller().game().unwrap().prose_context().0.len(),
        2
    );
    assert!(
        runtime
            .controller()
            .game()
            .unwrap()
            .current_summary()
            .upcoming_events()
            .as_slice()
            .is_empty()
    );
    let before_cancel = runtime.controller().game().unwrap().clone();
    runtime
        .dispatch(Intent::Turn(TurnDirection::Continue))
        .unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    runtime.poll();
    assert!(!runtime.controller().preview().is_empty());
    runtime.dispatch(Intent::Cancel).unwrap();
    settle(&mut runtime);
    assert_eq!(runtime.controller().game().unwrap(), &before_cancel);
    dispatch(&mut runtime, Intent::Retry);
    assert_eq!(
        runtime
            .controller()
            .game()
            .unwrap()
            .current_chapter()
            .unwrap()
            .title()
            .unwrap()
            .as_str(),
        "At Sea"
    );
    dispatch(&mut runtime, Intent::Rewind(TurnCount::new(2).unwrap()));
    assert_eq!(runtime.controller().game().unwrap(), &before);
    advance(&mut runtime);
    assert_eq!(events(runtime.controller().game().unwrap()), ["C", "F"]);
    let records = captures.lock().unwrap();
    assert_eq!(records.len(), 9);
    assert!(records[1].prompt.contains("Edited coastline"));
    assert_eq!(records[4].prompt, records[5].prompt);
    assert_eq!(records[6].prompt, records[7].prompt);
    assert_eq!(records[8].prompt, records[4].prompt);
    for (index, r) in records.iter().enumerate().skip(2) {
        assert_eq!(
            r.prompt.matches("Before writing the opening scene").count(),
            usize::from(index == 2)
        );
        assert!(r.instructions.contains("at most 2 major events"));
        assert!(r.schema["$defs"]["SummaryUpdate"]["properties"]["consolidated_major_events"]["description"].as_str().unwrap().contains("2 major events"));
        for id in ["protagonist", "ajax", "ajax-2"] {
            assert!(r.prompt.contains(&format!("\"id\": \"{id}\"")));
        }
    }
    assert!(
        records[6]
            .prompt
            .contains("The closing prose of the previous chapter")
    );
}

#[test]
fn controller_zero_npcs_and_both_restore_policies_use_active_limits() {
    let data = fixture();
    let limits = Limits {
        max_generated_npcs: MaxGeneratedNpcs::new(0),
        max_major_events: MajorEventLimit::new(2).unwrap(),
        ..Limits::default()
    };
    let (mut original, _) = runtime(
        vec![
            data["outline"].to_string(),
            data["cast"].to_string(),
            data["turns"][0].to_string(),
        ],
        limits,
        None,
    );
    dispatch(
        &mut original,
        Intent::SubmitBrief(Brief::new("bell").unwrap()),
    );
    dispatch(&mut original, Intent::AcceptOutline);
    dispatch(&mut original, Intent::Select(PlayablePosition::new(0)));
    advance(&mut original);
    let game = original.controller().game().unwrap().clone();
    assert!(game.world().cast().npcs().is_empty());
    for (policy, cap) in [
        (RestoreLimits::Original, 2),
        (
            RestoreLimits::Current(Limits {
                max_major_events: MajorEventLimit::new(1).unwrap(),
                ..limits
            }),
            1,
        ),
    ] {
        let restored = GameState::restore(
            game.brief().clone(),
            game.selected_world().clone(),
            game.style().clone(),
            game.original_limits(),
            policy,
            game.turns().to_vec(),
        );
        let captures = Arc::new(Mutex::new(Vec::new()));
        let backend = Recorder {
            backend: ChunkedBackend::new([Ok(data["turns"][1].to_string())], 1),
            captures: Arc::clone(&captures),
            pause: None,
        };
        let mut restored_runtime = SessionRuntime::new(
            SessionController::from_game(restored),
            StoryUseCases::new(GenerationEngine::new(
                backend,
                GenerationTemplates::bundled().unwrap(),
            )),
            PreviewLimit::default(),
        );
        advance(&mut restored_runtime);
        assert_eq!(
            restored_runtime
                .controller()
                .game()
                .unwrap()
                .limits()
                .max_major_events
                .get(),
            cap
        );
        assert_eq!(
            events(restored_runtime.controller().game().unwrap()).len(),
            cap
        );
        assert!(
            captures.lock().unwrap()[0]
                .instructions
                .contains(&format!("at most {cap} major events"))
        );
    }
}
