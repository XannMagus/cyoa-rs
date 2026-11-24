// Private coordinator state is exercised here to inject otherwise unreachable
// counter boundaries and stale/duplicate responses without weakening admission.
use super::*;
struct TestGeneration;
impl GenerationExecution for TestGeneration {
    fn start(&mut self, _: crate::session::WorkRequest) -> std::io::Result<()> {
        unreachable!()
    }
    fn poll(&mut self) -> Vec<crate::execution::WorkerEvent> {
        vec![]
    }
}
#[derive(Default)]
struct TestStorage {
    events: Vec<StorageEvent>,
}
impl StorageExecution for TestStorage {
    fn start(&mut self, key: StorageKey, intent: StorageIntent) -> std::io::Result<()> {
        assert!(matches!(intent, StorageIntent::Save(_)));
        self.events.push(StorageEvent::Finished {
            key,
            result: Ok(StorageOutcome::Saved(receipt())),
        });
        Ok(())
    }
    fn recover_fault(&mut self) {}
    fn cancel(&self) {}
    fn close(&mut self) {}
    fn poll(&mut self) -> Vec<StorageEvent> {
        std::mem::take(&mut self.events)
    }
}
#[path = "../tests/support/mod.rs"]
mod support;
use crate::session::SessionController;
fn receipt() -> SaveReceipt {
    SaveReceipt {
        metadata: SaveMetadata {
            id: SaveId::new("story-0123456789abcdef0123456789abcdef").unwrap(),
            revision: SaveRevision::new(1).unwrap(),
            saved_at: SavedAt::new(1, 0).unwrap(),
        },
        stamp: ContentStamp::new([1; 32]),
    }
}
fn session() -> PersistedSession<TestGeneration, TestStorage> {
    PersistedSession::new(
        SessionRuntime::new(
            SessionController::from_game(support::game()),
            TestGeneration,
        ),
        TestStorage::default(),
        StorySource::Live,
    )
}
#[test]
fn stale_duplicate_or_wrong_revision_storage_receipts_never_clean_or_rebind() {
    let mut s = session();
    let key = StorageKey {
        id: StorageRequestId::new(2).unwrap(),
        revision: s.view().revision(),
    };
    s.running = Some(Running {
        key,
        effect: Effect::Save,
        pending: None,
        prior_durability: Durability::Dirty,
    });
    let stale = StorageKey {
        id: StorageRequestId::new(1).unwrap(),
        ..key
    };
    assert!(
        matches!(
            s.accept_storage(StorageEvent::Finished {
                key: stale,
                result: Ok(StorageOutcome::Saved(receipt()))
            })
            .as_slice(),
            [PersistenceEvent::Ignored]
        ),
        "stale storage receipt must never clean or rebind canonical state"
    );
    assert_eq!(s.durability, Durability::Dirty);
    assert_eq!(s.binding, SaveBinding::Unbound);
    assert!(s.running.is_some());
    assert!(
        s.storage_failure().is_some(),
        "stale physical write diagnostics retained"
    );
    // Load advances the existing revision; old storage evidence cannot bind it.
    s.runtime
        .replace_game(support::committed(support::game()))
        .unwrap();
    assert!(matches!(
        s.accept_storage(StorageEvent::Finished {
            key,
            result: Ok(StorageOutcome::Saved(receipt()))
        })
        .as_slice(),
        [PersistenceEvent::Ignored]
    ));
    assert_eq!(s.durability, Durability::Dirty);
    assert_eq!(s.binding, SaveBinding::Unbound);
    let fresh = StorageKey {
        revision: s.view().revision(),
        id: StorageRequestId::new(3).unwrap(),
    };
    s.running = Some(Running {
        key: fresh,
        effect: Effect::Save,
        pending: None,
        prior_durability: Durability::Dirty,
    });
    s.accept_storage(StorageEvent::Finished {
        key: fresh,
        result: Ok(StorageOutcome::Saved(receipt())),
    });
    let binding = s.binding.clone();
    assert_eq!(s.durability, Durability::Clean);
    assert!(matches!(
        s.accept_storage(StorageEvent::Finished {
            key: fresh,
            result: Err(StorageFailure::cancelled(StorageOperation::Replace))
        })
        .as_slice(),
        [PersistenceEvent::Ignored]
    ));
    assert_eq!(s.durability, Durability::Clean);
    assert_eq!(s.binding, binding);
}
#[test]
fn storage_counter_exhaustion_rejects_admission_but_reserves_final_quit_save() {
    let mut s = session();
    s.durability = Durability::Clean;
    s.last_storage_id = Some(StorageRequestId::new(u64::MAX - 1).unwrap());
    assert!(matches!(
        s.save(SaveSlot::Current),
        Err(CoordinationError::Exhausted)
    ));
    assert!(matches!(
        s.dispatch(Intent::Turn(TurnDirection::Continue)),
        Err(CoordinationError::Exhausted)
    ));
    assert_eq!(s.view().phase(), Phase::Ready);
    assert!(s.running.is_none());
    s.quit();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while s.shutdown() != Shutdown::Complete {
        s.poll();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(s.last_storage_id.map(StorageRequestId::get), Some(u64::MAX));
    assert_eq!(s.durability, Durability::Clean);
    assert!(!s.exit_failed());
    s.quit();
    assert_eq!(s.last_storage_id.map(StorageRequestId::get), Some(u64::MAX));
}
#[test]
fn deferred_opening_and_load_reserve_their_followup_write_before_admission() {
    let mut s = session();
    s.durability = Durability::Clean;
    s.last_storage_id = Some(StorageRequestId::new(u64::MAX - 2).unwrap());
    let before = s.view().stage().clone();
    let revision = s.view().revision();
    assert!(matches!(
        s.load(LoadGame {
            id: receipt().metadata.id,
            copy: SaveCopy::Primary,
            limits: cyoa_core::limits::RestoreLimits::Original
        }),
        Err(CoordinationError::Exhausted)
    ));
    assert_eq!(s.view().stage(), &before);
    assert_eq!(s.view().revision(), revision);
    s.deferred_opening = true;
    assert!(matches!(
        s.save(SaveSlot::Current),
        Err(CoordinationError::Exhausted)
    ));
    assert!(s.running.is_none());
}
