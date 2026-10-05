use cyoa_application::{
    cancellation::{CancellationSource, CancellationToken},
    generation::*,
    persistence::*,
};
use cyoa_core::{game::TurnCount, limits::*};
use cyoa_infrastructure::{
    generation::{
        engine::GenerationEngine, scripted::ScriptedBackend, templates::GenerationTemplates,
    },
    persistence::codec::{decode, encode},
};
use serde_json::Value;
fn id() -> SaveId {
    SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap()
}
struct ReadRepository(StoredGame);
impl GameRepository for ReadRepository {
    fn load(
        &mut self,
        id: &SaveId,
        copy: SaveCopy,
        _: &CancellationToken,
    ) -> Result<StoredGame, StorageFailure> {
        assert_eq!(id, &self.0.metadata.id);
        let mut stored = self.0.clone();
        stored.copy = copy;
        Ok(stored)
    }
    fn create(
        &mut self,
        _: SaveSnapshot,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        panic!("load policy must not write automatically")
    }
    fn replace(
        &mut self,
        _: SaveTarget,
        _: SaveSnapshot,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        panic!("load policy must not write automatically")
    }
    fn reconcile(
        &mut self,
        _: PendingWrite,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        panic!("unexpected reconcile")
    }
    fn list(
        &mut self,
        _: SavePage,
        _: &CancellationToken,
    ) -> Result<SavePageResult, StorageFailure> {
        panic!("unexpected list")
    }
}
fn load(bytes: &[u8], policy: RestoreLimits) -> LoadedGame {
    let stored = decode(bytes, &id(), SaveCopy::Primary).unwrap();
    let mut cases = PersistenceUseCases::new(ReadRepository(stored));
    cases
        .load_game(
            LoadGame {
                id: id(),
                copy: SaveCopy::Primary,
                limits: policy,
            },
            &CancellationSource::default().token(),
        )
        .unwrap()
}
fn narrowed() -> Limits {
    Limits {
        max_major_events: MajorEventLimit::new(1).unwrap(),
        max_generated_npcs: MaxGeneratedNpcs::new(0),
        prose_bridge_turns: ProseBridgeTurns::new(0),
        min_playable_characters: MinPlayableCharacters::new(9).unwrap(),
    }
}
fn snapshots(game: &cyoa_core::game::GameState) -> Vec<Vec<String>> {
    game.turns()
        .iter()
        .map(|t| {
            t.summary()
                .major_events()
                .events()
                .iter()
                .map(|e| e.as_str().into())
                .collect()
        })
        .collect()
}
#[test]
fn repeated_codec_restore_cycles_rebind_every_snapshot_without_resurrecting_events_or_pruning_casts()
 {
    let bytes = include_bytes!("fixtures/saves/v1-full-story.json");
    let current = load(bytes, RestoreLimits::Current(narrowed()));
    let game = &current.stored.snapshot.game;
    let creation = game.original_limits();
    assert!(current.changed_by_restore);
    assert_eq!(creation.max_major_events.get(), 4);
    assert_eq!(game.limits(), narrowed());
    assert_eq!(
        snapshots(game),
        [vec!["B"], vec!["C"], vec!["D"], vec!["E"], vec!["F"]]
    );
    assert_eq!(game.world().cast().playable().len(), 2);
    assert_eq!(game.world().cast().npcs().len(), 2);
    assert_eq!(game.prose_context().0.len(), 0);
    for t in game.turns() {
        assert_eq!(t.summary().major_events().limit().get(), 1);
        assert_eq!(t.summary().characters().len(), 3);
    }
    let templates = GenerationTemplates::bundled().unwrap();
    let narrow_request = templates.turn_request(game, None, false).unwrap();
    assert!(
        narrow_request
            .instructions()
            .as_str()
            .contains("at most 1 major events")
    );
    assert!(narrow_request.schema()["$defs"]["SummaryUpdate"]["properties"]["consolidated_major_events"]["description"].as_str().unwrap().contains("1 major events"));
    assert!(
        !narrow_request
            .prompt()
            .as_str()
            .contains("The closing prose of the previous chapter")
    );
    let bytes = encode(&current.stored.snapshot, &current.stored.metadata).unwrap();
    let unchanged = load(&bytes, RestoreLimits::Current(narrowed()));
    assert!(!unchanged.changed_by_restore);
    let original = load(&bytes, RestoreLimits::Original);
    assert!(original.changed_by_restore);
    assert_eq!(original.stored.snapshot.game.original_limits(), creation);
    assert_eq!(original.stored.snapshot.game.limits(), creation);
    assert_eq!(snapshots(&original.stored.snapshot.game), snapshots(game));
    let bytes = encode(&original.stored.snapshot, &original.stored.metadata).unwrap();
    let again = load(&bytes, RestoreLimits::Original);
    assert!(!again.changed_by_restore);
    let mut restored = again.stored.snapshot.game;
    restored.rewind(TurnCount::new(2).unwrap()).unwrap();
    assert_eq!(
        restored
            .current_summary()
            .major_events()
            .events()
            .iter()
            .map(|e| e.as_str())
            .collect::<Vec<_>>(),
        ["D"]
    );
    assert_eq!(restored.prose_context().0.len(), 2);
    let original_request = templates.turn_request(&restored, None, false).unwrap();
    assert!(
        original_request
            .instructions()
            .as_str()
            .contains("at most 4 major events")
    );
    assert!(original_request.schema()["$defs"]["SummaryUpdate"]["properties"]["consolidated_major_events"]["description"].as_str().unwrap().contains("4 major events"));
    assert!(
        original_request
            .prompt()
            .as_str()
            .contains("The closing prose of the previous chapter")
    );
    let fixture: Value = serde_json::from_str(include_str!("fixtures/phase0_story.json")).unwrap();
    let mut cases = StoryUseCases::new(GenerationEngine::new(
        ScriptedBackend::new([Ok(fixture["turns"][4].to_string())]),
        templates,
    ));
    cases
        .take_turn(
            &mut restored,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(
        restored
            .current_summary()
            .major_events()
            .events()
            .iter()
            .map(|e| e.as_str())
            .collect::<Vec<_>>(),
        ["D", "F"]
    );
    assert_eq!(restored.original_limits(), creation);
    let backend = cases.into_generator().into_backend();
    assert_eq!(backend.requests().len(), 1);
    assert_eq!(
        backend.requests()[0].prompt(),
        original_request.prompt().as_str()
    );
}
#[test]
fn zero_turn_restore_uses_selected_cap_and_preserves_established_single_playable() {
    let mut fixture: Value =
        serde_json::from_str(include_str!("fixtures/saves/v1-minimal.json")).unwrap();
    fixture["game"]["world"]["characters"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    fixture["game"]["character_index"] = 0.into();
    let bytes = serde_json::to_vec(&fixture).unwrap();
    let loaded = load(&bytes, RestoreLimits::Current(narrowed()));
    let game = &loaded.stored.snapshot.game;
    assert_eq!(game.current_summary().major_events().limit().get(), 1);
    assert_eq!(game.protagonist().description().as_str(), "A mason");
    let encoded = encode(&loaded.stored.snapshot, &loaded.stored.metadata).unwrap();
    let restored = load(&encoded, RestoreLimits::Original);
    assert_eq!(
        restored
            .stored
            .snapshot
            .game
            .current_summary()
            .major_events()
            .limit()
            .get(),
        4
    );
    assert_eq!(
        restored
            .stored
            .snapshot
            .game
            .world()
            .cast()
            .playable()
            .len(),
        1
    );
    let request = GenerationTemplates::bundled()
        .unwrap()
        .turn_request(game, None, false)
        .unwrap();
    assert!(
        request
            .instructions()
            .as_str()
            .contains("at most 1 major events")
    );
    assert_eq!(
        request
            .prompt()
            .as_str()
            .matches("Before writing the opening scene")
            .count(),
        1
    );
}
