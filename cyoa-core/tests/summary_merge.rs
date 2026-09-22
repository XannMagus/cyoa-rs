//! Ported from calibre's test_ai_cyoa_summary_updates / test_ai_cyoa_character_ids,
//! with additional invariants and same-turn alias regression cases.
use cyoa_core::{
    character::{
        Character, CharacterCast, CharacterDelta, CharacterDetails, MissingCharacterDetails,
    },
    ids::CharacterId,
    summary::{
        EventList, MajorEventLimit, MajorEvents, StorySummary, SummaryUpdate, UpcomingEventsUpdate,
    },
    text::{
        Backstory, CharacterDescription, CharacterName, CharacterSituation, CurrentSituation,
        Relationships, WorldDescription,
    },
};

fn character(id: &str, name: &str) -> Character {
    Character::new(
        CharacterId::new(id).unwrap(),
        CharacterName::new(name).unwrap(),
        CharacterDetails::new(
            CharacterDescription::new("a stubborn engineer").ok(),
            Backstory::new("Built the mist engines.").ok(),
            None,
            None,
        )
        .unwrap(),
    )
}

fn initial() -> StorySummary {
    StorySummary::new(
        WorldDescription::new("A city lost in perpetual mist.").unwrap(),
        CurrentSituation::new("The adventure has not yet begun.").unwrap(),
        CharacterCast::new(vec![character("protagonist", "Ada")]).unwrap(),
        MajorEvents::new(EventList::default(), MajorEventLimit::default()),
        EventList::new(["The mist thickens."]),
    )
}

fn by_id(id: &str, state: &str) -> CharacterDelta {
    CharacterDelta {
        id: Some(CharacterId::new(id).unwrap()),
        current_state: CharacterSituation::new(state).ok(),
        ..Default::default()
    }
}

fn merge_characters(summary: &StorySummary, deltas: Vec<CharacterDelta>) -> StorySummary {
    summary.updated(&SummaryUpdate {
        character_updates: deltas,
        ..Default::default()
    })
}

fn events(list: &EventList) -> Vec<&str> {
    list.iter().map(|event| event.as_str()).collect()
}

#[test]
fn blank_updates_preserve_durable_fields_and_leave_previous_untouched() {
    let previous = initial();
    let snapshot = previous.clone();
    let next = merge_characters(
        &previous,
        vec![by_id(" protagonist ", " lost in the mist ")],
    );
    assert_eq!(previous, snapshot);
    let ada = next.characters().iter().next().unwrap();
    assert_eq!(
        ada.details()
            .current_state()
            .map(CharacterSituation::as_str),
        Some("lost in the mist")
    );
    assert_eq!(
        ada.details()
            .description()
            .map(CharacterDescription::as_str),
        Some("a stubborn engineer")
    );
    assert_eq!(
        ada.details().backstory().map(Backstory::as_str),
        Some("Built the mist engines.")
    );
    assert_eq!(next.world(), previous.world());
    assert_eq!(next.current_situation(), previous.current_situation());
    assert_eq!(next.updated(&SummaryUpdate::default()), next);
    assert_eq!(
        merge_characters(&next, vec![by_id("protagonist", " \n ")]),
        next
    );
}

#[test]
fn world_and_situation_can_change_without_losing_characters() {
    let old = initial();
    let next = old.updated(&SummaryUpdate {
        world: Some(WorldDescription::new("The mist has lifted.").unwrap()),
        current_situation: Some(CurrentSituation::new("In the sun.").unwrap()),
        ..Default::default()
    });
    assert_eq!(next.world().as_str(), "The mist has lifted.");
    assert_eq!(next.current_situation().as_str(), "In the sun.");
    assert_eq!(next.characters(), old.characters());
}

