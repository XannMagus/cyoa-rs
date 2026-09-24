//! Source DTOs. Serde validates bundled files too, so the bundled text cannot
//! silently redefine the shape accepted by its own override checks.
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PromptConfiguration {
    version: u32,
    fragments: Fragments,
    world: Generation,
    cast: Generation,
    quick_actions: QuickActions,
    prose_contract: Prose,
    turn: Turn,
    premade_worlds: PremadeWorlds,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Fragments {
    designer_role: String,
    markdown: String,
    designer_role_ref: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Generation {
    instructions: String,
    prompt: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Prose {
    dialogue_and_sensory: String,
    example_all_defaults: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct QuickActions {
    requested: Vec<Vec<String>>,
    kinds: Vec<ActionKind>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionKind {
    key: String,
    label: String,
    meaning: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Turn {
    role: String,
    rule_narrative: String,
    rule_scene_description: String,
    rule_summary_update: String,
    rule_character_updates: String,
    rule_character_ids: String,
    rule_new_major_events: String,
    rule_upcoming_events: String,
    rule_current_situation: String,
    rule_starts_new_chapter: String,
    rule_field_order: String,
    world_and_protagonist_template: String,
    prompt_parts: TurnParts,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TurnParts {
    summary_header: String,
    bridge_header: String,
    transcript_header: String,
    interesting_event_prompt: String,
    interesting_event_with_threads_header: String,
    interesting_event_fallback: String,
    player_directs: String,
    player_no_direction: String,
    continue_seamlessly: String,
    opening_with_input: String,
    opening_instruction: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PremadeWorlds {
    entries: Vec<PremadeWorld>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PremadeWorld {
    title: String,
    art_style: String,
    tone: String,
    brief: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StyleConfiguration {
    version: u32,
    art_style: Vec<Style>,
    pace: Vec<Style>,
    tone: Vec<Style>,
    narration: Vec<Style>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Style {
    key: String,
    name: String,
    prompt: String,
}
