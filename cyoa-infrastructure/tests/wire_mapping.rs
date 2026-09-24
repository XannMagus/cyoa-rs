//! Boundary-conversion tests for `generation::wire` (ARCH-002): every blank
//! wire string maps to `None`, null-vs-empty is preserved for
//! `upcoming_events`, incomplete generated characters are dropped rather than
//! failing the whole cast, an unrecognized quick-action kind never errors,
//! and the deliberate `ChapterMarker` extension (a `Continue` turn can carry
//! a title) actually reaches the domain type.

use cyoa_core::{
    summary::UpcomingEventsUpdate,
    turn::{ChapterMarker, QuickActionKind, StoryTurn},
    world::WorldOutline,
};
use cyoa_infrastructure::generation::wire::{
    CharacterDeltaWire, GeneratedCastWire, InvalidStoryTurn, InvalidWorldOutline,
    NonPlayerCharacterWire, PlayerCharacterWire, QuickActionKindWire, QuickActionWire,
    StoryTurnWire, SummaryUpdateWire, WorldOutlineWire, playable_and_npcs_from_wire,
};

fn quick_action(text: &str, kind: QuickActionKindWire) -> QuickActionWire {
    QuickActionWire {
        text: text.to_string(),
        kind,
    }
}

fn minimal_turn_wire() -> StoryTurnWire {
    StoryTurnWire {
        narrative: "The lantern flickered.".into(),
        quick_actions: vec![quick_action("Push on", QuickActionKindWire::Bold)],
        scene_description: String::new(),
        summary_update: SummaryUpdateWire::default(),
        starts_new_chapter: false,
        chapter_title: None,
    }
}

#[test]
fn blank_wire_fields_map_to_none_across_every_optional_field() {
    let delta = CharacterDeltaWire {
        id: "  ".into(),
        current_state: "".into(),
        name: "\t".into(),
        description: "".into(),
        backstory: "".into(),
        relationships: "".into(),
    };
    let domain: cyoa_core::character::CharacterDelta = delta.into();
    assert_eq!(domain.id, None);
    assert_eq!(domain.name, None);
    assert_eq!(domain.description, None);
    assert_eq!(domain.backstory, None);
    assert_eq!(domain.relationships, None);
    assert_eq!(domain.current_state, None);

    let nonblank = CharacterDeltaWire {
        id: "marlo".into(),
        current_state: "watching the road".into(),
        ..CharacterDeltaWire::default()
    };
    let domain: cyoa_core::character::CharacterDelta = nonblank.into();
    assert_eq!(domain.id.unwrap().as_str(), "marlo");
    assert_eq!(domain.current_state.unwrap().as_str(), "watching the road");
}

#[test]
fn upcoming_events_null_is_keep_empty_list_is_replace_with_no_threads() {
    let keep: cyoa_core::summary::SummaryUpdate = SummaryUpdateWire {
        current_situation: "Somewhere.".into(),
        upcoming_events: None,
        ..SummaryUpdateWire::default()
    }
    .into();
    assert_eq!(keep.upcoming_events, UpcomingEventsUpdate::Keep);

    let cleared: cyoa_core::summary::SummaryUpdate = SummaryUpdateWire {
        current_situation: "Somewhere.".into(),
        upcoming_events: Some(vec![]),
        ..SummaryUpdateWire::default()
    }
    .into();
    assert_eq!(
        cleared.upcoming_events,
        UpcomingEventsUpdate::Replace(cyoa_core::summary::EventList::default())
    );

    let replaced: cyoa_core::summary::SummaryUpdate = SummaryUpdateWire {
        current_situation: "Somewhere.".into(),
        upcoming_events: Some(vec!["Marlo returns".into()]),
        ..SummaryUpdateWire::default()
    }
    .into();
    assert_eq!(
        replaced.upcoming_events,
        UpcomingEventsUpdate::Replace(cyoa_core::summary::EventList::new(["Marlo returns"]))
    );
}

