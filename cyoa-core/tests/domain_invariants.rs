//! Public domain behavior, independent of adapters and application orchestration.
use cyoa_core::{
    game::GameState,
    limits::Limits,
    style::StoryStyle,
    text::{
        ArtStyleKey, Backstory, Brief, CharacterDescription, CharacterName, CurrencyCode,
        Instructions, ModelName, NarrationKey, Narrative, PaceKey, PlayerInput, ProviderName,
        QuickActionText, RawResponse, Relationships, RenderedPrompt, SceneDescription, ToneKey,
        WorldDescription, WorldTitle,
    },
    turn::{
        ChapterMarker, CostAmount, GenerationProvenance, ListPriceEstimate, PromptTrace,
        QuickAction, QuickActionKind, QuickActions, StoryTurn, TurnGenerationRecord,
    },
    world::{
        NonPlayerCharacter, PlayablePosition, PlayerCharacter, World, WorldCast, WorldOutline,
    },
};

fn game(style: StoryStyle) -> GameState {
    let limits = Limits::default();
    let players = ["Alex", "Blair"].map(|name| {
        PlayerCharacter::new(
            CharacterName::new(name).unwrap(),
            CharacterDescription::new("A sailor").unwrap(),
            Backstory::new("Grew up here").unwrap(),
        )
    });
    let npc = NonPlayerCharacter::new(
        CharacterName::new("Casey").unwrap(),
        CharacterDescription::new("Harbour keeper").unwrap(),
        Backstory::new("Knows the coast").unwrap(),
        Some(Relationships::new("Blair's sibling").unwrap()),
    );
    let world = World::new(
        WorldOutline::new(
            WorldTitle::new("Harbour").unwrap(),
            WorldDescription::new("A quiet coast").unwrap(),
        ),
        WorldCast::new(players, [npc], &limits).unwrap(),
    )
    .select(PlayablePosition::new(1))
    .unwrap();
    GameState::start(Brief::new("A harbour story").unwrap(), world, style, limits)
}

fn record() -> TurnGenerationRecord {
    TurnGenerationRecord {
        input: None,
        raw_response: RawResponse::new("{}"),
        provenance: GenerationProvenance::default(),
        prompt_trace: None,
    }
}

fn turn(chapter: ChapterMarker) -> StoryTurn {
    StoryTurn::new(
        Narrative::new("The bell rings.").unwrap(),
        QuickActions::select([QuickAction::new(
            QuickActionText::new("Listen").unwrap(),
            QuickActionKind::Cautious,
        )])
        .unwrap(),
        Some(SceneDescription::new("A bell tower above the quay").unwrap()),
        Default::default(),
        chapter,
    )
}

#[test]
fn chapters_expose_zero_based_numbers_and_one_based_ordinals() {
    let mut state = game(StoryStyle::default());
    for _ in 0..3 {
        state.commit_turn(turn(ChapterMarker::NewChapter { title: None }), record());
    }
    let chapters = state.chapters();
    assert_eq!(chapters.len(), 3);
    for (chapter, (number, ordinal)) in chapters.iter().zip([(0, 1), (1, 2), (2, 3)]) {
        assert_eq!(chapter.number().get(), number);
        assert_eq!(chapter.number().ordinal(), ordinal);
        assert_eq!(chapter.turns().len(), 1);
    }
}

#[test]
fn changing_style_preserves_committed_story_and_other_game_settings() {
    let initial = StoryStyle {
        art_style: Some(ArtStyleKey::new("ink").unwrap()),
        pace: Some(PaceKey::new("slow").unwrap()),
        tone: Some(ToneKey::new("quiet").unwrap()),
        narration: Some(NarrationKey::new("first-person").unwrap()),
    };
    let mut state = game(initial.clone());
    assert_eq!(state.style(), &initial);
    state.commit_turn(turn(ChapterMarker::Continue { title: None }), record());
    let before = state.clone();
    let changed = StoryStyle {
        pace: Some(PaceKey::new("brisk").unwrap()),
        ..Default::default()
    };
    state.set_style(changed.clone());
    assert_eq!(state.style(), &changed);
    assert_eq!(state.turns(), before.turns());
    assert_eq!(state.current_summary(), before.current_summary());
    assert_eq!(state.brief(), before.brief());
    assert_eq!(state.selected_world(), before.selected_world());
    assert_eq!(state.limits(), before.limits());
    assert_eq!(state.original_limits(), before.original_limits());
    state.set_style(initial);
    assert_eq!(state, before);
}

#[test]
fn npc_relationships_survive_initial_summary_construction() {
    let state = game(StoryStyle::default());
    let npc = &state.world().cast().npcs()[0];
    assert_eq!(npc.relationships().unwrap().as_str(), "Blair's sibling");
    let summary = state.current_summary();
    let stored = summary
        .characters()
        .iter()
        .find(|c| c.name().as_str() == "Casey")
        .unwrap();
    assert_eq!(
        stored.details().relationships().unwrap().as_str(),
        "Blair's sibling"
    );
}

