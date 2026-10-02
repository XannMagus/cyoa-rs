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
    let deadline = Instant::now() + Duration::from_secs(5);
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
