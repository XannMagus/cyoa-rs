use cyoa_application::{cancellation::CancellationToken, persistence::*};
use cyoa_core::{limits::Limits, style::StoryStyle};
use cyoa_presentation::{session::SessionController, storage::*};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn id() -> SaveId {
    SaveId::new("story-0123456789abcdef0123456789abcdef").unwrap()
}
fn pending() -> PendingWrite {
    PendingWrite::new(id(), None, ContentStamp::new([1; 32]), b"prepared".to_vec()).unwrap()
}
fn key() -> StorageKey {
    StorageKey {
        id: StorageRequestId::new(1).unwrap(),
        revision: SessionController::new(Limits::default(), StoryStyle::default()).revision(),
    }
}
struct Fake {
    evidence: PreparedWriteEvidence,
    mode: u8,
    started: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}
fn cancelled(op: StorageOperation) -> StorageFailure {
    StorageFailure::cancelled(op)
}
impl GameRepository for Fake {
    fn create(
        &mut self,
        _: SaveSnapshot,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        unreachable!()
    }
    fn replace(
        &mut self,
        _: SaveTarget,
        _: SaveSnapshot,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        unreachable!()
    }
    fn reconcile(
        &mut self,
        attempt: PendingWrite,
        token: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        if self.mode != 2 {
            self.evidence.publish(attempt.clone());
        }
        self.started.store(true, Ordering::SeqCst);
        if self.mode > 0 {
            panic!("controlled storage panic");
        }
        while !token.is_cancelled() {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.done.store(true, Ordering::SeqCst);
        let mut error = cancelled(StorageOperation::Reconcile);
        error.visibility = WriteVisibility::Unknown;
        error.pending = Some(Box::new(attempt));
        Err(error)
    }
    fn load(
        &mut self,
        _: &SaveId,
        _: SaveCopy,
        _: &CancellationToken,
    ) -> Result<StoredGame, StorageFailure> {
        unreachable!()
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
fn wait(flag: &AtomicBool) {
    let end = Instant::now() + Duration::from_secs(2);
    while !flag.load(Ordering::SeqCst) {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn polling_is_nonblocking_preparation_precedes_completion_and_close_joins_cancelled_work() {
    let started = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let start = started.clone();
    let finish = done.clone();
    let mut runner = StorageRunner::new(move |evidence| Fake {
        evidence,
        mode: 0,
        started: start.clone(),
        done: finish.clone(),
    });
    runner
        .start(
            key(),
            StorageIntent::Save(Box::new(SaveGame::Reconcile(pending()))),
        )
        .unwrap();
    wait(&started);
    let begin = Instant::now();
    let events = runner.poll();
    assert!(begin.elapsed() < Duration::from_millis(100));
    assert!(
        matches!(&events[..],[StorageEvent::Prepared{key:k,pending:p}]if *k==key()&&*p==pending())
    );
    assert!(!runner.is_ready());
    assert!(
        runner
            .start(
                key(),
                StorageIntent::List(ListSaves {
                    page: SavePage::new(None, 1).unwrap()
                })
            )
            .is_err()
    );
    runner.close();
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(StorageEvent::Finished {
            key: k,
            result: Err(error),
        }) = runner.poll().pop()
        {
            assert_eq!(k, key());
            assert_eq!(error.kind, StorageFailureKind::Cancelled);
            assert_eq!(error.pending.as_deref(), Some(&pending()));
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(runner.is_closed());
    assert!(done.load(Ordering::SeqCst));
}
#[test]
fn worker_panics_distinguish_unprepared_from_uncertain_writes_and_retain_identity() {
    for mode in [1, 2] {
        let started = Arc::new(AtomicBool::new(false));
        let start = started.clone();
        let mut runner = StorageRunner::new(move |evidence| Fake {
            evidence,
            mode,
            started: start.clone(),
            done: Arc::new(AtomicBool::new(false)),
        });
        runner
            .start(
                key(),
                StorageIntent::Save(Box::new(SaveGame::Reconcile(pending()))),
            )
            .unwrap();
        wait(&started);
        let end = Instant::now() + Duration::from_secs(2);
        let mut prepared = false;
        loop {
            let events = runner.poll();
            for event in events {
                match event {
                    StorageEvent::Prepared { pending: p, .. } => {
                        assert_eq!(p, pending());
                        prepared = true;
                    }
                    StorageEvent::Finished {
                        result: Err(error), ..
                    } => {
                        assert_eq!(error.kind, StorageFailureKind::WorkerFault);
                        assert_eq!(
                            error.visibility,
                            if mode == 1 {
                                WriteVisibility::Unknown
                            } else {
                                WriteVisibility::Unchanged
                            }
                        );
                        assert_eq!(prepared, mode == 1);
                        assert_eq!(error.pending.is_some(), mode == 1);
                        assert!(!runner.is_ready());
                        runner.close();
                        assert!(runner.is_closed());
                        break;
                    }
                    _ => panic!("unexpected storage success"),
                }
            }
            if runner.is_closed() {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
#[test]
fn dropping_runner_cancels_and_joins_without_leaving_storage_work_detached() {
    let started = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let start = started.clone();
    let finish = done.clone();
    let mut runner = StorageRunner::new(move |evidence| Fake {
        evidence,
        mode: 0,
        started: start.clone(),
        done: finish.clone(),
    });
    runner
        .start(
            key(),
            StorageIntent::Save(Box::new(SaveGame::Reconcile(pending()))),
        )
        .unwrap();
    wait(&started);
    drop(runner);
    assert!(done.load(Ordering::SeqCst));
}
