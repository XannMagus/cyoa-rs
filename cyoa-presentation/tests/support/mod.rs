#![allow(dead_code)]
use cyoa_application::generation::{FailureKind, Generated, GenerationFailure};
use cyoa_core::{
    game::GameState, limits::Limits, style::StoryStyle, summary::SummaryUpdate, text::*, turn::*,
    world::*,
};
pub fn outline(title: &str) -> WorldOutline {
    WorldOutline::new(
        WorldTitle::new(title).unwrap(),
        WorldDescription::new("Harbour").unwrap(),
    )
}
pub fn generated<T>(value: T) -> Generated<T> {
    Generated::new(
        value,
        RawResponse::new(" exact é\r\n"),
        GenerationProvenance::default(),
    )
}
pub fn world() -> World {
    let player = |history| {
        PlayerCharacter::new(
            CharacterName::new("Ajax").unwrap(),
            CharacterDescription::new("keeper").unwrap(),
            Backstory::new(history).unwrap(),
        )
    };
    World::new(
        outline("Harbour"),
        WorldCast::new([player("north"), player("south")], [], &Limits::default()).unwrap(),
    )
}
pub fn game() -> GameState {
    GameState::start(
        Brief::new("bell").unwrap(),
        world().select(PlayablePosition::new(0)).unwrap(),
        StoryStyle::default(),
        Limits::default(),
    )
}
pub fn turn(text: &str, chapter: ChapterMarker) -> StoryTurn {
    StoryTurn::new(
        Narrative::new(text).unwrap(),
        QuickActions::select([QuickAction::new(
            QuickActionText::new("Search 雪").unwrap(),
            QuickActionKind::Investigate,
        )])
        .unwrap(),
        None,
        SummaryUpdate::default(),
        chapter,
    )
}
pub fn committed(mut game: GameState) -> GameState {
    game.commit_turn(
        turn("Final prose", ChapterMarker::Continue { title: None }),
        TurnGenerationRecord {
            input: None,
            raw_response: RawResponse::new("raw"),
            provenance: GenerationProvenance::default(),
            prompt_trace: None,
        },
    );
    game
}
pub fn failure() -> GenerationFailure {
    GenerationFailure::new(
        FailureKind::InvalidResponse,
        "bad response",
        RawResponse::new("candidate é\r\n"),
        cyoa_application::diagnostics::TransportDiagnostics::new(vec![255, 13, 10], vec![254]),
    )
}