#[test]
fn rename_keeps_identity_and_name_fallback_ignores_invented_id() {
    let previous = initial();
    let renamed = merge_characters(
        &previous,
        vec![CharacterDelta {
            name: Some(CharacterName::new("Marlo").unwrap()),
            ..by_id("protagonist", "at the gate")
        }],
    );
    let next = merge_characters(
        &renamed,
        vec![CharacterDelta {
            name: Some(CharacterName::new(" MARLO ").unwrap()),
            ..by_id("invented-id", "guiding Ada")
        }],
    );
    assert_eq!(next.characters().len(), 1);
    assert_eq!(
        next.characters().iter().next().unwrap().id().as_str(),
        "protagonist"
    );
    assert_eq!(
        next.characters()
            .iter()
            .next()
            .unwrap()
            .details()
            .current_state()
            .map(CharacterSituation::as_str),
        Some("guiding Ada")
    );
}

#[test]
fn id_match_wins_over_conflicting_name_match() {
    let cast =
        CharacterCast::new(vec![character("ada", "Ada"), character("brin", "Brin")]).unwrap();
    let next = cast.updated(&[CharacterDelta {
        name: Some(CharacterName::new("Brin").unwrap()),
        ..by_id("ada", "changed")
    }]);
    assert_eq!(
        next.iter()
            .next()
            .unwrap()
            .details()
            .current_state()
            .map(CharacterSituation::as_str),
        Some("changed")
    );
    assert_eq!(next.iter().nth(1).unwrap(), cast.iter().nth(1).unwrap());
}

#[test]
fn first_update_wins_including_new_and_old_name_aliases() {
    for alias in ["Marlo", "Ada"] {
        let next = merge_characters(
            &initial(),
            vec![
                CharacterDelta {
                    name: Some(CharacterName::new("Marlo").unwrap()),
                    ..by_id("protagonist", "first")
                },
                CharacterDelta {
                    name: Some(CharacterName::new(alias).unwrap()),
                    ..by_id("unknown", "second")
                },
                by_id("protagonist", "third"),
            ],
        );
        assert_eq!(next.characters().len(), 1);
        assert_eq!(
            next.characters().iter().next().unwrap().name().as_str(),
            "Marlo"
        );
        assert_eq!(
            next.characters()
                .iter()
                .next()
                .unwrap()
                .details()
                .current_state()
                .map(CharacterSituation::as_str),
            Some("first")
        );
    }
}

#[test]
fn new_characters_need_name_and_durable_information() {
    let next = merge_characters(
        &initial(),
        vec![
            CharacterDelta {
                name: Some(CharacterName::new("Marlo").unwrap()),
                description: CharacterDescription::new(" a mist-runner ").ok(),
                ..Default::default()
            },
            CharacterDelta {
                name: Some(CharacterName::new("Brin").unwrap()),
                backstory: Backstory::new("a local").ok(),
                ..Default::default()
            },
            by_id("ghost", "watching"),
            CharacterDelta {
                name: Some(CharacterName::new("Nameless friend").unwrap()),
                description: CharacterDescription::new("  ").ok(),
                ..Default::default()
            },
            CharacterDelta {
                description: CharacterDescription::new("no name").ok(),
                ..Default::default()
            },
            CharacterDelta {
                name: Some(CharacterName::new("MARLO").unwrap()),
                ..by_id("marlo", "second update")
            },
        ],
    );
    let cast: Vec<_> = next.characters().iter().collect();
    assert_eq!(cast.len(), 3);
    assert_eq!(cast[1].id().as_str(), "marlo");
    assert_eq!(
        cast[1]
            .details()
            .description()
            .map(CharacterDescription::as_str),
        Some("a mist-runner")
    );
    assert_eq!(
        cast[1]
            .details()
            .current_state()
            .map(CharacterSituation::as_str),
        None
    );
    assert_eq!(cast[2].id().as_str(), "brin");
}

