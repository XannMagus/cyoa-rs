mod support;
use cyoa_application::{cancellation::CancellationToken, generation::*};
use cyoa_core::{
    game::GameState,
    limits::Limits,
    text::Brief,
    turn::{ChapterMarker, StoryTurn},
    world::{WorldCast, WorldOutline},
};
use cyoa_presentation::{runtime::*, session::*, worker::*};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use support::*;

enum Mode {
    Wait,
    Success,
    Panic,
    Flood,
}
struct Fake {
    modes: VecDeque<Mode>,
    started: mpsc::Sender<usize>,
    calls: Arc<Mutex<Vec<TurnDirection>>>,
}
impl StoryGenerator for Fake {
    fn outline(
        &mut self,
        _: &Brief,
        _: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        Ok(generated(outline("world")))
    }
    fn cast(
        &mut self,
        _: &Brief,
        _: &WorldOutline,
        _: &Limits,
        _: &CancellationToken,
    ) -> Result<Generated<WorldCast>, GenerationFailure> {
        Ok(generated(world().cast().clone()))
    }
    fn turn(
        &mut self,
        _: &GameState,
        direction: &TurnDirection,
        token: &CancellationToken,
        progress: &mut dyn FnMut(&str),
    ) -> Result<Generated<StoryTurn>, GenerationFailure> {
        let n = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(direction.clone());
            calls.len()
        };
        let _ = self.started.send(n);
        match self.modes.pop_front().expect("script exhausted") {
            Mode::Wait => {
                let (tx, rx) = mpsc::channel();
                let _registration = token.subscribe(move || {
                    let _ = tx.send(());
                });
                rx.recv_timeout(Duration::from_secs(5))
                    .expect("test cancellation watchdog");
                Err(GenerationFailure::new(
                    FailureKind::Cancelled,
                    "cancelled",
                    failure().raw_response().clone(),
                    failure().diagnostics().clone(),
                ))
            }
            Mode::Panic => panic!("controlled generator panic"),
            Mode::Flood => {
                for _ in 0..10000 {
                    progress("é雪");
                }
                let _ = self.started.send(n + 100);
                Ok(generated(turn(
                    "final",
                    ChapterMarker::Continue { title: None },
                )))
            }
            Mode::Success => {
                progress("tentative");
                Ok(generated(turn(
                    "final",
                    ChapterMarker::Continue { title: None },
                )))
            }
        }
    }
}
type Calls = Arc<Mutex<Vec<TurnDirection>>>;
fn fake(
    modes: impl IntoIterator<Item = Mode>,
) -> (StoryUseCases<Fake>, mpsc::Receiver<usize>, Calls) {
    let (tx, rx) = mpsc::channel();
    let calls = Arc::new(Mutex::new(Vec::new()));
    (
        StoryUseCases::new(Fake {
            modes: modes.into_iter().collect(),
            started: tx,
            calls: Arc::clone(&calls),
        }),
        rx,
        calls,
    )
}
fn finish(runner: &mut WorkerRunner<Fake>) -> WorkerEvent {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for e in runner.poll() {
            if !matches!(e, WorkerEvent::Progress { .. }) {
                return e;
            }
        }
        assert!(Instant::now() < deadline, "worker watchdog");
        thread::sleep(Duration::from_millis(1));
    }
}
fn settle(runtime: &mut SessionRuntime<Fake>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(
        runtime.controller().phase(),
        Phase::Running | Phase::Cancelling | Phase::Closing
    ) {
        runtime.poll();
        assert!(Instant::now() < deadline, "runtime watchdog");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn silent_cancel_joins_before_retry_and_preserves_generator_and_evidence() {
    let (cases, started, calls) = fake([Mode::Wait, Mode::Success]);
    let before = game();
    let mut runtime = SessionRuntime::new(
        SessionController::from_game(before.clone()),
        cases,
        PreviewLimit::default(),
    );
    runtime
        .dispatch(Intent::Turn(TurnDirection::Continue))
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    runtime.dispatch(Intent::Cancel).unwrap();
    assert_eq!(runtime.controller().phase(), Phase::Cancelling);
    assert!(runtime.dispatch(Intent::Retry).is_err());
    settle(&mut runtime);
    assert_eq!(runtime.controller().game().unwrap(), &before);
    let Some(Failure::Cancelled {
        rejected: Err(error),
    }) = runtime.controller().failure()
    else {
        panic!("cancel evidence lost")
    };
    assert_eq!(error.diagnostics().stdout(), [255, 13, 10]);
    runtime.dispatch(Intent::Retry).unwrap();
    settle(&mut runtime);
    assert_eq!(runtime.controller().game().unwrap().turns().len(), 1);
    assert_eq!(
        *calls.lock().unwrap(),
        [TurnDirection::Continue, TurnDirection::Continue]
    );
}
#[test]
fn joined_success_queued_before_cancel_is_discarded() {
    let (cases, _, _) = fake([Mode::Success]);
    let mut session = SessionController::from_game(game());
    let before = session.game().unwrap().clone();
    let r = session.take_turn(TurnDirection::Continue).unwrap();
    let mut runner = WorkerRunner::new(cases, PreviewLimit::default());
    runner.start(r).unwrap();
    let WorkerEvent::Finished(done) = finish(&mut runner) else {
        panic!("expected success")
    };
    assert!(done.outcome.is_ok());
    assert!(runner.is_ready());
    session.cancel();
    assert_eq!(session.complete(done), Acceptance::Failed);
    assert_eq!(
        session.game().unwrap(),
        &before,
        "cancelled success regression"
    );
}
#[test]
fn panic_faults_without_committing_and_quit_remains_available() {
    let (cases, _, _) = fake([Mode::Panic]);
    let before = game();
    let mut runtime = SessionRuntime::new(
        SessionController::from_game(before.clone()),
        cases,
        PreviewLimit::default(),
    );
    runtime
        .dispatch(Intent::Turn(TurnDirection::Continue))
        .unwrap();
    settle(&mut runtime);
    assert_eq!(runtime.controller().phase(), Phase::Faulted);
    assert_eq!(runtime.controller().game().unwrap(), &before);
    assert!(runtime.dispatch(Intent::Retry).is_err());
    runtime.dispatch(Intent::Quit).unwrap();
    assert_eq!(runtime.controller().phase(), Phase::Closed);
}
#[test]
fn paused_progress_consumer_cannot_block_completion_or_exceed_budget() {
    let (cases, started, _) = fake([Mode::Flood]);
    let mut session = SessionController::from_game(game());
    let r = session.take_turn(TurnDirection::Continue).unwrap();
    let mut runner = WorkerRunner::new(cases, PreviewLimit::new(7).unwrap());
    runner.start(r).unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut preview = String::new();
    // All callbacks finish before the consumer begins polling.
    assert_eq!(started.recv_timeout(Duration::from_secs(5)).unwrap(), 101);
    let mut incomplete = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let events = runner.poll();
        let mut done = false;
        for event in events {
            match event {
                WorkerEvent::Progress {
                    text,
                    incomplete: i,
                    ..
                } => {
                    preview.push_str(&text);
                    incomplete |= i;
                }
                WorkerEvent::Finished(c) => {
                    session.complete(c);
                    done = true;
                }
                _ => panic!("unexpected fault"),
            }
        }
        if done {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(preview, "é雪é");
    assert!(incomplete);
    assert_eq!(
        session.game().unwrap().turns()[0]
            .turn()
            .narrative()
            .as_str(),
        "final"
    );
}
#[test]
fn dropping_runner_cancels_and_joins_silent_request() {
    let (cases, started, _) = fake([Mode::Wait]);
    let mut session = SessionController::from_game(game());
    let r = session.take_turn(TurnDirection::Continue).unwrap();
    let token = r.token().clone();
    let mut runner = WorkerRunner::new(cases, PreviewLimit::default());
    runner.start(r).unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(runner);
    assert!(token.is_cancelled());
}
