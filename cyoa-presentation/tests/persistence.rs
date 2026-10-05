mod support;
use cyoa_application::{cancellation::CancellationToken, generation::*, persistence::*};
use cyoa_core::{
    game::{GameState, TurnCount},
    limits::{Limits, RestoreLimits},
    style::StoryStyle,
    text::Brief,
    turn::{ChapterMarker, StoryTurn},
    world::{PlayablePosition, WorldCast, WorldOutline},
};
use cyoa_presentation::{
    persistence::*, runtime::*, session::*, storage::StorageRunner, worker::PreviewLimit,
};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use support::*;
#[derive(Clone, Copy)]
enum Mode {
    Success,
    Fail,
    Unknown,
    Panic,
}
#[derive(Default)]
struct Disk {
    modes: VecDeque<Mode>,
    saved: Option<StoredGame>,
    pending_snapshot: Option<SaveSnapshot>,
    writes: Vec<(&'static str, usize)>,
    wait_write: Option<(Arc<AtomicBool>, Arc<AtomicBool>)>,
    wait_load: Option<Arc<AtomicBool>>,
}
struct Repo {
    disk: Arc<Mutex<Disk>>,
}
fn id() -> SaveId {
    SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap()
}
fn receipt(revision: u64) -> SaveReceipt {
    SaveReceipt {
        metadata: SaveMetadata {
            id: id(),
            revision: SaveRevision::new(revision).unwrap(),
            saved_at: SavedAt::new(1, 0).unwrap(),
        },
        stamp: ContentStamp::new([revision as u8; 32]),
    }
}
fn storage_error(operation: StorageOperation) -> StorageFailure {
    StorageFailure::new(
        operation,
        StorageStage::Write,
        StorageFailureKind::Io,
        "controlled write failure",
    )
}
impl Repo {
    fn write(
        &mut self,
        snapshot: SaveSnapshot,
        prepared: &PreparedWriteEvidence,
        kind: &'static str,
        operation: StorageOperation,
    ) -> Result<SaveReceipt, StorageFailure> {
        let mut disk = self.disk.lock().unwrap();
        disk.writes.push((kind, snapshot.game().turns().len()));
        let revision = disk
            .saved
            .as_ref()
            .map_or(1, |s| s.metadata.revision.get() + 1);
        let receipt = receipt(revision);
        let pending = PendingWrite::new(
            id(),
            disk.saved.as_ref().map(|s| s.stamp),
            receipt.stamp,
            b"opaque".to_vec(),
        )
        .unwrap();
        prepared.publish(pending.clone());
        disk.pending_snapshot = Some(snapshot.clone());
        let mode = disk.modes.pop_front().unwrap_or(Mode::Success);
        if let Some((started, release)) = disk.wait_write.take() {
            drop(disk);
            started.store(true, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(30);
            while !release.load(Ordering::SeqCst) {
                assert!(Instant::now() < deadline, "write release watchdog");
                std::thread::sleep(Duration::from_millis(1));
            }
            disk = self.disk.lock().unwrap();
        }
        if matches!(mode, Mode::Fail) {
            return Err(storage_error(operation));
        }
        disk.saved = Some(StoredGame {
            snapshot,
            metadata: receipt.metadata.clone(),
            stamp: receipt.stamp,
            copy: SaveCopy::Primary,
            unrecognized_fields: vec![],
        });
        drop(disk);
        match mode {
            Mode::Unknown => Err(storage_error(operation)
                .visibility_unknown()
                .prepared(pending)),
            Mode::Panic => panic!("controlled panic after prepared write"),
            _ => Ok(receipt),
        }
    }
}
impl GameRepository for Repo {
    fn create(
        &mut self,
        s: SaveSnapshot,
        prepared: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        self.write(s, prepared, "create", StorageOperation::Create)
    }
    fn replace(
        &mut self,
        _: SaveTarget,
        s: SaveSnapshot,
        prepared: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        self.write(s, prepared, "replace", StorageOperation::Replace)
    }
    fn reconcile(
        &mut self,
        p: PendingWrite,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        let mut disk = self.disk.lock().unwrap();
        let count = disk.pending_snapshot.as_ref().unwrap().game().turns().len();
        disk.writes.push(("reconcile", count));
        let saved = disk.saved.as_ref().unwrap();
        assert_eq!(saved.stamp, p.intended_stamp());
        Ok(SaveReceipt {
            metadata: saved.metadata.clone(),
            stamp: saved.stamp,
        })
    }
    fn load(
        &mut self,
        _: &SaveId,
        copy: SaveCopy,
        token: &CancellationToken,
    ) -> Result<StoredGame, StorageFailure> {
        let wait = self.disk.lock().unwrap().wait_load.take();
        if let Some(started) = wait {
            started.store(true, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(30);
            while !token.is_cancelled() {
                assert!(Instant::now() < deadline, "load cancellation watchdog");
                std::thread::sleep(Duration::from_millis(1));
            }
            return Err(StorageFailure::cancelled(StorageOperation::Load));
        }
        let mut stored = self
            .disk
            .lock()
            .unwrap()
            .saved
            .clone()
            .ok_or_else(|| storage_error(StorageOperation::Load))?;
        stored.copy = copy;
        Ok(stored)
    }
    fn list(
        &mut self,
        _: SavePage,
        _: &CancellationToken,
    ) -> Result<SavePageResult, StorageFailure> {
        Ok(SavePageResult {
            entries: vec![],
            next: None,
        })
    }
}
struct Generator {
    calls: Arc<AtomicUsize>,
    mode: GeneratorMode,
}
enum GeneratorMode {
    Success,
    Fail,
    Panic,
}
impl From<bool> for GeneratorMode {
    fn from(fail: bool) -> Self {
        if fail { Self::Fail } else { Self::Success }
    }
}
impl StoryGenerator for Generator {
    fn outline(
        &mut self,
        _: &Brief,
        _: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        Ok(generated(outline("Harbour")))
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
        _: &TurnDirection,
        _: &CancellationToken,
        _: &mut dyn FnMut(&str),
    ) -> Result<Generated<StoryTurn>, GenerationFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if matches!(self.mode, GeneratorMode::Panic) {
            panic!("controlled generation panic");
        }
        if matches!(self.mode, GeneratorMode::Fail) {
            Err(failure())
        } else {
            Ok(generated(turn(
                "passage",
                ChapterMarker::Continue { title: None },
            )))
        }
    }
}
type Session = PersistedSession<Generator, Repo, Box<dyn Fn() -> Repo + Send + Sync>>;
fn session(
    controller: SessionController,
    disk: Arc<Mutex<Disk>>,
    mode: impl Into<GeneratorMode>,
) -> (Session, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = SessionRuntime::new(
        controller,
        StoryUseCases::new(Generator {
            calls: calls.clone(),
            mode: mode.into(),
        }),
        PreviewLimit::default(),
    );
    let factory: Box<dyn Fn() -> Repo + Send + Sync> =
        Box::new(move || Repo { disk: disk.clone() });
    (
        PersistedSession::new(runtime, StorageRunner::new(factory), StorySource::Live),
        calls,
    )
}
fn settle(s: &mut Session) -> Vec<PersistenceEvent> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut events = vec![];
    loop {
        events.extend(s.poll());
        if !s.storage_busy()
            && !matches!(
                s.controller().phase(),
                Phase::Running | Phase::Cancelling | Phase::Closing
            )
        {
            return events;
        }
        assert!(Instant::now() < deadline, "coordinator watchdog");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn bound(s: &mut Session) {
    s.save(false).unwrap();
    settle(s);
    assert_eq!(s.durability(), Durability::Clean);
}
#[test]
fn failed_autosave_keeps_accepted_turn_and_storage_retry_never_regenerates() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(SessionController::from_game(game()), disk.clone(), false);
    bound(&mut s);
    disk.lock().unwrap().modes.push_back(Mode::Fail);
    s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
    settle(&mut s);
    assert_eq!(
        s.controller().game().unwrap().turns().len(),
        1,
        "save failure must never retry inference"
    );
    assert_eq!(s.durability(), Durability::Dirty);
    assert!(s.storage_failure().is_some());
    assert!(s.controller().failure().is_none());
    assert!(s.dispatch(Intent::Turn(TurnDirection::Continue)).is_err());
    assert!(
        s.dispatch(Intent::Rewind(TurnCount::new(1).unwrap()))
            .is_err()
    );
    assert!(
        s.load(LoadGame {
            id: id(),
            copy: SaveCopy::Primary,
            limits: RestoreLimits::Original
        })
        .is_err()
    );
    let canonical = s.controller().game().unwrap().clone();
    s.save(false).unwrap();
    settle(&mut s);
    assert_eq!(s.controller().game().unwrap(), &canonical);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.durability(), Durability::Clean);
    assert!(s.storage_failure().is_none());
    assert_eq!(
        disk.lock().unwrap().writes,
        [("create", 0), ("replace", 1), ("replace", 1)]
    );
}
#[test]
fn uncertain_write_and_worker_panic_reconcile_same_attempt_without_a_new_slot() {
    for mode in [Mode::Unknown, Mode::Panic] {
        let disk = Arc::new(Mutex::new(Disk::default()));
        let (mut s, calls) = session(SessionController::from_game(game()), disk.clone(), false);
        bound(&mut s);
        disk.lock().unwrap().modes.push_back(mode);
        s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
        settle(&mut s);
        assert_eq!(
            s.durability(),
            Durability::Uncertain,
            "post-replacement failure must never be marked clean"
        );
        assert!(s.storage_failure().unwrap().pending().is_some());
        let intended = disk.lock().unwrap().saved.clone().unwrap();
        s.save(false).unwrap();
        settle(&mut s);
        assert_eq!(s.durability(), Durability::Clean);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(disk.lock().unwrap().saved.as_ref(), Some(&intended));
        assert_eq!(disk.lock().unwrap().writes.last(), Some(&("reconcile", 1)));
    }
}
#[test]
fn load_failure_preserves_stage_revision_binding_and_generation_retry() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(SessionController::from_game(game()), disk.clone(), true);
    bound(&mut s);
    s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
    settle(&mut s);
    let stage = s.controller().stage().clone();
    let revision = s.controller().revision();
    let binding = s.binding().clone();
    disk.lock().unwrap().saved = None;
    s.load(LoadGame {
        id: id(),
        copy: SaveCopy::Primary,
        limits: RestoreLimits::Original,
    })
    .unwrap();
    settle(&mut s);
    assert_eq!(s.controller().stage(), &stage);
    assert_eq!(s.controller().revision(), revision);
    assert_eq!(s.binding(), &binding);
    assert!(matches!(
        s.controller().failure(),
        Some(Failure::Generation(_))
    ));
    assert_eq!(disk.lock().unwrap().writes.len(), 1);
    s.dispatch(Intent::Retry).unwrap();
    settle(&mut s);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(disk.lock().unwrap().writes.len(), 1);
}
#[test]
fn selection_saves_zero_turn_before_one_deferred_opening_and_quit_clears_it() {
    for quit in [false, true] {
        let disk = Arc::new(Mutex::new(Disk::default()));
        let (mut s, calls) = session(
            SessionController::new(Limits::default(), StoryStyle::default()),
            disk.clone(),
            false,
        );
        s.dispatch(Intent::SubmitBrief(Brief::new("bell").unwrap()))
            .unwrap();
        settle(&mut s);
        s.dispatch(Intent::AcceptOutline).unwrap();
        settle(&mut s);
        assert!(disk.lock().unwrap().writes.is_empty());
        disk.lock().unwrap().modes.push_back(Mode::Fail);
        s.dispatch(Intent::Select(PlayablePosition::new(1)))
            .unwrap();
        settle(&mut s);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            s.controller()
                .game()
                .unwrap()
                .selected_world()
                .position()
                .get(),
            1
        );
        if quit {
            s.quit();
            s.quit();
            settle(&mut s);
            assert_eq!(s.shutdown(), Shutdown::DrainingOutput);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert_eq!(disk.lock().unwrap().writes, [("create", 0), ("create", 0)]);
        } else {
            s.save(false).unwrap();
            settle(&mut s);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(
                disk.lock().unwrap().writes,
                [("create", 0), ("create", 0), ("replace", 1)]
            );
        }
    }
}
#[test]
fn rewind_and_changed_policy_load_save_canonical_snapshots_before_generation() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(
        SessionController::from_game(committed(game())),
        disk.clone(),
        false,
    );
    bound(&mut s);
    s.dispatch(Intent::Rewind(TurnCount::new(1).unwrap()))
        .unwrap();
    assert!(s.dispatch(Intent::Turn(TurnDirection::Continue)).is_err());
    settle(&mut s);
    assert!(
        disk.lock()
            .unwrap()
            .saved
            .as_ref()
            .unwrap()
            .snapshot
            .game()
            .turns()
            .is_empty()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let revision = s.controller().revision();
    let mut limits = Limits::default();
    limits.max_major_events = cyoa_core::limits::MajorEventLimit::new(1).unwrap();
    s.load(LoadGame {
        id: id(),
        copy: SaveCopy::Primary,
        limits: RestoreLimits::Current(limits),
    })
    .unwrap();
    settle(&mut s);
    assert!(s.controller().revision().get() > revision.get());
    assert_eq!(s.controller().game().unwrap().limits(), limits);
    assert_eq!(
        disk.lock()
            .unwrap()
            .saved
            .as_ref()
            .unwrap()
            .snapshot
            .game()
            .limits(),
        limits
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
    settle(&mut s);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
/// Re-label the stored save with another source, as a tampered or foreign file would be.
fn retag(disk: &Arc<Mutex<Disk>>, source: StorySource) {
    let mut disk = disk.lock().unwrap();
    let saved = disk.saved.as_mut().unwrap();
    saved.snapshot = SaveSnapshot::new(saved.snapshot.game().clone(), source).unwrap();
}
#[test]
fn source_mismatch_preserves_session_and_backup_recovery_creates_instead_of_replacing() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(SessionController::from_game(game()), disk.clone(), false);
    bound(&mut s);
    let before = s.controller().stage().clone();
    let binding = s.binding().clone();
    retag(
        &disk,
        StorySource::Demo {
            scenario: DemoScenarioId::HarbourV1,
        },
    );
    s.load(LoadGame {
        id: id(),
        copy: SaveCopy::Primary,
        limits: RestoreLimits::Original,
    })
    .unwrap();
    settle(&mut s);
    assert_eq!(s.controller().stage(), &before);
    assert_eq!(s.binding(), &binding);
    assert_eq!(disk.lock().unwrap().writes.len(), 1);
    retag(&disk, StorySource::Live);
    s.load(LoadGame {
        id: id(),
        copy: SaveCopy::Backup,
        limits: RestoreLimits::Original,
    })
    .unwrap();
    settle(&mut s);
    assert_eq!(disk.lock().unwrap().writes, [("create", 0), ("create", 0)]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
fn wait_started(flag: &AtomicBool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !flag.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn quit_during_write_finishes_current_revision_once_without_duplicate_backup_rotation() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(SessionController::from_game(game()), disk.clone(), false);
    bound(&mut s);
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    disk.lock().unwrap().wait_write = Some((started.clone(), release.clone()));
    s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !s.storage_busy() {
        s.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    wait_started(&started);
    s.quit();
    s.quit();
    s.poll();
    assert_eq!(s.shutdown(), Shutdown::StoppingGeneration);
    assert!(s.storage_busy());
    assert!(s.save(false).is_err());
    release.store(true, Ordering::SeqCst);
    settle(&mut s);
    assert_eq!(s.shutdown(), Shutdown::DrainingOutput);
    assert!(!s.exit_failed());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(disk.lock().unwrap().writes, [("create", 0), ("replace", 1)]);
}
#[test]
fn quit_cancels_load_joins_and_saves_old_canonical_game_once() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(
        SessionController::from_game(committed(game())),
        disk.clone(),
        false,
    );
    bound(&mut s);
    let before = s.controller().game().unwrap().clone();
    let revision = s.controller().revision();
    let started = Arc::new(AtomicBool::new(false));
    disk.lock().unwrap().wait_load = Some(started.clone());
    s.load(LoadGame {
        id: id(),
        copy: SaveCopy::Primary,
        limits: RestoreLimits::Original,
    })
    .unwrap();
    wait_started(&started);
    s.quit();
    s.quit();
    settle(&mut s);
    assert_eq!(s.controller().game().unwrap(), &before);
    assert_eq!(s.controller().revision(), revision);
    assert_eq!(disk.lock().unwrap().writes, [("create", 1), ("replace", 1)]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(s.shutdown(), Shutdown::DrainingOutput);
    assert!(!s.exit_failed());
}
#[test]
fn generation_and_storage_failure_views_remain_independent_and_copy_failure_keeps_binding() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(SessionController::from_game(game()), disk.clone(), true);
    bound(&mut s);
    s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
    settle(&mut s);
    let binding = s.binding().clone();
    let revision = s.controller().revision();
    disk.lock().unwrap().modes.push_back(Mode::Fail);
    s.save(true).unwrap();
    settle(&mut s);
    assert_eq!(s.binding(), &binding);
    assert_eq!(s.controller().revision(), revision);
    assert!(s.storage_failure().is_some());
    assert!(matches!(
        s.controller().failure(),
        Some(Failure::Generation(_))
    ));
    s.save(true).unwrap();
    settle(&mut s);
    assert!(s.storage_failure().is_none());
    assert!(matches!(
        s.controller().failure(),
        Some(Failure::Generation(_))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn cancelled_generation_never_autosaves_a_rejected_turn() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, _) = session(SessionController::from_game(game()), disk.clone(), false);
    bound(&mut s);
    let before = s.controller().game().unwrap().clone();
    let revision = s.controller().revision();
    s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
    s.dispatch(Intent::Cancel).unwrap();
    settle(&mut s);
    assert_eq!(s.controller().game(), Some(&before));
    assert_eq!(s.controller().revision(), revision);
    assert_eq!(s.durability(), Durability::Clean);
    assert_eq!(
        disk.lock().unwrap().writes,
        [("create", 0)],
        "unaccepted generation must never autosave"
    );
    s.dispatch(Intent::Retry).unwrap();
    settle(&mut s);
    assert_eq!(s.controller().game().unwrap().turns().len(), 1);
    assert_eq!(disk.lock().unwrap().writes, [("create", 0), ("replace", 1)]);
}

#[test]
fn failed_copy_retains_previous_uncertain_attempt_for_reconciliation() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(SessionController::from_game(game()), disk.clone(), false);
    bound(&mut s);
    disk.lock().unwrap().modes.push_back(Mode::Unknown);
    s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
    settle(&mut s);
    let saved = disk.lock().unwrap().saved.clone().unwrap();
    disk.lock().unwrap().modes.push_back(Mode::Fail);
    s.save(true).unwrap();
    settle(&mut s);
    assert_eq!(s.durability(), Durability::Uncertain);
    s.save(false).unwrap();
    settle(&mut s);
    assert_eq!(s.durability(), Durability::Clean);
    assert_eq!(disk.lock().unwrap().saved.as_ref(), Some(&saved));
    assert_eq!(disk.lock().unwrap().writes.last(), Some(&("reconcile", 1)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn queued_generation_panic_remains_an_exit_failure_after_quit_and_successful_save() {
    let disk = Arc::new(Mutex::new(Disk::default()));
    let (mut s, calls) = session(
        SessionController::from_game(game()),
        disk.clone(),
        GeneratorMode::Panic,
    );
    bound(&mut s);
    s.dispatch(Intent::Turn(TurnDirection::Continue)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while calls.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    // No completion has been polled; quit invalidates acceptance first.
    s.quit();
    settle(&mut s);
    assert_eq!(s.shutdown(), Shutdown::DrainingOutput);
    assert_eq!(s.durability(), Durability::Clean);
    assert!(s.exit_failed());
    assert!(s.controller().game().unwrap().turns().is_empty());
    assert_eq!(disk.lock().unwrap().writes, [("create", 0), ("replace", 0)]);
}
