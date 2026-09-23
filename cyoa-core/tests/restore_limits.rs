use cyoa_core::{
    game::{GameState, TurnCount},
    limits::{
        Limits, MajorEventLimit, MaxGeneratedNpcs, MinPlayableCharacters, ProseBridgeTurns,
        RestoreLimits,
    },
    style::StoryStyle,
    summary::{EventList, SummaryUpdate},
    text::{
        Backstory, Brief, CharacterDescription, CharacterName, Instructions, Narrative,
        QuickActionText, RawResponse, RenderedPrompt, WorldDescription, WorldTitle,
    },
    turn::{
        ChapterMarker, GenerationProvenance, PromptTrace, QuickAction, QuickActionKind,
        QuickActions, StoryTurn, TurnGenerationRecord,
    },
    world::{
        NonPlayerCharacter, PlayablePosition, PlayerCharacter, World, WorldCast, WorldOutline,
    },
};

fn game(limits: Limits) -> GameState {
    let players = ["Alex", "Blair"].map(|name| {
        PlayerCharacter::new(
            CharacterName::new(name).unwrap(),
            CharacterDescription::new("description").unwrap(),
            Backstory::new("history").unwrap(),
        )
    });
    let npc = NonPlayerCharacter::new(
        CharacterName::new("Casey").unwrap(),
        CharacterDescription::new("description").unwrap(),
        Backstory::new("history").unwrap(),
        None,
    );
    let world = World::new(
        WorldOutline::new(
            WorldTitle::new("Title").unwrap(),
            WorldDescription::new("World").unwrap(),
        ),
        WorldCast::new(players, [npc], &limits).unwrap(),
    )
    .select(PlayablePosition::new(1))
    .unwrap();
    GameState::start(
        Brief::new("Brief").unwrap(),
        world,
        StoryStyle::default(),
        limits,
    )
}

fn restore(state: &GameState, policy: RestoreLimits) -> GameState {
    GameState::restore(
        state.brief().clone(),
        state.selected_world().clone(),
        state.style().clone(),
        state.original_limits(),
        policy,
        state.turns().to_vec(),
    )
}

fn commit_events(state: &mut GameState, events: &[&str]) {
    state.commit_turn(
        StoryTurn::new(
            Narrative::new("A passage.").unwrap(),
            QuickActions::select([QuickAction::new(
                QuickActionText::new("Go").unwrap(),
                QuickActionKind::Bold,
            )])
            .unwrap(),
            None,
            SummaryUpdate {
                new_major_events: EventList::new(events),
                ..Default::default()
            },
            ChapterMarker::NewChapter { title: None },
        ),
        TurnGenerationRecord {
            input: None,
            raw_response: RawResponse::new(" \n{}\r\n"),
            provenance: GenerationProvenance::default(),
            prompt_trace: Some(PromptTrace {
                instructions: Instructions::new("\nInstructions \n"),
                prompt: RenderedPrompt::new(" Prompt\t"),
            }),
        },
    );
}

fn assert_events(state: &GameState, expected: &[&str], limit: usize) {
    let summary = state.current_summary();
    let actual: Vec<_> = summary
        .major_events()
        .events()
        .iter()
        .map(|event| event.as_str())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(summary.major_events().limit().get(), limit);
    assert_eq!(state.limits().max_major_events.get(), limit);
    for turn in state.turns() {
        assert_eq!(turn.summary().major_events().limit().get(), limit);
        assert!(turn.summary().major_events().events().as_slice().len() <= limit);
    }
}