#[test]
fn derived_id_collisions_get_a_unique_suffix() {
    let cast = CharacterCast::new(vec![
        character("the-stranger", "Marlo"),
        character("the-stranger-2", "Brin"),
    ])
    .unwrap();
    let next = cast.updated(&[CharacterDelta {
        name: Some(CharacterName::new("The Stranger").unwrap()),
        description: CharacterDescription::new("a different hooded figure").ok(),
        ..Default::default()
    }]);
    assert_eq!(next.iter().nth(2).unwrap().id().as_str(), "the-stranger-3");
}

#[test]
fn events_accumulate_deduplicate_and_consolidate_in_order() {
    let first = initial().updated(&SummaryUpdate {
        new_major_events: EventList::new([" awoke ", "AWOKE", "", "saw shapes"]),
        ..Default::default()
    });
    let next = first.updated(&SummaryUpdate {
        new_major_events: EventList::new(["Saw Shapes", "escaped"]),
        ..Default::default()
    });
    assert_eq!(
        events(next.major_events().events()),
        ["awoke", "saw shapes", "escaped"]
    );
    let consolidated = next.updated(&SummaryUpdate {
        consolidated_major_events: EventList::new(["everything up to now"]),
        new_major_events: EventList::new(["and then this happened"]),
        ..Default::default()
    });
    assert_eq!(
        events(consolidated.major_events().events()),
        ["everything up to now", "and then this happened"]
    );
    assert_eq!(
        next.updated(&SummaryUpdate {
            consolidated_major_events: EventList::new([" "]),
            ..Default::default()
        }),
        next
    );
}

#[test]
fn cap_keeps_last_thirty_and_supports_a_configured_limit() {
    let list = EventList::new((0..35).map(|n| format!("event {n}")));
    let next = initial().updated(&SummaryUpdate {
        new_major_events: list.clone(),
        ..Default::default()
    });
    assert_eq!(next.major_events().events().as_slice().len(), 30);
    assert_eq!(
        next.major_events().events().as_slice()[0].as_str(),
        "event 5"
    );
    let small = MajorEvents::new(list, MajorEventLimit::new(2).unwrap());
    assert_eq!(events(small.events()), ["event 33", "event 34"]);
    assert!(MajorEventLimit::new(0).is_err());
}

#[test]
fn upcoming_events_null_is_not_empty() {
    let initial = initial();
    assert_eq!(
        initial.updated(&SummaryUpdate::default()).upcoming_events(),
        initial.upcoming_events()
    );
    let replace = initial.updated(&SummaryUpdate {
        upcoming_events: UpcomingEventsUpdate::Replace(EventList::new([
            "Marlo returns",
            " MARLO RETURNS ",
        ])),
        ..Default::default()
    });
    assert_eq!(events(replace.upcoming_events()), ["Marlo returns"]);
    let cleared = replace.updated(&SummaryUpdate {
        upcoming_events: UpcomingEventsUpdate::Replace(EventList::default()),
        ..Default::default()
    });
    assert!(cleared.upcoming_events().is_empty());
}

#[test]
fn details_establish_the_invariant_before_infallible_character_construction() {
    assert_eq!(
        CharacterDetails::new(
            None,
            None,
            Relationships::new("knows Ada").ok(),
            CharacterSituation::new("watching").ok()
        ),
        Err(MissingCharacterDetails),
    );
    for (description, backstory) in [
        (
            Some(CharacterDescription::new("a mist-runner").unwrap()),
            None,
        ),
        (None, Some(Backstory::new("grew up in the city").unwrap())),
    ] {
        let details = CharacterDetails::new(description, backstory, None, None).unwrap();
        let character: Character = Character::new(
            CharacterId::protagonist(),
            CharacterName::new("Ada").unwrap(),
            details,
        );
        assert!(
            character.details().description().is_some()
                || character.details().backstory().is_some()
        );
        assert_eq!(character.details().relationships(), None);
        assert_eq!(character.details().current_state(), None);
    }
}

