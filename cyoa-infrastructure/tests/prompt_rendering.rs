//! Spec-first tests for `generation::prompts`, each tied to a rule stated in
//! PLAN.md or transcribed directly from `reference/calibre/cyoa.py`.

use cyoa_core::{
    game::GameState,
    limits::Limits,
    style::StoryStyle,
    summary::{EventList, SummaryUpdate, UpcomingEventsUpdate},
    text::{
        Backstory, Brief, ChapterTitle, CharacterDescription, CharacterName, Narrative,
        QuickActionText, RawResponse, ToneKey, WorldDescription, WorldTitle,
    },
    turn::{
        ChapterMarker, GenerationProvenance, QuickAction, QuickActionKind, QuickActions, StoryTurn,
        TurnGenerationRecord,
    },
    world::{
        NonPlayerCharacter, PlayablePosition, PlayerCharacter, World, WorldCast, WorldOutline,
    },
};
use cyoa_infrastructure::generation::prompts::{
    cast_generation_prompt, default_prompts, environment, merge_per_key, prose_contract,
    quick_action_instructions, startup_self_check, turn_instructions, turn_prompt,
    world_generation_prompt,
};

fn player(name: &str) -> PlayerCharacter {
    PlayerCharacter::new(
        CharacterName::new(name).unwrap(),
        CharacterDescription::new("description").unwrap(),
        Backstory::new("backstory").unwrap(),
    )
}

fn npc(name: &str) -> NonPlayerCharacter {
    NonPlayerCharacter::new(
        CharacterName::new(name).unwrap(),
        CharacterDescription::new("description").unwrap(),
        Backstory::new("backstory").unwrap(),
        None,
    )
}

fn world() -> World {
    let outline = WorldOutline::new(
        WorldTitle::new("The Mist City").unwrap(),
        WorldDescription::new("A city lost in fog.").unwrap(),
    );
    let limits = Limits::default();
    let cast = WorldCast::new([player("Alex"), player("Blair")], [npc("Casey")], &limits).unwrap();
    World::new(outline, cast)
}

fn game() -> GameState {
    let selected = world().select(PlayablePosition::new(0)).unwrap();
    GameState::start(
        Brief::new("brief").unwrap(),
        selected,
        StoryStyle::default(),
        Limits::default(),
    )
}

fn turn(chapter: ChapterMarker, narrative: &str) -> StoryTurn {
    turn_with_update(chapter, narrative, SummaryUpdate::default())
}

fn turn_with_update(chapter: ChapterMarker, narrative: &str, update: SummaryUpdate) -> StoryTurn {
    StoryTurn::new(
        Narrative::new(narrative).unwrap(),
        QuickActions::select([QuickAction::new(
            QuickActionText::new("Go").unwrap(),
            QuickActionKind::Bold,
        )])
        .unwrap(),
        None,
        update,
        chapter,
    )
}

fn commit(state: &mut GameState, chapter: ChapterMarker, input: Option<&str>, narrative: &str) {
    state.commit_turn(
        turn(chapter, narrative),
        TurnGenerationRecord {
            input: input.map(|i| cyoa_core::text::PlayerInput::new(i).unwrap()),
            raw_response: RawResponse::new("{}"),
            provenance: GenerationProvenance::default(),
            prompt_trace: None,
        },
    );
}

fn parsed_override(toml_text: &str) -> toml::Value {
    toml::Value::Table(toml_text.parse::<toml::Table>().unwrap())
}

#[test]
fn toml_override_merges_per_key_not_per_file() {
    let base = toml::Value::Table(default_prompts());
    let override_ = parsed_override("[world]\ninstructions = \"Custom instructions.\"");
    let merged = merge_per_key(base, &override_);
    let merged = merged.as_table().unwrap();
    let default = default_prompts();

    assert_eq!(
        merged["world"]["instructions"].as_str(),
        Some("Custom instructions.")
    );
    // The sibling key is untouched...
    assert_eq!(merged["world"]["prompt"], default["world"]["prompt"]);
    // ...and so is an entirely different table.
    assert_eq!(merged["cast"], default["cast"]);
}

