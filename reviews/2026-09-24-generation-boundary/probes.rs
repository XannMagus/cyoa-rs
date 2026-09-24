use cyoa_core::{
    limits::{Limits, MaxGeneratedNpcs, MinPlayableCharacters},
    style::StoryStyle,
    text::*,
    world::*,
};
use cyoa_infrastructure::generation::{prompts::*, schema::*, wire::*};
fn world() -> World {
    let players = ["Alex", "Blair"].map(|n| {
        PlayerCharacter::new(
            CharacterName::new(n).unwrap(),
            CharacterDescription::new("description").unwrap(),
            Backstory::new("history").unwrap(),
        )
    });
    World::new(
        WorldOutline::new(
            WorldTitle::new("Title").unwrap(),
            WorldDescription::new("A world").unwrap(),
        ),
        WorldCast::new(players, [], &Limits::default()).unwrap(),
    )
}
fn main() {
    let w = world();
    for maximum in 0..=2 {
        let limits = Limits {
            max_generated_npcs: MaxGeneratedNpcs::new(maximum),
            ..Limits::default()
        };
        let (instructions, _) = cast_generation_prompt(&Brief::new("brief").unwrap(), &w, &limits);
        assert!(instructions.contains(&format!("between three and {maximum} other characters")));
        let schema = generated_cast_schema(&limits);
        println!(
            "R2 NPC cap {maximum}: {}",
            schema["properties"]["npcs"]["description"]
        );
    }
    let limits = Limits {
        min_playable_characters: MinPlayableCharacters::new(6).unwrap(),
        ..Limits::default()
    };
    let (instructions, _) = cast_generation_prompt(&Brief::new("brief").unwrap(), &w, &limits);
    assert!(instructions.contains("between three and five distinct playable characters"));
    println!("R2 minimum six: instructions still ask for three to five playables");
    let mut invalid = default_prompts();
    invalid["prose_contract"]
        .as_table_mut()
        .unwrap()
        .insert("dialogue_and_sensory".into(), toml::Value::Integer(42));
    assert!(startup_self_check(&invalid).is_ok());
    assert!(
        std::panic::catch_unwind(|| prose_contract(
            &environment(),
            &invalid,
            &StoryStyle::default()
        ))
        .is_err()
    );
    println!("R3 wrong template type: startup check succeeds, runtime renderer panics");
    let mut misplaced_variable = default_prompts();
    misplaced_variable["fragments"]
        .as_table_mut()
        .unwrap()
        .insert("markdown".into(), toml::Value::String("{{ brief }}".into()));
    assert!(startup_self_check(&misplaced_variable).is_ok());
    assert!(
        std::panic::catch_unwind(|| prose_contract(
            &environment(),
            &misplaced_variable,
            &StoryStyle::default()
        ))
        .is_err()
    );
    println!("R3 wrong context: startup check succeeds, runtime renderer panics");
    let schema = story_turn_schema(&Limits::default());
    assert!(
        schema["properties"]["chapter_title"]["description"]
            .as_str()
            .unwrap()
            .contains("null otherwise")
    );
    println!(
        "R4 chapter title instruction: {}",
        schema["properties"]["chapter_title"]["description"]
    );
    let npc = serde_json::from_value::<NonPlayerCharacterWire>(
        serde_json::json!({"name":"Casey","description":"guard","backstory":"history"}),
    )
    .unwrap();
    assert!(npc.relationships.is_empty());
    let turn = serde_json::from_value::<StoryTurnWire>(serde_json::json!({"narrative":"Story", "quick_actions":[{"text":"Go"}], "summary_update":{"current_situation":"Here"}, "starts_new_chapter":false,"chapter_title":null})).unwrap();
    assert!(turn.scene_description.is_empty());
    println!("R5 missing required Python fields relationships and scene_description both accepted");
    std::fs::write(
        "cast.schema.json",
        serde_json::to_string_pretty(&generated_cast_schema(&Limits::default())).unwrap(),
    )
    .unwrap();
    std::fs::write(
        "turn.schema.json",
        serde_json::to_string_pretty(&schema).unwrap(),
    )
    .unwrap();
}
