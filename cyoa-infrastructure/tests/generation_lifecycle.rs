use cyoa_core::{
    game::GameState,
    limits::Limits,
    style::StoryStyle,
    text::Brief,
    world::{PlayablePosition, World, WorldCast, WorldOutline},
};
use cyoa_infrastructure::generation::{
    prompts::{cast_generation_prompt, turn_prompt, world_generation_prompt},
    wire::{GeneratedCastWire, WorldOutlineWire, playable_and_npcs_from_wire},
};

#[test]
fn outline_to_selected_world_requires_no_placeholder_cast() {
    let brief = Brief::new("Two Ajaxes at a harbour").unwrap();
    let (_, request) = world_generation_prompt(&brief);
    assert!(request.contains(brief.as_str()));
    let outline = WorldOutline::try_from(
        serde_json::from_str::<WorldOutlineWire>(
            r#"{"title":"Harbour","world_description":"Ships shelter behind a stone wall."}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let limits = Limits::default();
    let (_, request) = cast_generation_prompt(&brief, &outline, &limits);
    assert!(request.contains(outline.title().as_str()));
    assert!(request.contains(outline.description().as_str()));
    let response: GeneratedCastWire = serde_json::from_str(r#"{
        "characters":[
            {"name":"Ajax","description":"A sailor","backstory":"Grew up at sea"},
            {"name":"Ajax","description":"A mason","backstory":"Built the harbour wall"}
        ],
        "npcs":[
            {"name":"Ajax","description":"A merchant","backstory":"Sells sails","relationships":"Knows the sailor"},
            {"name":"Ajax","description":"A merchant","backstory":"Sells sails","relationships":"Knows the sailor"},
            {"name":"Ajax","description":"A guard","backstory":"Guards the wall","relationships":"Knows the mason"}
        ]
    }"#).unwrap();
    let (players, npcs) = playable_and_npcs_from_wire(response);
    let cast = WorldCast::new(players, npcs, &limits).unwrap();
    assert_eq!(cast.playable().len(), 2);
    assert_eq!(cast.npcs().len(), 2);
    let selected = World::new(outline, cast)
        .select(PlayablePosition::new(1))
        .unwrap();
    let state = GameState::start(brief, selected, StoryStyle::default(), limits);
    assert_eq!(state.protagonist().description().as_str(), "A mason");
    let summary = state.current_summary();
    let ids: Vec<_> = summary
        .characters()
        .iter()
        .map(|c| c.id().as_str())
        .collect();
    assert_eq!(ids, ["protagonist", "ajax", "ajax-2"]);
    let opening = turn_prompt(&state, None, false);
    for id in ids {
        assert!(opening.contains(&format!("\"id\": \"{id}\"")));
    }
}
