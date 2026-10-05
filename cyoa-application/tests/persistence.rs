use cyoa_application::{
    cancellation::{CancellationSource, CancellationToken},
    persistence::*,
};
use cyoa_core::{game::GameState, limits::*, style::StoryStyle, text::*, world::*};
fn id() -> SaveId {
    SaveId::new("story-0123456789abcdef0123456789abcdef").unwrap()
}
fn stored() -> StoredGame {
    let cast = WorldCast::restore(
        vec![PlayerCharacter::new(
            CharacterName::new("Player").unwrap(),
            CharacterDescription::new("A sailor").unwrap(),
            Backstory::new("At sea").unwrap(),
        )],
        vec![],
    )
    .unwrap();
    let world = World::new(
        WorldOutline::new(
            WorldTitle::new("Story").unwrap(),
            WorldDescription::new("Harbour").unwrap(),
        ),
        cast,
    );
    let game = GameState::start(
        Brief::new("Sail").unwrap(),
        world.select(PlayablePosition::new(0)).unwrap(),
        StoryStyle::default(),
        Limits::default(),
    );
    StoredGame {
        snapshot: SaveSnapshot::new(game, StorySource::Live).unwrap(),
        metadata: SaveMetadata {
            id: id(),
            revision: SaveRevision::new(1).unwrap(),
            saved_at: SavedAt::new(0, 0).unwrap(),
        },
        stamp: ContentStamp::new([1; 32]),
        copy: SaveCopy::Primary,
        unrecognized_fields: vec![],
    }
}