#[test]
fn present_delta_values_cannot_be_blank() {
    for blank in ["", " ", "\n\t"] {
        assert!(CharacterDescription::new(blank).is_err());
        assert!(Backstory::new(blank).is_err());
        assert!(Relationships::new(blank).is_err());
        assert!(CharacterSituation::new(blank).is_err());
    }
    let no_change = CharacterDelta::default();
    assert_eq!(no_change.description, None);
    assert_eq!(no_change.backstory, None);
    assert_eq!(no_change.relationships, None);
    assert_eq!(no_change.current_state, None);
}

#[test]
fn cast_suffixes_nonadjacent_duplicate_ids_without_dropping_characters() {
    let cast = CharacterCast::new(vec![
        character("same", "Ada"),
        character("other", "Brin"),
        character("same", "Marlo"),
    ])
    .unwrap();
    assert_eq!(
        cast.iter()
            .map(|c| (c.id().as_str(), c.name().as_str()))
            .collect::<Vec<_>>(),
        [("same", "Ada"), ("other", "Brin"), ("same-2", "Marlo")]
    );
    let original = vec![character("second", "Ada"), character("first", "Ada")];
    assert_eq!(
        CharacterCast::new(original.clone())
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        original
    );
}

#[test]
fn domain_constructors_reject_invalid_state() {
    assert!(CharacterId::new(" ").is_err());
    assert!(CharacterName::new("\n").is_err());
    assert!(WorldDescription::new("").is_err());
    assert!(CurrentSituation::new(" ").is_err());
    assert!(CharacterDetails::new(None, None, None, None).is_err());
    assert!(CharacterCast::new(vec![]).is_err());
}

#[test]
fn suffixing_preserves_existing_ids_and_repaired_ids_support_future_updates() {
    let cast = CharacterCast::new([
        character("ajax", "Ajax"),
        Character::new(
            CharacterId::new("ajax").unwrap(),
            CharacterName::new("Ajax").unwrap(),
            CharacterDetails::new(
                Some(CharacterDescription::new("the other Ajax").unwrap()),
                None,
                None,
                None,
            )
            .unwrap(),
        ),
        character("ajax-2", "Brin"),
        character("ajax", "Marlo"),
    ])
    .unwrap();
    assert_eq!(
        cast.iter().map(|c| c.id().as_str()).collect::<Vec<_>>(),
        ["ajax", "ajax-3", "ajax-2", "ajax-4"]
    );
    let next = cast.updated(&[by_id("ajax-3", "at the gate")]);
    assert_eq!(
        next.get(&CharacterId::new("ajax-3").unwrap())
            .unwrap()
            .details()
            .current_state()
            .map(CharacterSituation::as_str),
        Some("at the gate")
    );
    assert_eq!(
        next.get(&CharacterId::new("ajax").unwrap()),
        cast.get(&CharacterId::new("ajax").unwrap())
    );
}

#[test]
fn character_ids_follow_calibre_slug_rules_and_character_not_byte_limit() {
    for (name, expected) in [
        ("  The Stranger ", "the-stranger"),
        ("Ada Lovelace-Smith", "ada-lovelace-smith"),
        (" ?! ", "character"),
    ] {
        assert_eq!(CharacterId::for_name(name).as_str(), expected);
    }
    assert_eq!(
        CharacterId::for_name(&"x".repeat(40)).as_str(),
        "x".repeat(32)
    );
    assert_eq!(
        CharacterId::for_name(&"界".repeat(40)).as_str(),
        "界".repeat(32)
    );
    assert_eq!(
        CharacterId::for_name(&format!("{} b", "a".repeat(31))).as_str(),
        "a".repeat(31)
    );
}

