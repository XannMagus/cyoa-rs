use cyoa_core::{
    game::GameState,
    limits::{Limits, MaxGeneratedNpcs, MinPlayableCharacters, RestoreLimits},
    style::StoryStyle,
    text::{Backstory, Brief, CharacterDescription, CharacterName, WorldDescription, WorldTitle},
    world::{
        InvalidRestoredWorldCast, NonPlayerCharacter, PlayablePosition, PlayerCharacter, World,
        WorldCast, WorldOutline,
    },
};

fn player(description: &str) -> PlayerCharacter {
    PlayerCharacter::new(
        CharacterName::new("Ajax").unwrap(),
        CharacterDescription::new(description).unwrap(),
        Backstory::new("history").unwrap(),
    )
}
fn npc(description: &str) -> NonPlayerCharacter {
    NonPlayerCharacter::new(
        CharacterName::new("Ajax").unwrap(),
        CharacterDescription::new(description).unwrap(),
        Backstory::new("history").unwrap(),
        None,
    )
}

#[test]
fn restored_cast_rejects_empty_playables_and_exact_duplicates_in_each_role() {
    assert_eq!(
        WorldCast::restore(vec![], vec![npc("npc")]).unwrap_err(),
        InvalidRestoredWorldCast::EmptyPlayable
    );
    assert_eq!(
        WorldCast::restore(vec![player("one"), player("one")], vec![]).unwrap_err(),
        InvalidRestoredWorldCast::DuplicatePlayable
    );
    assert_eq!(
        WorldCast::restore(vec![player("one")], vec![npc("one"), npc("one")]).unwrap_err(),
        InvalidRestoredWorldCast::DuplicateNpc
    );
}

#[test]
fn restored_cast_preserves_namesakes_role_order_and_checked_selection() {
    let playable = vec![player("second"), player("first")];
    let npcs = vec![npc("second"), npc("first")];
    let cast = WorldCast::restore(playable.clone(), npcs.clone()).unwrap();
    assert_eq!(cast.playable(), playable);
    assert_eq!(cast.npcs(), npcs);
    let world = World::new(
        WorldOutline::new(
            WorldTitle::new("title").unwrap(),
            WorldDescription::new("world").unwrap(),
        ),
        cast,
    );
    assert!(world.clone().select(PlayablePosition::new(2)).is_err());
    let selected = world.select(PlayablePosition::new(1)).unwrap();
    assert_eq!(selected.protagonist(), &playable[1]);
    assert_eq!(selected.position().get(), 1);
}

#[test]
fn established_cast_is_not_pruned_or_rejected_by_restore_generation_limits() {
    let cast = WorldCast::restore(vec![player("one")], vec![npc("one"), npc("two")]).unwrap();
    let selected = World::new(
        WorldOutline::new(
            WorldTitle::new("title").unwrap(),
            WorldDescription::new("world").unwrap(),
        ),
        cast.clone(),
    )
    .select(PlayablePosition::new(0))
    .unwrap();
    let current = Limits {
        max_generated_npcs: MaxGeneratedNpcs::new(0),
        min_playable_characters: MinPlayableCharacters::new(6).unwrap(),
        ..Limits::default()
    };
    let game = GameState::restore(
        Brief::new("brief").unwrap(),
        selected,
        StoryStyle::default(),
        Limits::default(),
        RestoreLimits::Current(current),
        vec![],
    );
    assert_eq!(game.world().cast(), &cast);
    assert_eq!(game.current_summary().characters().len(), 3);
    assert_eq!(game.limits(), current);
    assert_eq!(game.original_limits(), Limits::default());
    assert!(WorldCast::new(vec![player("one")], vec![], &current).is_err());
}

#[test]
fn restored_single_playable_and_no_npcs_is_a_valid_zero_turn_game() {
    let cast = WorldCast::restore(vec![player("alone")], vec![]).unwrap();
    assert_eq!(cast.playable(), &[player("alone")]);
    assert!(cast.npcs().is_empty());
    let selected = World::new(
        WorldOutline::new(
            WorldTitle::new("title").unwrap(),
            WorldDescription::new("world").unwrap(),
        ),
        cast,
    )
    .select(PlayablePosition::new(0))
    .unwrap();
    let game = GameState::start(
        Brief::new("brief").unwrap(),
        selected,
        StoryStyle::default(),
        Limits::default(),
    );
    assert!(game.turns().is_empty());
    assert_eq!(game.protagonist(), &player("alone"));
    assert_eq!(game.current_summary().characters().len(), 1);
}