#[test]
fn incomplete_generated_characters_are_dropped_not_erroring_the_whole_cast() {
    let wire = GeneratedCastWire {
        characters: vec![
            PlayerCharacterWire {
                name: "Ada".into(),
                description: "a stubborn engineer".into(),
                backstory: "built the mist engines".into(),
            },
            PlayerCharacterWire {
                name: "".into(),
                description: "no name".into(),
                backstory: "no name".into(),
            },
        ],
        npcs: vec![
            NonPlayerCharacterWire {
                name: "Casey".into(),
                description: "a wary guard".into(),
                backstory: "".into(), // no backstory AND blank relationships is fine: only backstory is optional-with-relationships? see below
                relationships: "".into(),
            },
            NonPlayerCharacterWire {
                name: "Ghost".into(),
                description: "".into(),
                backstory: "".into(),
                relationships: "".into(),
            },
        ],
    };
    let (playable, npcs) = playable_and_npcs_from_wire(wire);
    assert_eq!(playable.len(), 1);
    assert_eq!(playable[0].name().as_str(), "Ada");
    // Casey is missing a backstory, which calibre also requires: dropped.
    assert_eq!(npcs.len(), 0);
}

#[test]
fn unknown_quick_action_kind_deserializes_to_other_not_an_error() {
    let parsed: QuickActionKindWire = serde_json::from_str("\"aggressive\"").unwrap();
    assert_eq!(parsed.to_domain(), QuickActionKind::Other);
    let known: QuickActionKindWire = serde_json::from_str("\"bold\"").unwrap();
    assert_eq!(known.to_domain(), QuickActionKind::Bold);
}

#[test]
fn blank_quick_action_text_is_filtered_before_selection_not_a_hard_error() {
    let mut wire = minimal_turn_wire();
    wire.quick_actions = vec![
        quick_action("   ", QuickActionKindWire::Cautious),
        quick_action("Push on", QuickActionKindWire::Bold),
    ];
    let turn = StoryTurn::try_from(wire).unwrap();
    assert_eq!(turn.quick_actions().as_slice().len(), 1);

    let mut all_blank = minimal_turn_wire();
    all_blank.quick_actions = vec![quick_action("  ", QuickActionKindWire::Cautious)];
    assert_eq!(
        StoryTurn::try_from(all_blank).unwrap_err(),
        InvalidStoryTurn::NoQuickActions
    );
}

#[test]
fn blank_narrative_is_rejected_but_blank_scene_description_is_not() {
    let mut wire = minimal_turn_wire();
    wire.narrative = "   ".into();
    assert_eq!(
        StoryTurn::try_from(wire).unwrap_err(),
        InvalidStoryTurn::BlankNarrative
    );

    let mut wire = minimal_turn_wire();
    wire.scene_description = "".into();
    let turn = StoryTurn::try_from(wire).unwrap();
    assert_eq!(turn.scene_description(), None);
}

#[test]
fn new_chapter_marker_carries_a_blank_filtered_title_and_so_does_continue() {
    let mut wire = minimal_turn_wire();
    wire.starts_new_chapter = true;
    wire.chapter_title = Some("Prologue".into());
    let turn = StoryTurn::try_from(wire).unwrap();
    match turn.chapter() {
        ChapterMarker::NewChapter { title } => {
            assert_eq!(title.as_ref().unwrap().as_str(), "Prologue");
        }
        other => panic!("expected NewChapter, got {other:?}"),
    }

    let mut wire = minimal_turn_wire();
    wire.starts_new_chapter = false;
    wire.chapter_title = Some("Renamed".into());
    let turn = StoryTurn::try_from(wire).unwrap();
    match turn.chapter() {
        ChapterMarker::Continue { title } => {
            assert_eq!(title.as_ref().unwrap().as_str(), "Renamed");
        }
        other => panic!("expected Continue with a title, got {other:?}"),
    }

    let mut wire = minimal_turn_wire();
    wire.starts_new_chapter = true;
    wire.chapter_title = Some("   ".into());
    let turn = StoryTurn::try_from(wire).unwrap();
    match turn.chapter() {
        ChapterMarker::NewChapter { title } => assert!(title.is_none()),
        other => panic!("expected NewChapter with no title, got {other:?}"),
    }
}

#[test]
fn world_outline_rejects_blank_title_or_description() {
    let good = WorldOutlineWire {
        title: "The Mist City".into(),
        world_description: "A city lost in fog.".into(),
    };
    let outline = WorldOutline::try_from(good).unwrap();
    assert_eq!(outline.title().as_str(), "The Mist City");

    let blank_title = WorldOutlineWire {
        title: " ".into(),
        world_description: "A city lost in fog.".into(),
    };
    assert_eq!(
        WorldOutline::try_from(blank_title).unwrap_err(),
        InvalidWorldOutline::BlankTitle
    );

    let blank_description = WorldOutlineWire {
        title: "The Mist City".into(),
        world_description: "".into(),
    };
    assert_eq!(
        WorldOutline::try_from(blank_description).unwrap_err(),
        InvalidWorldOutline::BlankDescription
    );
}
