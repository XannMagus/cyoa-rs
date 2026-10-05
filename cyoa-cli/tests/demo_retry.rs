//! Actual demo -> engine -> application -> worker -> canonical acceptance.
//! A handshake holds a consumed response before controller acceptance.
use cyoa_application::{
    cancellation::CancellationToken,
    generation::{Generated, GenerationFailure, StoryGenerator, StoryUseCases, TurnDirection},
};
use cyoa_core::{
    game::GameState,
    limits::Limits,
    style::StoryStyle,
    text::{Brief, PlayerInput},
    turn::StoryTurn,
    world::{PlayablePosition, WorldCast, WorldOutline},
};
use cyoa_infrastructure::generation::demo;
use cyoa_presentation::{
    runtime::{Intent, SessionRuntime},
    session::{Phase, SessionController, Stage},
    worker::PreviewLimit,
};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn demo_generator() -> impl StoryGenerator + Send {
    demo::harbour_v1().unwrap()
}
struct Gate<G> {
    inner: G,
    consumed: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl<G> Gate<G> {
    fn finished<T>(
        &mut self,
        result: Result<T, GenerationFailure>,
    ) -> Result<T, GenerationFailure> {
        if result.is_ok() {
            self.consumed.send(()).unwrap();
            self.release
                .recv_timeout(Duration::from_secs(5))
                .expect("demo handshake watchdog");
        }
        result
    }
}
impl<G: StoryGenerator> StoryGenerator for Gate<G> {
    fn outline(
        &mut self,
        brief: &Brief,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        let result = self.inner.outline(brief, cancel);
        self.finished(result)
    }
    fn cast(
        &mut self,
        brief: &Brief,
        outline: &WorldOutline,
        limits: &Limits,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldCast>, GenerationFailure> {
        let result = self.inner.cast(brief, outline, limits, cancel);
        self.finished(result)
    }
    fn turn(
        &mut self,
        game: &GameState,
        direction: &TurnDirection,
        cancel: &CancellationToken,
        preview: &mut dyn FnMut(&str),
    ) -> Result<Generated<StoryTurn>, GenerationFailure> {
        let result = self.inner.turn(game, direction, cancel, preview);
        self.finished(result)
    }
}
fn settle<G: StoryGenerator + Send + 'static>(runtime: &mut SessionRuntime<G>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while matches!(
        runtime.controller().phase(),
        Phase::Running | Phase::Cancelling | Phase::Closing
    ) {
        runtime.poll();
        assert!(Instant::now() < deadline, "demo worker watchdog");
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn cancellation_after_demo_consumption_retries_each_stage_and_preserves_all_passages() {
    let (consumed_tx, consumed_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let cases = StoryUseCases::new(Gate {
        inner: demo_generator(),
        consumed: consumed_tx,
        release: release_rx,
    });
    let mut runtime = SessionRuntime::new(
        SessionController::new(Limits::default(), StoryStyle::default()),
        cases,
        PreviewLimit::default(),
    );
    let run = |runtime: &mut SessionRuntime<_>, intent, cancel: bool| {
        runtime.dispatch(intent).unwrap();
        consumed_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("demo did not generate the expected stage");
        if cancel {
            runtime.dispatch(Intent::Cancel).unwrap();
        }
        release_tx.send(()).unwrap();
        settle(runtime);
    };
    let cancelled = |runtime: &SessionRuntime<_>| {
        assert_eq!(
            runtime.controller().phase(),
            Phase::Failed,
            "demo cancellation must leave retryable failure"
        );
    };
    run(
        &mut runtime,
        Intent::SubmitBrief(Brief::new("A harbour").unwrap()),
        true,
    );
    cancelled(&runtime);
    assert_eq!(runtime.controller().stage(), &Stage::Brief);
    // If retry gets the cast payload, it fails before reaching the success gate.
    runtime.dispatch(Intent::Retry).unwrap();
    let retry_generated = consumed_rx.recv_timeout(Duration::from_secs(2)).is_ok();
    if retry_generated {
        release_tx.send(()).unwrap();
    }
    settle(&mut runtime);
    assert!(
        retry_generated,
        "demo outline retry regression: consumed outline advanced the replay"
    );
    assert_eq!(runtime.controller().phase(), Phase::Ready);
    let outline_stage = runtime.controller().stage().clone();
    run(&mut runtime, Intent::AcceptOutline, true);
    cancelled(&runtime);
    assert_eq!(runtime.controller().stage(), &outline_stage);
    run(&mut runtime, Intent::Retry, false);
    assert_eq!(runtime.controller().phase(), Phase::Ready);
    runtime
        .dispatch(Intent::Select(PlayablePosition::new(0)))
        .unwrap();
    let expected = [
        "Opening: the lantern flickered.",
        "Second: the bell rang.",
        "Voyage: the ship sailed.",
        "Fourth: the waves rose.",
        "Rewritten: the ship stayed.",
    ];
    for (index, narrative) in expected.iter().enumerate() {
        let before = runtime.controller().game().unwrap().clone();
        run(&mut runtime, Intent::Turn(TurnDirection::Continue), true);
        cancelled(&runtime);
        assert_eq!(runtime.controller().game(), Some(&before));
        if index % 2 == 0 {
            run(&mut runtime, Intent::Retry, false);
        } else {
            run(
                &mut runtime,
                Intent::Turn(TurnDirection::Player(
                    PlayerInput::new("A changed direction after cancel").unwrap(),
                )),
                false,
            );
        }
        let game = runtime.controller().game().unwrap();
        assert_eq!(game.turns().len(), index + 1);
        assert_eq!(
            game.turns().last().unwrap().turn().narrative().as_str(),
            *narrative,
            "demo passage retry regression"
        );
    }
    let before = runtime.controller().game().unwrap().clone();
    runtime
        .dispatch(Intent::Turn(TurnDirection::Continue))
        .unwrap();
    settle(&mut runtime);
    assert_eq!(runtime.controller().phase(), Phase::Failed);
    assert_eq!(runtime.controller().game(), Some(&before));
    runtime.dispatch(Intent::Retry).unwrap();
    settle(&mut runtime);
    assert_eq!(runtime.controller().phase(), Phase::Failed);
    assert_eq!(runtime.controller().game(), Some(&before));
}

#[test]
fn cancelled_consumed_demo_passages_preserve_disk_and_retry_all_five_with_persistence() {
    use cyoa_application::{cancellation::CancellationSource, persistence::*};
    use cyoa_infrastructure::persistence::{
        codec,
        helper::{HelperConfig, SupervisedRepository},
        repository::LocalRepository,
    };
    use cyoa_presentation::{
        persistence::{Durability, HeadlessSession, PersistedSession},
        storage::StorageRunner,
    };
    let root = tempfile::tempdir().unwrap();
    let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
    let game = codec::decode(
        include_bytes!("../../cyoa-infrastructure/tests/fixtures/saves/v1-minimal.json"),
        &id,
        SaveCopy::Primary,
    )
    .unwrap()
    .snapshot
    .into_game();
    let (consumed_tx, consumed_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let runtime = SessionRuntime::new(
        SessionController::from_game(game),
        StoryUseCases::new(Gate {
            inner: demo_generator(),
            consumed: consumed_tx,
            release: release_rx,
        }),
        PreviewLimit::default(),
    );
    let config = HelperConfig::new(
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_cyoa")),
        root.path().into(),
    )
    .unwrap();
    let mut session = PersistedSession::new(
        runtime,
        StorageRunner::new(move || SupervisedRepository::new(config.clone())),
        StorySource::Demo {
            scenario: DemoScenarioId::HarbourV1,
        },
    );
    fn finish(session: &mut dyn HeadlessSession) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while session.storage_busy()
            || matches!(
                session.controller().phase(),
                Phase::Running | Phase::Cancelling | Phase::Closing
            )
        {
            session.poll();
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }
    session.save(false).unwrap();
    finish(&mut session);
    let mut reader = LocalRepository::new(root.path().into()).unwrap();
    let token = CancellationSource::default().token();
    let id = reader
        .list(SavePage::new(None, PageSize::new(100).unwrap()), &token)
        .unwrap()
        .entries[0]
        .id
        .clone();
    let path = root
        .path()
        .join("saves")
        .join(format!("{}.json", id.as_str()));
    for (index, narrative) in [
        "Opening: the lantern flickered.",
        "Second: the bell rang.",
        "Voyage: the ship sailed.",
        "Fourth: the waves rose.",
        "Rewritten: the ship stayed.",
    ]
    .iter()
    .enumerate()
    {
        let before = session.controller().game().unwrap().clone();
        let bytes = std::fs::read(&path).unwrap();
        session
            .dispatch(Intent::Turn(TurnDirection::Continue))
            .unwrap();
        consumed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        session.dispatch(Intent::Cancel).unwrap();
        release_tx.send(()).unwrap();
        finish(&mut session);
        assert_eq!(session.controller().game(), Some(&before));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(session.durability(), Durability::Clean);
        session.dispatch(Intent::Retry).unwrap();
        consumed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        release_tx.send(()).unwrap();
        finish(&mut session);
        let stored = reader.load(&id, SaveCopy::Primary, &token).unwrap();
        assert_eq!(stored.snapshot.game().turns().len(), index + 1);
        assert_eq!(
            stored
                .snapshot
                .game()
                .turns()
                .last()
                .unwrap()
                .turn()
                .narrative()
                .as_str(),
            *narrative,
            "persistent demo replay drift"
        );
        assert_eq!(stored.snapshot.game(), session.controller().game().unwrap());
    }
}
