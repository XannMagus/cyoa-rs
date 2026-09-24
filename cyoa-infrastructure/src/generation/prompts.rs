//! TOML-loaded prompt templates rendered with minijinja. One function per
//! call site, taking `cyoa-core` domain types by reference. See PLAN.md's
//! "TOML prompt externalization" section, and `turn_prompt`'s control flow
//! below, transcribed directly from `reference/calibre/cyoa.py:1081-1127`.

use std::sync::OnceLock;

use cyoa_core::{
    game::GameState,
    limits::Limits,
    style::StoryStyle,
    summary::StorySummary,
    text::Brief,
    turn::TurnRecord,
    world::{PlayerCharacter, World, WorldOutline},
};
use minijinja::{Environment, UndefinedBehavior, context, value::Value as JinjaValue};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

const PROMPTS_TOML: &str = include_str!("defaults/prompts.toml");
const STYLES_TOML: &str = include_str!("defaults/styles.toml");
const PROMPT_ADDITIONS_TOML: &str = include_str!("defaults/prompt_additions.toml");

fn parse_toml(text: &str) -> toml::Table {
    text.parse()
        .unwrap_or_else(|error| panic!("failed to parse bundled prompt TOML: {error}"))
}

pub fn default_prompts() -> toml::Table {
    parse_toml(PROMPTS_TOML)
}

fn default_prompt_additions() -> toml::Table {
    parse_toml(PROMPT_ADDITIONS_TOML)
}

fn cached_prompts() -> &'static toml::Table {
    static PROMPTS: OnceLock<toml::Table> = OnceLock::new();
    PROMPTS.get_or_init(default_prompts)
}

#[derive(Debug, Deserialize)]
struct StyleEntry {
    key: String,
    #[allow(dead_code)]
    name: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct StylesFile {
    art_style: Vec<StyleEntry>,
    pace: Vec<StyleEntry>,
    tone: Vec<StyleEntry>,
    narration: Vec<StyleEntry>,
}

fn styles() -> &'static StylesFile {
    static STYLES: OnceLock<StylesFile> = OnceLock::new();
    STYLES.get_or_init(|| {
        toml::from_str(STYLES_TOML).unwrap_or_else(|error| panic!("defaults/styles.toml: {error}"))
    })
}

/// Falls back to the first (default) entry for an unknown/absent key
/// (calibre `style_for_key`, cyoa.py:868-875).
fn style_for_key<'a>(table: &'a [StyleEntry], key: Option<&str>) -> &'a StyleEntry {
    key.and_then(|k| table.iter().find(|entry| entry.key == k))
        .unwrap_or(&table[0])
}