#[test]
fn current_limits_apply_to_every_snapshot_and_survive_commit_and_rewind() {
    let original = Limits {
        max_major_events: MajorEventLimit::new(4).unwrap(),
        ..Limits::default()
    };
    let mut source = game(original);
    commit_events(&mut source, &["one", "two"]);
    commit_events(&mut source, &["three"]);
    commit_events(&mut source, &["four"]);
    let current = Limits {
        max_major_events: MajorEventLimit::new(1).unwrap(),
        max_generated_npcs: MaxGeneratedNpcs::new(0),
        min_playable_characters: MinPlayableCharacters::new(99).unwrap(),
        prose_bridge_turns: ProseBridgeTurns::new(0),
    };
    let mut restored = restore(&source, RestoreLimits::Current(current));
    assert_eq!(restored.limits(), current);
    assert_eq!(restored.original_limits(), original);
    assert_eq!(restored.selected_world(), source.selected_world());
    assert_eq!(restored.protagonist().name().as_str(), "Blair");
    assert!(restored.prose_context().0.is_empty());
    assert!(!source.prose_context().0.is_empty());
    for (before, after) in source.turns().iter().zip(restored.turns()) {
        assert_eq!(before.turn(), after.turn());
        assert_eq!(before.raw_response(), after.raw_response());
        assert_eq!(after.raw_response().as_str(), " \n{}\r\n");
        assert_eq!(before.prompt_trace(), after.prompt_trace());
        assert_eq!(before.summary().characters(), after.summary().characters());
    }
    assert_events(&restored, &["four"], 1);
    commit_events(&mut restored, &["five"]);
    assert_events(&restored, &["five"], 1);
    restored.rewind(TurnCount::new(1).unwrap()).unwrap();
    assert_events(&restored, &["four"], 1);
    restored.rewind(TurnCount::new(1).unwrap()).unwrap();
    assert_events(&restored, &["three"], 1);
    restored.rewind(TurnCount::new(2).unwrap()).unwrap();
    assert_events(&restored, &[], 1);
    commit_events(&mut restored, &["restart one", "restart two"]);
    assert_events(&restored, &["restart two"], 1);
    assert_events(&source, &["one", "two", "three", "four"], 4);
}

#[test]
fn original_limits_remain_available_after_restoring_with_current_settings() {
    let original = Limits {
        max_major_events: MajorEventLimit::new(4).unwrap(),
        ..Limits::default()
    };
    let mut source = game(original);
    commit_events(&mut source, &["one", "two", "three"]);
    assert_eq!(restore(&source, RestoreLimits::Original), source);
    let current = Limits {
        max_major_events: MajorEventLimit::new(1).unwrap(),
        prose_bridge_turns: ProseBridgeTurns::new(0),
        ..original
    };
    let narrowed = restore(&source, RestoreLimits::Current(current));
    let mut restored = restore(&narrowed, RestoreLimits::Original);
    assert_eq!(restored.limits(), original);
    assert_eq!(restored.original_limits(), original);
    assert_events(&restored, &["three"], 4);
    commit_events(&mut restored, &["four", "five", "six", "seven"]);
    assert_events(&restored, &["four", "five", "six", "seven"], 4);
    restored.rewind(TurnCount::new(1).unwrap()).unwrap();
    assert_events(&restored, &["three"], 4);
}

#[test]
fn raising_the_limit_allows_future_events_without_recovering_discarded_ones() {
    let original = Limits {
        max_major_events: MajorEventLimit::new(2).unwrap(),
        ..Limits::default()
    };
    let mut source = game(original);
    commit_events(&mut source, &["one", "two", "three"]);
    let current = Limits {
        max_major_events: MajorEventLimit::new(4).unwrap(),
        ..original
    };
    let mut restored = restore(&source, RestoreLimits::Current(current));
    assert_events(&restored, &["two", "three"], 4);
    commit_events(&mut restored, &["four", "five"]);
    assert_events(&restored, &["two", "three", "four", "five"], 4);
}

#[test]
fn empty_restored_games_use_the_selected_limits_for_their_opening_summary() {
    let source = game(Limits::default());
    let current = Limits {
        max_major_events: MajorEventLimit::new(1).unwrap(),
        ..Limits::default()
    };
    let mut restored = restore(&source, RestoreLimits::Current(current));
    assert_events(&restored, &[], 1);
    commit_events(&mut restored, &["one", "two"]);
    assert_events(&restored, &["two"], 1);
    assert_eq!(restore(&source, RestoreLimits::Original), source);
}
