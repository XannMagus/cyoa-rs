//! Convenience adapters for retained regressions. Production exposes fallible,
//! instance-owned rendering only; unwraps here fail the test on any render error.
#![allow(dead_code)]

/// Real-process test harness for `backend_contract.rs` only (needs
/// `CARGO_BIN_EXE_subprocess_fixture`, set only for this crate's own tests).
pub mod fixture_backend;

use cyoa_core::{
    game::GameState,
    limits::Limits,
    style::StoryStyle,
    text::Brief,
    world::{PlayerCharacter, WorldOutline},
};
use cyoa_infrastructure::generation::templates::GenerationTemplates;
fn templates() -> &'static GenerationTemplates {
    static TEMPLATES: std::sync::OnceLock<GenerationTemplates> = std::sync::OnceLock::new();
    TEMPLATES.get_or_init(|| GenerationTemplates::bundled().unwrap())
}
pub fn default_prompts() -> toml::Table {
    toml::from_str(include_str!("../../src/generation/defaults/prompts.toml")).unwrap()
}
pub fn world_generation_prompt(brief: &Brief) -> (String, String) {
    let r = templates().world_request(brief).unwrap();
    (r.instructions().as_str().into(), r.prompt().as_str().into())
}
pub fn cast_generation_prompt(
    brief: &Brief,
    world: &WorldOutline,
    limits: &Limits,
) -> (String, String) {
    let r = templates().cast_request(brief, world, limits).unwrap();
    (r.instructions().as_str().into(), r.prompt().as_str().into())
}
pub fn turn_instructions(
    style: &StoryStyle,
    world: &WorldOutline,
    player: &PlayerCharacter,
    limits: &Limits,
) -> String {
    templates()
        .turn_instructions(style, world, player, limits)
        .unwrap()
        .as_str()
        .into()
}
pub fn turn_prompt(state: &GameState, input: Option<&str>, interesting: bool) -> String {
    templates()
        .turn_prompt(state, input, interesting)
        .unwrap()
        .as_str()
        .into()
}
pub fn generated_cast_schema(limits: &Limits) -> serde_json::Value {
    templates().cast_schema(limits).unwrap()
}
pub fn story_turn_schema(limits: &Limits) -> serde_json::Value {
    templates().turn_schema(limits).unwrap()
}
