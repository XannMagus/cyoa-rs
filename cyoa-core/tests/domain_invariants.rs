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
