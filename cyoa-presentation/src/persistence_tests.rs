// Private coordinator state is exercised here to inject otherwise unreachable
// counter boundaries and stale/duplicate responses without weakening admission.
use super::*;
#[path = "../tests/support/mod.rs"]
mod support;
use cyoa_application::{
    cancellation::CancellationToken,
    generation::{Generated, GenerationFailure, StoryUseCases},
};
use cyoa_core::{
    game::GameState,
    limits::Limits,
    text::Brief,
    turn::StoryTurn,
    world::{WorldCast, WorldOutline},
};
struct Generator;
impl StoryGenerator for Generator {
    fn outline(
        &mut self,
        _: &Brief,
        _: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        unreachable!()
    }
    fn cast(
        &mut self,
        _: &Brief,
        _: &WorldOutline,
        _: &Limits,
        _: &CancellationToken,
    ) -> Result<Generated<WorldCast>, GenerationFailure> {
        unreachable!()
    }
    fn turn(
        &mut self,
        _: &GameState,
        _: &TurnDirection,
        _: &CancellationToken,
        _: &mut dyn FnMut(&str),
    ) -> Result<Generated<StoryTurn>, GenerationFailure> {
        unreachable!()
    }
}
struct Repository;
impl GameRepository for Repository {
    fn create(
        &mut self,
        _: SaveSnapshot,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        Ok(receipt())
    }
    fn replace(
        &mut self,
        _: SaveTarget,
        _: SaveSnapshot,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        Ok(receipt())
    }
    fn reconcile(
        &mut self,
        _: PendingWrite,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        Ok(receipt())
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
        unreachable!()
    }
}
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
fn session() -> PersistedSession<Generator, Repository, impl Fn() -> Repository> {
    PersistedSession::new(
        SessionRuntime::new(
            SessionController::from_game(support::game()),
            StoryUseCases::new(Generator),
            crate::worker::PreviewLimit::default(),
        ),
        StorageRunner::new(|| Repository),
        StorySource::Live,
    )
}
#[test]
fn stale_duplicate_or_wrong_revision_storage_receipts_never_clean_or_rebind() {
    let mut s = session();
    let key = StorageKey {
        id: StorageRequestId::new(2).unwrap(),
        revision: s.controller().revision(),
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
        revision: s.controller().revision(),
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
    assert_eq!(s.controller().phase(), Phase::Ready);
    assert!(s.running.is_none());
    s.quit();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while s.shutdown() != Shutdown::DrainingOutput {
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
    let before = s.controller().stage().clone();
    let revision = s.controller().revision();
    assert!(matches!(
        s.load(LoadGame {
            id: receipt().metadata.id,
            copy: SaveCopy::Primary,
            limits: cyoa_core::limits::RestoreLimits::Original
        }),
        Err(CoordinationError::Exhausted)
    ));
    assert_eq!(s.controller().stage(), &before);
    assert_eq!(s.controller().revision(), revision);
    s.deferred_opening = true;
    assert!(matches!(
        s.save(SaveSlot::Current),
        Err(CoordinationError::Exhausted)
    ));
    assert!(s.running.is_none());
}