#[test]
fn exact_duplicates_collapse_before_suffixes_are_assigned() {
    let ada = character("same", "Ada");
    let brin = character("same", "Brin");
    let cast = CharacterCast::new([
        ada.clone(),
        brin.clone(),
        ada.clone(),
        brin,
        character("same", "Marlo"),
    ])
    .unwrap();
    assert_eq!(
        cast.iter()
            .map(|c| (c.id().as_str(), c.name().as_str()))
            .collect::<Vec<_>>(),
        [("same", "Ada"), ("same-2", "Brin"), ("same-3", "Marlo")]
    );
    assert_eq!(
        CharacterCast::new([ada.clone(), ada.clone()])
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        [&ada]
    );
    assert_eq!(CharacterCast::new(cast.iter().cloned()).unwrap(), cast);
}

#[test]
fn exact_deduplication_includes_identity_and_all_character_details() {
    let base = character("ajax", "Ajax");
    let details = base.details();
    let variants = [
        base.clone(),
        character("other-ajax", "Ajax"),
        Character::new(
            base.id().clone(),
            base.name().clone(),
            CharacterDetails::new(
                Some(CharacterDescription::new("different description").unwrap()),
                details.backstory().cloned(),
                None,
                None,
            )
            .unwrap(),
        ),
        Character::new(
            base.id().clone(),
            base.name().clone(),
            CharacterDetails::new(
                details.description().cloned(),
                Some(Backstory::new("different history").unwrap()),
                None,
                None,
            )
            .unwrap(),
        ),
        Character::new(
            base.id().clone(),
            base.name().clone(),
            CharacterDetails::new(
                details.description().cloned(),
                details.backstory().cloned(),
                Some(Relationships::new("knows Ada").unwrap()),
                None,
            )
            .unwrap(),
        ),
        Character::new(
            base.id().clone(),
            base.name().clone(),
            CharacterDetails::new(
                details.description().cloned(),
                details.backstory().cloned(),
                None,
                Some(CharacterSituation::new("at the gate").unwrap()),
            )
            .unwrap(),
        ),
    ];
    let cast = CharacterCast::new(variants.clone()).unwrap();
    assert_eq!(cast.len(), variants.len());
    for (actual, original) in cast.iter().zip(&variants) {
        assert_eq!(actual.name(), original.name());
        assert_eq!(actual.details(), original.details());
    }
}

#[test]
fn each_nonblank_character_field_changes_independently() {
    let old = initial();
    let next = merge_characters(
        &old,
        vec![CharacterDelta {
            description: CharacterDescription::new(" now scarred ").ok(),
            backstory: Backstory::new(" built the new engines ").ok(),
            relationships: Relationships::new(" trusts Marlo ").ok(),
            ..by_id("protagonist", " blinking in the sun ")
        }],
    );
    let details = next.characters().iter().next().unwrap().details();
    assert_eq!(
        details.description().map(CharacterDescription::as_str),
        Some("now scarred")
    );
    assert_eq!(
        details.backstory().map(Backstory::as_str),
        Some("built the new engines")
    );
    assert_eq!(
        details.relationships().map(Relationships::as_str),
        Some("trusts Marlo")
    );
    assert_eq!(
        details.current_state().map(CharacterSituation::as_str),
        Some("blinking in the sun")
    );
    let unchanged = merge_characters(
        &next,
        vec![CharacterDelta {
            description: CharacterDescription::new(" \n ").ok(),
            backstory: Backstory::new(" \t ").ok(),
            relationships: Relationships::new(" ").ok(),
            ..by_id("protagonist", "")
        }],
    );
    assert_eq!(unchanged, next);
}

#[test]
fn name_aliases_only_last_for_the_current_turn() {
    let cast = CharacterCast::new(vec![character("the-stranger", "The Stranger")]).unwrap();
    let renamed = cast.updated(&[CharacterDelta {
        name: Some(CharacterName::new("Marlo").unwrap()),
        ..by_id("the-stranger", "revealed")
    }]);
    let next = renamed.updated(&[CharacterDelta {
        name: Some(CharacterName::new("The Stranger").unwrap()),
        description: CharacterDescription::new("someone else").ok(),
        ..Default::default()
    }]);
    assert_eq!(next.len(), 2);
    assert_eq!(next.iter().next().unwrap().name().as_str(), "Marlo");
    assert_eq!(next.iter().nth(1).unwrap().id().as_str(), "the-stranger-2");
}

