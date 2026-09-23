use cyoa_core::{
    game::GameState,
    limits::{Limits, MajorEventLimit},
    style::StoryStyle,
    summary::{EventList, SummaryUpdate},
    text::*,
    turn::*,
    world::*,
};
fn player(name: &str, description: &str) -> PlayerCharacter {
    PlayerCharacter::new(
        CharacterName::new(name).unwrap(),
        CharacterDescription::new(description).unwrap(),
        Backstory::new("history").unwrap(),
    )
}
fn cast(names: &[&str]) -> WorldCast {
    WorldCast::new(
        names.iter().map(|n| player(n, "description")),
        [],
        &Limits::default(),
    )
    .unwrap()
}
fn world(cast: WorldCast) -> World {
    World::new(
        WorldOutline::new(
            WorldTitle::new("Title").unwrap(),
            WorldDescription::new("World").unwrap(),
        ),
        cast,
    )
}
fn record() -> TurnGenerationRecord {
    TurnGenerationRecord {
        input: None,
        raw_response: RawResponse::new("{}").unwrap(),
        provenance: GenerationProvenance::default(),
        prompt_trace: None,
    }
}
fn turn(update: SummaryUpdate) -> StoryTurn {
    StoryTurn::new(
        Narrative::new("Story").unwrap(),
        QuickActions::select([QuickAction::new(
            QuickActionText::new("Go").unwrap(),
            QuickActionKind::Bold,
        )])
        .unwrap(),
        None,
        update,
        ChapterMarker::Continue { title: None },
    )
}
fn main() {
    let namesakes = WorldCast::new(
        [
            player("Ajax", "Telamon's son"),
            player("Ajax", "Oileus's son"),
            player("Odysseus", "Ithaca's king"),
        ],
        [],
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(namesakes.playable().len(), 2);
    println!(
        "R1: distinct Ajax playables retained: {}",
        namesakes
            .playable()
            .iter()
            .filter(|p| p.name().as_str() == "Ajax")
            .count()
    );
    let large = cast(&["A", "B", "C"]);
    let small = cast(&["D", "E"]);
    let foreign_index = large.playable_index(2).unwrap();
    assert!(std::panic::catch_unwind(|| small.playable_at(foreign_index)).is_err());
    let wrong_person = small.playable_at(large.playable_index(0).unwrap());
    println!(
        "R2: foreign index panics; in-range foreign index silently selects {}",
        wrong_person.name().as_str()
    );
    for text in [
        RawResponse::new(" \n{}\n").unwrap().as_str(),
        Instructions::new(" \n{}\n").unwrap().as_str(),
        RenderedPrompt::new(" \n{}\n").unwrap().as_str(),
    ] {
        assert_eq!(text, "{}");
    }
    println!("R4: all three audit text wrappers strip boundary whitespace");
    let w = world(cast(&["A", "B"]));
    let index = w.cast().playable_index(0).unwrap();
    let mut original = GameState::start(
        Brief::new("Brief").unwrap(),
        w.clone(),
        index,
        StoryStyle::default(),
        Limits::default(),
    );
    original.commit_turn(
        turn(SummaryUpdate {
            new_major_events: EventList::new(["one", "two", "three"]),
            ..SummaryUpdate::default()
        }),
        record(),
    );
    let limits = Limits {
        max_major_events: MajorEventLimit::new(1).unwrap(),
        ..Limits::default()
    };
    let mut restored = GameState::restore(
        Brief::new("Brief").unwrap(),
        w,
        index,
        StoryStyle::default(),
        limits,
        original.turns().to_vec(),
    );
    restored.commit_turn(
        turn(SummaryUpdate {
            new_major_events: EventList::new(["four"]),
            ..SummaryUpdate::default()
        }),
        record(),
    );
    let summary = restored.current_summary();
    let count = summary.major_events().events().iter().count();
    assert_eq!(count, 4);
    println!(
        "R5: configured major-event limit {}, actual count {}, carried limit {}",
        restored.limits().max_major_events.get(),
        count,
        summary.major_events().limit().get()
    );
}