#[test]
fn undefined_template_variable_is_a_render_error() {
    assert!(startup_self_check(&default_prompts()).is_ok());

    let base = toml::Value::Table(default_prompts());
    let broken = parsed_override("[world]\nprompt = \"Hello {{ nonexistent_var }}\"");
    let merged = merge_per_key(base, &broken);
    assert!(startup_self_check(merged.as_table().unwrap()).is_err());
}

fn scan_for_banned_phrases(value: &toml::Value, banned: &[&str]) -> Vec<String> {
    match value {
        toml::Value::String(s) => {
            let lower = s.to_lowercase();
            if banned.iter().any(|phrase| lower.contains(phrase)) {
                vec![s.clone()]
            } else {
                Vec::new()
            }
        }
        toml::Value::Table(t) => t
            .values()
            .flat_map(|v| scan_for_banned_phrases(v, banned))
            .collect(),
        toml::Value::Array(a) => a
            .iter()
            .flat_map(|v| scan_for_banned_phrases(v, banned))
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn loaded_instructions_contain_no_json_formatting_directive() {
    let banned = [
        "only valid json",
        "only json",
        "respond with json",
        "reply as json",
        "output only json",
    ];
    let hits = scan_for_banned_phrases(&toml::Value::Table(default_prompts()), &banned);
    assert!(hits.is_empty(), "found banned phrasing: {hits:?}");
}

#[test]
fn loaded_turn_instructions_ask_for_schema_field_order() {
    let instructions = turn_instructions(
        &StoryStyle::default(),
        world().outline(),
        &player("Ada"),
        &Limits::default(),
    );
    assert!(instructions.contains("in the order they appear in the schema"));
}

#[test]
fn prose_contract_omits_tone_clause_when_tone_is_default() {
    let env = environment();
    let prompts = default_prompts();
    let default_text = prose_contract(&env, &prompts, &StoryStyle::default());
    assert!(!default_text.to_lowercase().contains("register"));

    let grimdark = StoryStyle {
        tone: Some(ToneKey::new("grimdark").unwrap()),
        ..StoryStyle::default()
    };
    let grimdark_text = prose_contract(&env, &prompts, &grimdark);
    assert!(grimdark_text.to_lowercase().contains("grimdark register"));
}

#[test]
fn quick_action_instructions_change_when_kinds_table_is_overridden() {
    let default_text = quick_action_instructions(&default_prompts());
    assert!(default_text.contains("hold back, defend"));

    let base = toml::Value::Table(default_prompts());
    let override_ = parsed_override(
        r#"
[quick_actions]
requested = [["cautious"]]
[[quick_actions.kinds]]
key = "cautious"
label = "Cautious"
meaning = "TEST MEANING"
"#,
    );
    let merged = merge_per_key(base, &override_);
    let overridden_text = quick_action_instructions(merged.as_table().unwrap());
    assert!(overridden_text.contains("TEST MEANING"));
    assert!(!overridden_text.contains("hold back, defend"));
}

#[test]
fn opening_turn_prompt_includes_the_identity_review_addition_exactly_once() {
    let state = game();
    let opening = turn_prompt(&state, None, false);
    assert!(opening.contains("Before writing the opening scene"));

    let mut state = state;
    commit(
        &mut state,
        ChapterMarker::Continue { title: None },
        None,
        "The mist parted.",
    );
    let later = turn_prompt(&state, Some("Push forward"), false);
    assert!(!later.contains("Before writing the opening scene"));
}

#[test]
fn rendered_limits_reflect_the_supplied_value_not_a_constant() {
    let small = Limits {
        max_major_events: cyoa_core::limits::MajorEventLimit::new(5).unwrap(),
        ..Limits::default()
    };
    let big = Limits {
        max_major_events: cyoa_core::limits::MajorEventLimit::new(99).unwrap(),
        ..Limits::default()
    };
    let world = world();
    let small_text = turn_instructions(
        &StoryStyle::default(),
        world.outline(),
        &player("Ada"),
        &small,
    );
    let big_text = turn_instructions(
        &StoryStyle::default(),
        world.outline(),
        &player("Ada"),
        &big,
    );
    assert!(small_text.contains("at most 5 major events"));
    assert!(big_text.contains("at most 99 major events"));
    assert_ne!(small_text, big_text);

    let few_npcs = Limits {
        max_generated_npcs: cyoa_core::limits::MaxGeneratedNpcs::new(3),
        ..Limits::default()
    };
    let (few_instructions, _) =
        cast_generation_prompt(&Brief::new("brief").unwrap(), &world, &few_npcs);
    assert!(few_instructions.contains("between three and 3 other characters"));
}

#[test]
fn world_and_cast_prompts_interpolate_the_brief_and_world() {
    let (_, world_prompt) = world_generation_prompt(&Brief::new("a rain-soaked city").unwrap());
    assert!(world_prompt.contains("a rain-soaked city"));

    let world = world();
    let (_, cast_prompt) = cast_generation_prompt(
        &Brief::new("a rain-soaked city").unwrap(),
        &world,
        &Limits::default(),
    );
    assert!(cast_prompt.contains("The Mist City"));
    assert!(cast_prompt.contains("A city lost in fog."));
}

#[test]
fn turn_prompt_bridge_and_transcript_appear_only_when_nonempty() {
    let mut state = game();
    let opening = turn_prompt(&state, None, false);
    assert!(!opening.contains("The prose of the current chapter so far"));
    assert!(!opening.contains("The closing prose of the previous chapter"));

    commit(
        &mut state,
        ChapterMarker::Continue { title: None },
        None,
        "First passage.",
    );
    let with_transcript = turn_prompt(&state, Some("Look around"), false);
    assert!(with_transcript.contains("The prose of the current chapter so far"));
    assert!(with_transcript.contains("First passage."));
    assert!(!with_transcript.contains("The closing prose of the previous chapter"));

    commit(
        &mut state,
        ChapterMarker::NewChapter {
            title: Some(ChapterTitle::new("Chapter Two").unwrap()),
        },
        Some("Look around"),
        "Second passage.",
    );
    let with_bridge = turn_prompt(&state, Some("Push on"), false);
    assert!(with_bridge.contains("The closing prose of the previous chapter"));
    assert!(with_bridge.contains("First passage."));
}

#[test]
fn interesting_event_includes_threads_only_when_present() {
    let mut state = game();
    commit(
        &mut state,
        ChapterMarker::Continue { title: None },
        None,
        "First passage.",
    );
    let no_threads = turn_prompt(&state, None, true);
    assert!(no_threads.contains("Have something unexpected"));
    assert!(!no_threads.contains("unresolved threads"));

    state.commit_turn(
        turn_with_update(
            ChapterMarker::Continue { title: None },
            "Second passage.",
            SummaryUpdate {
                upcoming_events: UpcomingEventsUpdate::Replace(EventList::new(["Marlo returns"])),
                ..SummaryUpdate::default()
            },
        ),
        TurnGenerationRecord {
            input: None,
            raw_response: RawResponse::new("{}"),
            provenance: GenerationProvenance::default(),
            prompt_trace: None,
        },
    );
    let with_thread = turn_prompt(&state, None, true);
    assert!(with_thread.contains("unresolved threads"));
    assert!(with_thread.contains("- Marlo returns"));
    assert!(with_thread.contains("Invent something unrelated only if"));
}

#[test]
fn player_input_versus_no_direction() {
    let mut state = game();
    commit(
        &mut state,
        ChapterMarker::Continue { title: None },
        None,
        "First passage.",
    );
    let directed = turn_prompt(&state, Some("Open the door"), false);
    assert!(directed.contains("The reader directs: Open the door"));

    let undirected = turn_prompt(&state, None, false);
    assert!(undirected.contains("The reader offers no direction."));
}