/// Deep-merges `override_` into `base`, per-key at every nested table level —
/// a leaf value one turn deep in `base` that `override_` never mentions is
/// left untouched, no matter how many sibling keys `override_` does set
/// (PLAN.md: "merge per-key, not per-file").
pub fn merge_per_key(base: &mut toml::Value, override_: &toml::Value) {
    match (base, override_) {
        (toml::Value::Table(base_table), toml::Value::Table(override_table)) => {
            for (key, value) in override_table {
                match base_table.get_mut(key) {
                    Some(existing) => merge_per_key(existing, value),
                    None => {
                        base_table.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (slot, value) => *slot = value.clone(),
    }
}

pub fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env
}

fn render(env: &Environment<'_>, template: &str, ctx: JinjaValue) -> String {
    env.render_str(template, ctx)
        .unwrap_or_else(|error| panic!("prompt template failed to render: {error}"))
}

/// Renders every string in `prompts`/the style tables against a synthetic
/// context covering every variable any default template references.
/// `UndefinedBehavior::Strict` means a broken override (a typo'd variable, a
/// stray `{{ }}`) fails here, at startup, rather than mutilating a prompt on
/// turn 40.
pub fn startup_self_check(prompts: &toml::Table) -> Result<(), minijinja::Error> {
    let env = environment();
    let ctx = context! {
        brief => "a synthetic brief",
        max_generated_npcs => 8,
        max_major_events => 30,
        what => "synthetic descriptive text",
        world => context! {
            title => "Synthetic World",
            world_description => "A synthetic world description.",
        },
        protagonist => context! {
            name => "Protagonist",
            description => "a synthetic description",
            backstory => "a synthetic backstory",
        },
        player_input => "a synthetic direction",
    };
    check_value(&env, &toml::Value::Table(prompts.clone()), &ctx)?;
    for entry in styles()
        .art_style
        .iter()
        .chain(&styles().pace)
        .chain(&styles().tone)
        .chain(&styles().narration)
    {
        env.render_str(&entry.prompt, ctx.clone())?;
    }
    Ok(())
}

fn check_value(
    env: &Environment<'_>,
    value: &toml::Value,
    ctx: &JinjaValue,
) -> Result<(), minijinja::Error> {
    match value {
        toml::Value::String(s) => env.render_str(s, ctx.clone()).map(|_| ()),
        toml::Value::Table(t) => {
            for v in t.values() {
                check_value(env, v, ctx)?;
            }
            Ok(())
        }
        toml::Value::Array(a) => {
            for v in a {
                check_value(env, v, ctx)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn str_at<'a>(table: &'a toml::Table, path: &[&str]) -> &'a str {
    let (last, init) = path.split_last().expect("non-empty path");
    let mut current = table;
    for segment in init {
        current = current
            .get(*segment)
            .and_then(toml::Value::as_table)
            .unwrap_or_else(|| panic!("prompts.toml[{}] is missing a table", path.join(".")));
    }
    current
        .get(*last)
        .and_then(toml::Value::as_str)
        .unwrap_or_else(|| panic!("prompts.toml[{}] is missing a string", path.join(".")))
}

fn markdown_fragment(env: &Environment<'_>, prompts: &toml::Table, what: &str) -> String {
    render(
        env,
        str_at(prompts, &["fragments", "markdown"]),
        context! { what },
    )
}

/// `cyoa.py:984-1004`: space-joined, the tone clause entirely omitted when
/// the resolved tone's prompt is blank (the "default" tone).
pub fn prose_contract(env: &Environment<'_>, prompts: &toml::Table, style: &StoryStyle) -> String {
    let pace = style_for_key(&styles().pace, style.pace.as_ref().map(|k| k.as_str()));
    let narration = style_for_key(
        &styles().narration,
        style.narration.as_ref().map(|k| k.as_str()),
    );
    let tone = style_for_key(&styles().tone, style.tone.as_ref().map(|k| k.as_str()));

    let mut parts = vec![
        pace.prompt.clone(),
        format!(
            "Write it {}, and keep to that throughout.",
            narration.prompt
        ),
    ];
    if !tone.prompt.is_empty() {
        parts.push(tone.prompt.clone());
    }
    parts.push(str_at(prompts, &["prose_contract", "dialogue_and_sensory"]).to_string());
    parts.push(markdown_fragment(
        env,
        prompts,
        "all narrative and descriptive text",
    ));
    parts.join(" ")
}

#[derive(Debug, Clone, Deserialize)]
struct QuickActionKindEntry {
    key: String,
    meaning: String,
}

/// Rendered dynamically from `[quick_actions.kinds]`/`requested`
/// (calibre `quick_action_instructions`, cyoa.py:961-981) — editing a kind's
/// meaning in TOML changes this text; it is not a baked string.
pub fn quick_action_instructions(prompts: &toml::Table) -> String {
    let quick_actions = prompts
        .get("quick_actions")
        .and_then(toml::Value::as_table)
        .expect("prompts.toml[quick_actions] is present");
    let kinds: Vec<QuickActionKindEntry> = quick_actions
        .get("kinds")
        .cloned()
        .expect("prompts.toml[quick_actions.kinds] is present")
        .try_into()
        .expect("prompts.toml[quick_actions.kinds] matches its expected shape");
    let requested: Vec<Vec<String>> = quick_actions
        .get("requested")
        .cloned()
        .expect("prompts.toml[quick_actions.requested] is present")
        .try_into()
        .expect("prompts.toml[quick_actions.requested] matches its expected shape");

    let meaning_of = |key: &str| -> String {
        kinds
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.meaning.clone())
            .unwrap_or_default()
    };

    let mut lines = vec![
        "- quick_actions: exactly three actions the reader could have the protagonist take next, \
         each a short imperative phrase and each tagged with the kind of approach it takes. They \
         must be three genuinely different approaches to the situation, not three phrasings of \
         the obvious next step, so give one action of each of these kinds, in this order:"
            .to_string(),
    ];
    for group in &requested {
        let kinds_str = group
            .iter()
            .map(|k| format!("\"{k}\""))
            .collect::<Vec<_>>()
            .join(" or ");
        let meanings_str = group
            .iter()
            .map(|k| meaning_of(k))
            .collect::<Vec<_>>()
            .join("; or ");
        lines.push(format!("  * {kinds_str}: {meanings_str}."));
    }
    lines.push(
        "  Use whichever kind of the last group the scene affords. Use \"other\" only for an \
         action that genuinely fits none of the kinds. Every action must be something the \
         protagonist can actually do from where they are right now, and must follow from the \
         passage you have just written rather than from the story in general."
            .to_string(),
    );
    lines.join("\n")
}

/// LLM call #1 (calibre `world_generation_prompt`, cyoa.py:925-926):
/// `(instructions, prompt)`.
pub fn world_generation_prompt(brief: &Brief) -> (String, String) {
    let prompts = cached_prompts();
    let env = environment();
    let instructions = render(
        &env,
        str_at(prompts, &["world", "instructions"]),
        context! {},
    );
    let prompt = render(
        &env,
        str_at(prompts, &["world", "prompt"]),
        context! { brief => brief.as_str() },
    );
    (instructions, prompt)
}

/// LLM call #2 (calibre `cast_generation_prompt`, cyoa.py:948-954):
/// `(instructions, prompt)`.
pub fn cast_generation_prompt(brief: &Brief, world: &World, limits: &Limits) -> (String, String) {
    let prompts = cached_prompts();
    let env = environment();
    let instructions = render(
        &env,
        str_at(prompts, &["cast", "instructions"]),
        context! { max_generated_npcs => limits.max_generated_npcs.get() },
    );
    let prompt = render(
        &env,
        str_at(prompts, &["cast", "prompt"]),
        context! {
            world => context! {
                title => world.outline().title().as_str(),
                world_description => world.outline().description().as_str(),
            },
            brief => brief.as_str(),
        },
    );
    (instructions, prompt)
}

/// The system prompt sent with every turn (calibre `turn_instructions`,
/// cyoa.py:1007-1078), joined with newlines.
pub fn turn_instructions(
    style: &StoryStyle,
    world: &WorldOutline,
    protagonist: &PlayerCharacter,
    limits: &Limits,
) -> String {
    let prompts = cached_prompts();
    let env = environment();
    let parts = vec![
        render(
            &env,
            str_at(prompts, &["turn", "role"]),
            context! { protagonist => context! { name => protagonist.name().as_str() } },
        ),
        prose_contract(&env, prompts, style),
        "Rules for the fields of your response:".to_string(),
        str_at(prompts, &["turn", "rule_narrative"]).to_string(),
        quick_action_instructions(prompts),
        str_at(prompts, &["turn", "rule_scene_description"]).to_string(),
        str_at(prompts, &["turn", "rule_summary_update"]).to_string(),
        str_at(prompts, &["turn", "rule_character_updates"]).to_string(),
        str_at(prompts, &["turn", "rule_character_ids"]).to_string(),
        render(
            &env,
            str_at(prompts, &["turn", "rule_new_major_events"]),
            context! { max_major_events => limits.max_major_events.get() },
        ),
        str_at(prompts, &["turn", "rule_upcoming_events"]).to_string(),
        str_at(prompts, &["turn", "rule_current_situation"]).to_string(),
        str_at(prompts, &["turn", "rule_starts_new_chapter"]).to_string(),
        str_at(prompts, &["turn", "rule_field_order"]).to_string(),
        render(
            &env,
            str_at(prompts, &["turn", "world_and_protagonist_template"]),
            context! {
                world => context! {
                    title => world.title().as_str(),
                    world_description => world.description().as_str(),
                },
                protagonist => context! {
                    name => protagonist.name().as_str(),
                    description => protagonist.description().as_str(),
                    backstory => protagonist.backstory().as_str(),
                },
            },
        ),
    ];
    parts.join("\n")
}

fn summary_as_json(summary: &StorySummary) -> JsonValue {
    json!({
        "world": summary.world().as_str(),
        "major_events": summary.major_events().events().iter().map(|e| e.as_str()).collect::<Vec<_>>(),
        "characters": summary.characters().iter().map(|c| json!({
            "name": c.name().as_str(),
            "description": c.details().description().map(cyoa_core::text::CharacterDescription::as_str).unwrap_or(""),
            "backstory": c.details().backstory().map(cyoa_core::text::Backstory::as_str).unwrap_or(""),
            "relationships": c.details().relationships().map(cyoa_core::text::Relationships::as_str).unwrap_or(""),
            "current_state": c.details().current_state().map(cyoa_core::text::CharacterSituation::as_str).unwrap_or(""),
            "id": c.id().as_str(),
        })).collect::<Vec<_>>(),
        "current_situation": summary.current_situation().as_str(),
        "upcoming_events": summary.upcoming_events().iter().map(|e| e.as_str()).collect::<Vec<_>>(),
    })
}

fn add_prose(parts: &mut Vec<String>, turns: &[TurnRecord]) {
    for turn in turns {
        if let Some(input) = turn.input() {
            parts.push(format!("[The reader directs: {}]", input.as_str()));
            parts.push(String::new());
        }
        parts.push(turn.turn().narrative().as_str().to_string());
        parts.push(String::new());
    }
}

/// The user message sent with every turn (calibre `turn_prompt`,
/// cyoa.py:1081-1127) — see this module's doc comment for the control flow,
/// transcribed line-for-line from that function.
pub fn turn_prompt(
    state: &GameState,
    player_input: Option<&str>,
    interesting_event: bool,
) -> String {
    let prompts = cached_prompts();
    let env = environment();
    let summary = state.current_summary();
    let mut parts = vec![
        str_at(prompts, &["turn", "prompt_parts", "summary_header"]).to_string(),
        serde_json::to_string_pretty(&summary_as_json(&summary))
            .expect("StorySummary always serializes"),
        String::new(),
    ];

    let (bridge, transcript) = state.prose_context();
    if !bridge.is_empty() {
        parts.push(str_at(prompts, &["turn", "prompt_parts", "bridge_header"]).to_string());
        parts.push(String::new());
        add_prose(&mut parts, bridge);
    }
    if !transcript.is_empty() {
        parts.push(str_at(prompts, &["turn", "prompt_parts", "transcript_header"]).to_string());
        parts.push(String::new());
        add_prose(&mut parts, transcript);
    }

    let input = player_input.filter(|text| !text.trim().is_empty());
    if !state.turns().is_empty() {
        if interesting_event {
            parts.push(
                str_at(
                    prompts,
                    &["turn", "prompt_parts", "interesting_event_prompt"],
                )
                .to_string(),
            );
            let threads = summary.upcoming_events();
            if !threads.is_empty() {
                parts.push(
                    str_at(
                        prompts,
                        &[
                            "turn",
                            "prompt_parts",
                            "interesting_event_with_threads_header",
                        ],
                    )
                    .to_string(),
                );
                for thread in threads.iter() {
                    parts.push(format!("- {}", thread.as_str()));
                }
                parts.push(
                    str_at(
                        prompts,
                        &["turn", "prompt_parts", "interesting_event_fallback"],
                    )
                    .to_string(),
                );
            }
        } else if let Some(input) = input {
            parts.push(render(
                &env,
                str_at(prompts, &["turn", "prompt_parts", "player_directs"]),
                context! { player_input => input },
            ));
        } else {
            parts.push(
                str_at(prompts, &["turn", "prompt_parts", "player_no_direction"]).to_string(),
            );
        }
        parts.push(str_at(prompts, &["turn", "prompt_parts", "continue_seamlessly"]).to_string());
    } else {
        if let Some(input) = input {
            parts.push(render(
                &env,
                str_at(prompts, &["turn", "prompt_parts", "opening_with_input"]),
                context! { player_input => input },
            ));
        }
        // PROMPTS-001: the opening turn only, never repeated on later turns.
        parts.push(
            str_at(
                &default_prompt_additions(),
                &["turn", "prompt_parts", "opening_identity_review"],
            )
            .to_string(),
        );
        parts.push(str_at(prompts, &["turn", "prompt_parts", "opening_instruction"]).to_string());
    }
    parts.join("\n")
}