struct FakeRepository {
    calls: Vec<StorageOperation>,
    loaded: StoredGame,
    failure: Option<StorageFailure>,
    cancel_on_call: Option<std::sync::Arc<CancellationSource>>,
}
impl FakeRepository {
    fn new() -> Self {
        Self {
            calls: vec![],
            loaded: stored(),
            failure: None,
            cancel_on_call: None,
        }
    }
    fn call(&mut self, op: StorageOperation) -> Result<(), StorageFailure> {
        self.calls.push(op);
        if let Some(source) = &self.cancel_on_call {
            source.cancel();
        }
        match &self.failure {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
    fn receipt(&self) -> SaveReceipt {
        SaveReceipt {
            metadata: self.loaded.metadata.clone(),
            stamp: self.loaded.stamp,
        }
    }
}
impl GameRepository for FakeRepository {
    fn create(
        &mut self,
        s: SaveSnapshot,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        assert_eq!(s, self.loaded.snapshot);
        self.call(StorageOperation::Create)?;
        Ok(self.receipt())
    }
    fn replace(
        &mut self,
        t: SaveTarget,
        s: SaveSnapshot,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        assert_eq!(s, self.loaded.snapshot);
        assert_eq!(t.id, id());
        assert_eq!(t.expected_stamp, self.loaded.stamp);
        self.call(StorageOperation::Replace)?;
        Ok(self.receipt())
    }
    fn reconcile(
        &mut self,
        t: PendingWrite,
        prepared: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        assert_eq!(t.target(), &id());
        assert_eq!(t.bytes(), b"prepared");
        prepared.publish(t.clone());
        self.call(StorageOperation::Reconcile)?;
        Ok(self.receipt())
    }
    fn load(
        &mut self,
        i: &SaveId,
        c: SaveCopy,
        _: &CancellationToken,
    ) -> Result<StoredGame, StorageFailure> {
        assert_eq!(i, &id());
        self.call(StorageOperation::Load)?;
        let mut loaded = self.loaded.clone();
        loaded.copy = c;
        Ok(loaded)
    }
    fn list(
        &mut self,
        p: SavePage,
        _: &CancellationToken,
    ) -> Result<SavePageResult, StorageFailure> {
        assert_eq!(p.limit(), 2);
        assert_eq!(p.after(), Some(&id()));
        self.call(StorageOperation::List)?;
        Ok(SavePageResult {
            entries: vec![SaveListing {
                id: id(),
                status: SaveListingStatus::Busy,
                backup_available: true,
            }],
            next: Some(id()),
        })
    }
}
fn failure() -> StorageFailure {
    StorageFailure {
        operation: StorageOperation::Replace,
        stage: StorageStage::Sync,
        kind: StorageFailureKind::Io,
        message: "directory sync failed".into(),
        visibility: WriteVisibility::Replaced {
            stamp: ContentStamp::new([2; 32]),
        },
        pending: Some(Box::new(pending())),
        cleanup_errors: vec!["cleanup failed".into()].into_boxed_slice(),
    }
}
fn pending() -> PendingWrite {
    PendingWrite::new(
        id(),
        Some(ContentStamp::new([1; 32])),
        ContentStamp::new([2; 32]),
        b"prepared".to_vec(),
    )
    .unwrap()
}
#[test]
fn repository_failures_are_preserved_without_retry_or_changing_the_owned_snapshot() {
    let before = stored();
    let mut repo = FakeRepository::new();
    repo.failure = Some(failure());
    let mut cases = PersistenceUseCases::new(repo);
    let token = CancellationSource::default().token();
    let error = cases
        .save_game(
            SaveGame::Replace {
                target: SaveTarget {
                    id: id(),
                    expected_stamp: before.stamp,
                },
                snapshot: before.snapshot.clone(),
            },
            &PreparedWriteEvidence::default(),
            &token,
        )
        .unwrap_err();
    assert_eq!(error, failure());
    assert_eq!(cases.into_repository().calls, [StorageOperation::Replace]);
    assert_eq!(before, stored());
    for op in [
        StorageOperation::Load,
        StorageOperation::Inspect,
        StorageOperation::List,
    ] {
        let mut repo = FakeRepository::new();
        repo.failure = Some(failure());
        let mut cases = PersistenceUseCases::new(repo);
        let error = match op {
            StorageOperation::Load => cases
                .load_game(
                    LoadGame {
                        id: id(),
                        copy: SaveCopy::Primary,
                        limits: RestoreLimits::Original,
                    },
                    &token,
                )
                .unwrap_err(),
            StorageOperation::Inspect => cases
                .inspect_save(
                    InspectSave {
                        id: id(),
                        copy: SaveCopy::Backup,
                    },
                    &token,
                )
                .unwrap_err(),
            _ => cases
                .list_saves(
                    ListSaves {
                        page: SavePage::new(Some(id()), 2).unwrap(),
                    },
                    &token,
                )
                .unwrap_err(),
        };
        assert_eq!(error, failure());
        assert_eq!(cases.into_repository().calls.len(), 1);
    }
}
#[test]
fn pre_cancelled_commands_and_queries_never_call_the_repository() {
    let source = CancellationSource::default();
    source.cancel();
    let token = source.token();
    let mut cases = PersistenceUseCases::new(FakeRepository::new());
    let snapshot = stored().snapshot;
    let target = SaveTarget {
        id: id(),
        expected_stamp: ContentStamp::new([1; 32]),
    };
    for cmd in [
        SaveGame::Create(snapshot.clone()),
        SaveGame::Replace { target, snapshot },
        SaveGame::Reconcile(pending()),
    ] {
        assert_eq!(
            cases
                .save_game(cmd, &PreparedWriteEvidence::default(), &token)
                .unwrap_err()
                .kind,
            StorageFailureKind::Cancelled
        );
    }
    assert!(
        cases
            .load_game(
                LoadGame {
                    id: id(),
                    copy: SaveCopy::Primary,
                    limits: RestoreLimits::Original
                },
                &token
            )
            .is_err()
    );
    assert!(
        cases
            .inspect_save(
                InspectSave {
                    id: id(),
                    copy: SaveCopy::Primary
                },
                &token
            )
            .is_err()
    );
    assert!(
        cases
            .list_saves(
                ListSaves {
                    page: SavePage::new(Some(id()), 2).unwrap()
                },
                &token
            )
            .is_err()
    );
    assert!(cases.into_repository().calls.is_empty());
}
#[test]
fn late_read_cancellation_rejects_results_but_never_hides_a_durable_write_receipt() {
    for op in [
        StorageOperation::Load,
        StorageOperation::Inspect,
        StorageOperation::List,
        StorageOperation::Create,
    ] {
        let source = std::sync::Arc::new(CancellationSource::default());
        let mut repo = FakeRepository::new();
        repo.cancel_on_call = Some(source.clone());
        let mut cases = PersistenceUseCases::new(repo);
        let token = source.token();
        match op {
            StorageOperation::Load => assert_eq!(
                cases
                    .load_game(
                        LoadGame {
                            id: id(),
                            copy: SaveCopy::Primary,
                            limits: RestoreLimits::Original
                        },
                        &token
                    )
                    .unwrap_err()
                    .kind,
                StorageFailureKind::Cancelled
            ),
            StorageOperation::Inspect => assert_eq!(
                cases
                    .inspect_save(
                        InspectSave {
                            id: id(),
                            copy: SaveCopy::Primary
                        },
                        &token
                    )
                    .unwrap_err()
                    .kind,
                StorageFailureKind::Cancelled
            ),
            StorageOperation::List => assert_eq!(
                cases
                    .list_saves(
                        ListSaves {
                            page: SavePage::new(Some(id()), 2).unwrap()
                        },
                        &token
                    )
                    .unwrap_err()
                    .kind,
                StorageFailureKind::Cancelled
            ),
            _ => assert_eq!(
                cases
                    .save_game(
                        SaveGame::Create(stored().snapshot),
                        &PreparedWriteEvidence::default(),
                        &token
                    )
                    .unwrap()
                    .stamp,
                stored().stamp
            ),
        }
        assert_eq!(cases.into_repository().calls.len(), 1);
    }
}
#[test]
fn checked_persistence_values_reject_invalid_names_counters_pages_and_bounds() {
    for name in [
        "../story",
        "story",
        "Story-0123456789abcdef0123456789abcdef",
        "-0123456789abcdef0123456789abcdef",
        "two--words-0123456789abcdef0123456789abcdef",
        "story-0123456789abcdef0123456789abcdeF",
    ] {
        assert!(SaveId::new(name).is_err());
    }
    assert!(SaveRevision::new(0).is_err());
    assert!(StorageRequestId::new(0).is_err());
    assert!(SaveRevision::new(u64::MAX).unwrap().next().is_err());
    assert!(StorageRequestId::new(u64::MAX).unwrap().next().is_err());
    assert_eq!(SaveRevision::new(1).unwrap().next().unwrap().get(), 2);
    assert_eq!(StorageRequestId::new(1).unwrap().next().unwrap().get(), 2);
    assert!(SavePage::new(None, 0).is_err());
    assert!(SavePage::new(None, 101).is_err());
    assert!(SavePage::new(None, 100).is_ok());
    assert!(SavedAt::new(0, 1_000_000_000).is_err());
    assert!(SavedAt::new(-62_135_596_801, 0).is_err());
    assert!(SavedAt::new(253_402_300_800, 0).is_err());
    assert_eq!(
        SavedAt::new(0, 123_456_789).unwrap().nanoseconds(),
        123_000_000
    );
    assert!(PendingWrite::new(id(), None, ContentStamp::new([0; 32]), vec![]).is_err());
    assert_eq!(pending().previous_stamp(), Some(ContentStamp::new([1; 32])));
    assert_eq!(pending().intended_stamp(), ContentStamp::new([2; 32]));
}
#[test]
fn explicit_restore_policy_and_read_only_queries_do_not_write_or_rebind_the_disk_stamp() {
    let mut cases = PersistenceUseCases::new(FakeRepository::new());
    let token = CancellationSource::default().token();
    let original = cases
        .load_game(
            LoadGame {
                id: id(),
                copy: SaveCopy::Primary,
                limits: RestoreLimits::Original,
            },
            &token,
        )
        .unwrap();
    assert!(!original.changed_by_restore);
    let limits = Limits {
        max_major_events: MajorEventLimit::new(1).unwrap(),
        max_generated_npcs: MaxGeneratedNpcs::new(0),
        prose_bridge_turns: ProseBridgeTurns::new(0),
        min_playable_characters: MinPlayableCharacters::new(9).unwrap(),
    };
    let narrowed = cases
        .load_game(
            LoadGame {
                id: id(),
                copy: SaveCopy::Backup,
                limits: RestoreLimits::Current(limits),
            },
            &token,
        )
        .unwrap();
    assert!(narrowed.changed_by_restore);
    assert_eq!(narrowed.stored.snapshot.game().limits(), limits);
    assert_eq!(
        narrowed.stored.snapshot.game().original_limits(),
        Limits::default()
    );
    assert_eq!(narrowed.stored.stamp, stored().stamp);
    assert_eq!(narrowed.stored.copy, SaveCopy::Backup);
    let inspected = cases
        .inspect_save(
            InspectSave {
                id: id(),
                copy: SaveCopy::Primary,
            },
            &token,
        )
        .unwrap();
    assert_eq!(inspected, stored());
    let listed = cases
        .list_saves(
            ListSaves {
                page: SavePage::new(Some(id()), 2).unwrap(),
            },
            &token,
        )
        .unwrap();
    assert_eq!(listed.entries[0].status, SaveListingStatus::Busy);
    assert_eq!(listed.next, Some(id()));
    assert_eq!(
        cases.into_repository().calls,
        [
            StorageOperation::Load,
            StorageOperation::Load,
            StorageOperation::Load,
            StorageOperation::List
        ]
    );
}
#[test]
fn explicit_storage_commands_make_exactly_one_call_each_and_return_observed_receipts() {
    let mut cases = PersistenceUseCases::new(FakeRepository::new());
    let token = CancellationSource::default().token();
    for cmd in [
        SaveGame::Create(stored().snapshot),
        SaveGame::Replace {
            target: SaveTarget {
                id: id(),
                expected_stamp: stored().stamp,
            },
            snapshot: stored().snapshot,
        },
        SaveGame::Reconcile(pending()),
    ] {
        let receipt = cases
            .save_game(cmd, &PreparedWriteEvidence::default(), &token)
            .unwrap();
        assert_eq!(receipt.metadata, stored().metadata);
        assert_eq!(receipt.stamp, stored().stamp);
    }
    assert_eq!(
        cases.into_repository().calls,
        [
            StorageOperation::Create,
            StorageOperation::Replace,
            StorageOperation::Reconcile
        ]
    );
}
#[test]
fn writes_forward_the_callers_own_preparation_sink_to_the_repository() {
    let mut cases = PersistenceUseCases::new(FakeRepository::new());
    let token = CancellationSource::default().token();
    let caller = PreparedWriteEvidence::default();
    cases
        .save_game(SaveGame::Reconcile(pending()), &caller.clone(), &token)
        .unwrap();
    // The repository published through a clone; the caller's handle observes it.
    assert_eq!(caller.pending(), Some(pending()));
}
#[test]
fn demo_sources_admit_at_most_their_scenarios_passages_and_live_admits_any() {
    let demo = StorySource::Demo {
        scenario: DemoScenarioId::HarbourV1,
    };
    let passages = DemoScenarioId::HarbourV1.passages().get();
    assert_eq!(passages, 5);
    assert!((0..=passages).all(|turns| demo.admits(turns)));
    assert!(!demo.admits(passages + 1));
    assert!(StorySource::Live.admits(usize::MAX));
    let game = stored().snapshot.into_game();
    let snapshot = SaveSnapshot::new(game.clone(), demo).unwrap();
    assert_eq!((snapshot.game(), snapshot.source()), (&game, demo));
}