#[test]
fn optional_turn_fields_and_audit_record_survive_commit() {
    let mut state = game(StoryStyle::default());
    let provenance = GenerationProvenance {
        provider: Some(ProviderName::new("fixture").unwrap()),
        model: Some(ModelName::new("story-model").unwrap()),
        cost: Some(ListPriceEstimate::new(
            CostAmount::new(2.75).unwrap(),
            CurrencyCode::new("USD").unwrap(),
        )),
    };
    let trace = PromptTrace {
        instructions: Instructions::new(" instructions\r\n"),
        prompt: RenderedPrompt::new(" prompt\n"),
    };
    let input = PlayerInput::new("Look up").unwrap();
    let mut metadata = record();
    metadata.input = Some(input.clone());
    metadata.raw_response = RawResponse::new(" \r\n{response}\n");
    metadata.provenance = provenance.clone();
    metadata.prompt_trace = Some(trace.clone());
    state.commit_turn(turn(ChapterMarker::Continue { title: None }), metadata);
    let stored = &state.turns()[0];
    assert_eq!(stored.input(), Some(&input));
    assert_eq!(stored.raw_response().as_str(), " \r\n{response}\n");
    assert_eq!(stored.provenance(), &provenance);
    assert_eq!(
        stored.provenance().cost.as_ref().unwrap().amount().get(),
        2.75
    );
    assert_eq!(
        stored
            .provenance()
            .cost
            .as_ref()
            .unwrap()
            .currency()
            .as_str(),
        "USD"
    );
    assert_eq!(stored.prompt_trace(), Some(&trace));
    assert_eq!(
        stored.turn().scene_description().unwrap().as_str(),
        "A bell tower above the quay"
    );
}

#[test]
fn cost_amount_preserves_valid_values_and_rejects_nonfinite_or_negative_values() {
    for amount in [0.0, -0.0, 0.25, 2.75, f64::MAX] {
        assert_eq!(CostAmount::new(amount).unwrap().get(), amount);
    }
    for amount in [-0.25, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(CostAmount::new(amount).is_err());
    }
}

#[test]
fn persistence_identity_counters_stamps_and_time_keep_checked_boundaries() {
    use cyoa_core::persistence::*;
    let hex = "0123456789abcdef0123456789abcdef";
    for slug in [
        "a".to_string(),
        "a-b".to_string(),
        "a1-2b".to_string(),
        "a".repeat(40),
    ] {
        let name = format!("{slug}-{hex}");
        assert_eq!(SaveId::new(name.clone()).unwrap().as_str(), name);
    }
    for name in [
        "story".to_string(),
        format!("-{hex}"),
        format!("{}-{hex}", "a".repeat(41)),
        format!("two--words-{hex}"),
        format!("Upper-{hex}"),
        format!("a_-{hex}"),
        format!("é-{hex}"),
        format!("a-{}", &hex[..31]),
        format!("a-{hex}0"),
        "a-0123456789abcdef0123456789abcdeF".into(),
        "a-0123456789abcdef0123456789abcdeg".into(),
    ] {
        assert!(SaveId::new(name).is_err());
    }
    assert!(SaveRevision::new(0).is_err());
    let one = SaveRevision::new(1).unwrap();
    assert_eq!(one.get(), 1);
    assert_eq!(one.next().unwrap().get(), 2);
    let last = SaveRevision::new(u64::MAX).unwrap();
    assert_eq!(last.get(), u64::MAX);
    assert!(last.next().is_err());
    let stamp = ContentStamp::new([9; 32]);
    assert_eq!(stamp.bytes(), &[9; 32]);
    assert!(SavedAt::new(-62_135_596_801, 0).is_err());
    assert!(SavedAt::new(253_402_300_800, 0).is_err());
    assert!(SavedAt::new(0, 1_000_000_000).is_err());
    for seconds in [-62_135_596_800, 0, 253_402_300_799] {
        for (nanos, expected) in [
            (0, 0),
            (123_456_789, 123_000_000),
            (999_999_999, 999_000_000),
        ] {
            let at = SavedAt::new(seconds, nanos).unwrap();
            assert_eq!(at.unix_seconds(), seconds);
            assert_eq!(at.nanoseconds(), expected);
        }
    }
}

#[test]
fn save_snapshot_preserves_game_source_and_demo_passage_limit() {
    use cyoa_core::persistence::*;
    let demo = StorySource::Demo {
        scenario: DemoScenarioId::HarbourV1,
    };
    assert_eq!(DemoScenarioId::HarbourV1.as_str(), "harbour-v1");
    assert_eq!(DemoScenarioId::HarbourV1.passages().get(), 5);
    assert!(StorySource::Live.admits(usize::MAX));
    assert!(demo.admits(0));
    assert!(demo.admits(5));
    assert!(!demo.admits(6));
    assert!(!demo.admits(usize::MAX));
    let mut state = game(StoryStyle::default());
    for turns in 0..=6 {
        let live = SaveSnapshot::new(state.clone(), StorySource::Live).unwrap();
        assert_eq!(live.game(), &state);
        assert_eq!(live.source(), StorySource::Live);
        assert_eq!(live.into_game(), state);
        if turns <= 5 {
            let saved = SaveSnapshot::new(state.clone(), demo).unwrap();
            assert_eq!(saved.game(), &state);
            assert_eq!(saved.source(), demo);
            assert_eq!(saved.into_game(), state);
        } else {
            let failure = SaveSnapshot::new(state.clone(), demo).unwrap_err();
            assert_eq!(failure.scenario, DemoScenarioId::HarbourV1);
            assert_eq!(failure.turns, 6);
        }
        state.commit_turn(turn(ChapterMarker::Continue { title: None }), record());
    }
}

#[test]
fn turn_direction_keeps_player_input_distinct_from_generated_continuation() {
    use cyoa_core::generation::TurnDirection;
    assert_eq!(TurnDirection::Continue.input(), None);
    assert_eq!(TurnDirection::InterestingEvent.input(), None);
    let input = PlayerInput::new("Wait by the door").unwrap();
    assert_eq!(TurnDirection::Player(input.clone()).input(), Some(&input));
}