#[test]
fn explicit_ids_are_preserved_and_duplicate_names_match_the_last_entry() {
    let cast =
        CharacterCast::new(vec![character("First", "Ada"), character("Second", "Ada")]).unwrap();
    let next = cast.updated(&[CharacterDelta {
        name: Some(CharacterName::new("ada").unwrap()),
        current_state: CharacterSituation::new("changed").ok(),
        ..Default::default()
    }]);
    assert_eq!(next.iter().next().unwrap(), cast.iter().next().unwrap());
    assert_eq!(next.iter().nth(1).unwrap().id().as_str(), "Second");
    assert_eq!(
        next.iter()
            .nth(1)
            .unwrap()
            .details()
            .current_state()
            .map(CharacterSituation::as_str),
        Some("changed")
    );
    let added = next.updated(&[CharacterDelta {
        id: Some(CharacterId::new(" Explicit ID ").unwrap()),
        name: Some(CharacterName::new("Brin").unwrap()),
        backstory: Backstory::new("a local").ok(),
        ..Default::default()
    }]);
    assert_eq!(added.iter().nth(2).unwrap().id().as_str(), "Explicit ID");
}

#[test]
fn merging_deduplicates_before_truncating_and_preserves_configured_limit() {
    let old = initial();
    let small = StorySummary::new(
        old.world().clone(),
        old.current_situation().clone(),
        old.characters().clone(),
        MajorEvents::new(
            EventList::new(["first", "second"]),
            MajorEventLimit::new(2).unwrap(),
        ),
        EventList::default(),
    );
    let next = small.updated(&SummaryUpdate {
        new_major_events: EventList::new(["third", "FIRST"]),
        ..Default::default()
    });
    assert_eq!(events(next.major_events().events()), ["second", "third"]);
    assert_eq!(next.major_events().limit().get(), 2);
    assert_eq!(events(small.major_events().events()), ["first", "second"]);
}

#[test]
fn cast_supports_id_lookup_without_changing_order_after_rename_or_insertion() {
    let cast = CharacterCast::new([
        character("z-last", "The Stranger"),
        character("a-first", "Ada"),
    ])
    .unwrap();
    let id = CharacterId::new("z-last").unwrap();
    assert_eq!(cast.get(&id).unwrap().name().as_str(), "The Stranger");
    assert!(cast.get(&CharacterId::new("missing").unwrap()).is_none());
    let next = cast.updated(&[
        CharacterDelta {
            name: Some(CharacterName::new("Marlo").unwrap()),
            ..by_id("z-last", "revealed")
        },
        CharacterDelta {
            name: Some(CharacterName::new("Brin").unwrap()),
            description: Some(CharacterDescription::new("a thief").unwrap()),
            ..Default::default()
        },
    ]);
    assert_eq!(next.get(&id).unwrap().name().as_str(), "Marlo");
    assert_eq!(
        next.iter()
            .map(|character| character.id().as_str())
            .collect::<Vec<_>>(),
        ["z-last", "a-first", "brin"]
    );
    assert_eq!(next.len(), 3);
    assert!(!next.is_empty());
    assert_eq!(cast.get(&id).unwrap().name().as_str(), "The Stranger");
}

#[test]
fn cast_equality_includes_presentation_order() {
    let ada = character("ada", "Ada");
    let brin = character("brin", "Brin");
    let first = CharacterCast::new([ada.clone(), brin.clone()]).unwrap();
    let reversed = CharacterCast::new([brin, ada]).unwrap();
    assert_eq!(first, first.clone());
    assert_ne!(first, reversed);
}
