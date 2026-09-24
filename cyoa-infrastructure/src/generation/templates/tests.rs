use super::*;

fn changed(source: &str, path: &[&str], value: Option<toml::Value>) -> String {
    let mut root: toml::Value = toml::from_str(source).unwrap();
    let mut target = &mut root;
    for part in &path[..path.len() - 1] {
        target = target.get_mut(*part).unwrap();
    }
    let table = target.as_table_mut().unwrap();
    match value {
        Some(value) => {
            table.insert(path[path.len() - 1].into(), value);
        }
        None => {
            table.remove(path[path.len() - 1]);
        }
    }
    toml::to_string(&root).unwrap()
}

fn rejected(prompts: &str, styles: &str, docs: &str, file: &str, path: &str) {
    let error = GenerationTemplates::from_sources(prompts, styles, docs)
        .unwrap_err()
        .to_string();
    assert!(error.contains(file), "{error}");
    assert!(error.contains(path), "{error}");
}

#[test]
fn malformed_sources_report_the_source_file() {
    for (p, s, d, file) in [
        ("[", STYLES, DOCS, "prompts.toml"),
        (PROMPTS, "[", DOCS, "styles.toml"),
        (PROMPTS, STYLES, "[", "schema_docs.toml"),
    ] {
        rejected(p, s, d, file, "");
    }
}

#[test]
fn prompt_shape_rejects_missing_wrong_type_and_unknown_keys() {
    for value in [None, Some(toml::Value::Integer(42))] {
        let source = changed(PROMPTS, &["prose_contract", "dialogue_and_sensory"], value);
        rejected(
            &source,
            STYLES,
            DOCS,
            "prompts.toml",
            "prose_contract.dialogue_and_sensory",
        );
    }
    let source = changed(PROMPTS, &["world", "typo"], Some("ignored".into()));
    rejected(&source, STYLES, DOCS, "prompts.toml", "world.typo");
}

#[test]
fn templates_reject_syntax_wrong_context_and_nested_typos() {
    for (path, text) in [
        (vec!["world", "prompt"], "{{"),
        (vec!["fragments", "markdown"], "{{ brief }}"),
        (vec!["cast", "prompt"], "{{ world.titel }}"),
        (
            vec!["cast", "prompt"],
            "{% if false %}{{ world.titel }}{% endif %}",
        ),
    ] {
        let source = changed(PROMPTS, &path, Some(text.into()));
        rejected(&source, STYLES, DOCS, "prompts.toml", &path.join("."));
    }
}

#[test]
fn styles_reject_empty_tables_duplicate_keys_and_wrong_shapes() {
    for value in [
        toml::Value::Array(vec![]),
        toml::Value::Integer(42),
        toml::from_str::<toml::Table>(
            "entries = [{key='same',name='One',prompt='A'}, {key='same',name='Two',prompt='B'}]",
        )
        .unwrap()["entries"]
            .clone(),
    ] {
        let source = changed(STYLES, &["pace"], Some(value));
        rejected(PROMPTS, &source, DOCS, "styles.toml", "pace");
    }
}

#[test]
fn quick_actions_reject_dangling_references_and_empty_groups() {
    for requested in [vec![vec!["nonexistent"]], vec![vec![]], vec![]] {
        let value = toml::Value::try_from(requested).unwrap();
        let source = changed(PROMPTS, &["quick_actions", "requested"], Some(value));
        rejected(
            &source,
            STYLES,
            DOCS,
            "prompts.toml",
            "quick_actions.requested",
        );
    }
}

#[test]
fn schema_documentation_rejects_unknown_missing_and_invalid_entries() {
    for (path, value) in [
        (vec!["Orphan"], Some(toml::Value::Table(toml::Table::new()))),
        (vec!["StoryTurn", "narrative"], None),
        (vec!["StoryTurn", "stale"], Some("text".into())),
        (vec!["StoryTurn", "narrative"], Some(42.into())),
        (vec!["GeneratedCast", "npcs"], Some("{{ typo }}".into())),
    ] {
        let source = changed(DOCS, &path, value);
        rejected(
            PROMPTS,
            STYLES,
            &source,
            "schema_docs.toml",
            &path.join("."),
        );
    }
}

#[test]
fn data_dependent_render_errors_are_values_with_template_locations() {
    let source = changed(
        PROMPTS,
        &["world", "prompt"],
        Some(
            "{% if brief == 'break' %}{{ brief[1000] }}{% else %}World: {{ brief }}{% endif %}"
                .into(),
        ),
    );
    let templates = GenerationTemplates::from_sources(&source, STYLES, DOCS).unwrap();
    let error = templates
        .world_request(&Brief::new("break").unwrap())
        .unwrap_err();
    assert!(error.to_string().contains("prompts.toml:world.prompt"));
}

fn game() -> cyoa_core::game::GameState {
    use cyoa_core::{game::GameState, style::StoryStyle, text::*, world::*};
    let players = ["Ajax", "Blair"].map(|name| {
        PlayerCharacter::new(
            CharacterName::new(name).unwrap(),
            CharacterDescription::new("A sailor").unwrap(),
            Backstory::new("Grew up at sea").unwrap(),
        )
    });
    let limits = Limits::default();
    let cast = WorldCast::new(players, [], &limits).unwrap();
    let world = World::new(
        WorldOutline::new(
            WorldTitle::new("Harbour").unwrap(),
            WorldDescription::new("A sheltered harbour").unwrap(),
        ),
        cast,
    );
    GameState::start(
        Brief::new("A harbour adventure").unwrap(),
        world.select(PlayablePosition::new(0)).unwrap(),
        StoryStyle::default(),
        limits,
    )
}

#[test]
fn empty_schema_descriptions_remove_defaults_and_nested_overrides_apply() {
    let docs = changed(DOCS, &["WorldOutline", "title"], Some("".into()));
    let docs = changed(
        &docs,
        &["QuickAction", "text"],
        Some("A different action description".into()),
    );
    let templates = GenerationTemplates::from_sources(PROMPTS, STYLES, &docs).unwrap();
    let world = templates
        .world_request(&Brief::new("brief").unwrap())
        .unwrap();
    assert!(
        world.schema()["properties"]["title"]
            .get("description")
            .is_none()
    );
    let turn = templates.turn_request(&game(), None, false).unwrap();
    assert_eq!(
        turn.schema()["$defs"]["QuickAction"]["properties"]["text"]["description"],
        "A different action description"
    );
}

#[test]
fn unknown_styles_use_configured_first_entry_and_blank_tone_is_omitted() {
    use cyoa_core::{
        style::StoryStyle,
        text::{PaceKey, ToneKey},
    };
    let mut styles: toml::Value = toml::from_str(STYLES).unwrap();
    styles["pace"][0]["prompt"] = "CUSTOM PACE".into();
    styles["tone"][0]["prompt"] = "".into();
    let templates =
        GenerationTemplates::from_sources(PROMPTS, &toml::to_string(&styles).unwrap(), DOCS)
            .unwrap();
    let mut game = game();
    game.set_style(StoryStyle {
        pace: Some(PaceKey::new("unknown").unwrap()),
        tone: Some(ToneKey::new("unknown").unwrap()),
        ..Default::default()
    });
    let request = templates.turn_request(&game, None, false).unwrap();
    assert!(
        request
            .instructions()
            .as_str()
            .contains("CUSTOM PACE Write it")
    );
    assert!(!request.instructions().as_str().contains("register"));
}

#[test]
fn configured_turn_fragments_render_once_and_player_text_is_not_a_template() {
    let prompts = changed(
        PROMPTS,
        &["turn", "prompt_parts", "opening_with_input"],
        Some("CUSTOM {{ player_input }}".into()),
    );
    let templates = GenerationTemplates::from_sources(&prompts, STYLES, DOCS).unwrap();
    let request = templates
        .turn_request(&game(), Some("{{ missing }}\r\n🎭"), false)
        .unwrap();
    assert!(
        request
            .prompt()
            .as_str()
            .contains("CUSTOM {{ missing }}\r\n🎭")
    );
    assert_eq!(
        request
            .prompt()
            .as_str()
            .matches("Before writing the opening scene")
            .count(),
        1
    );
}

#[test]
fn startup_checks_the_zero_npc_branch_in_prompts_and_schema_docs() {
    let broken = "{% if max_generated_npcs == 0 %}{{ max_generated_npcs[1] }}{% endif %}";
    let prompts = changed(PROMPTS, &["cast", "instructions"], Some(broken.into()));
    rejected(&prompts, STYLES, DOCS, "prompts.toml", "cast.instructions");
    let docs = changed(DOCS, &["GeneratedCast", "npcs"], Some(broken.into()));
    rejected(
        PROMPTS,
        STYLES,
        &docs,
        "schema_docs.toml",
        "GeneratedCast.npcs",
    );
}

#[test]
fn merged_configuration_drives_all_three_generation_requests() {
    use super::super::prompts::merge_per_key;
    let override_:toml::Value=toml::from_str(r#"
[world]
instructions="CUSTOM WORLD"
[cast]
instructions="CUSTOM CAST {{ min_generated_playables }}–{{ max_generated_playables }} / {{ max_generated_npcs }}"
[turn]
role="CUSTOM TURN {{ protagonist.name }}"
"#).unwrap();
    let prompts = merge_per_key(toml::from_str(PROMPTS).unwrap(), &override_);
    let templates =
        GenerationTemplates::from_sources(&toml::to_string(&prompts).unwrap(), STYLES, DOCS)
            .unwrap();
    let game = game();
    let world = templates.world_request(game.brief()).unwrap();
    assert_eq!(world.instructions().as_str(), "CUSTOM WORLD");
    assert_eq!(
        world.prompt().as_str(),
        "Create the world for an adventure game based on this description:\n\nA harbour adventure"
    );
    let cast = templates
        .cast_request(game.brief(), game.world().outline(), &game.limits())
        .unwrap();
    assert_eq!(cast.instructions().as_str(), "CUSTOM CAST 3–5 / 8");
    assert!(cast.prompt().as_str().contains("A sheltered harbour"));
    let turn = templates.turn_request(&game, None, false).unwrap();
    assert!(
        turn.instructions()
            .as_str()
            .starts_with("CUSTOM TURN Ajax\n")
    );
    assert!(
        turn.instructions()
            .as_str()
            .contains("at most 30 major events")
    );
    assert!(turn.prompt().as_str().contains("\"id\": \"protagonist\""));
    assert_eq!(
        turn.schema()["properties"]
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap(),
        "narrative"
    );
}

#[test]
fn configured_opening_continuation_and_bridge_match_complete_snapshots() {
    use cyoa_core::{text::*, turn::*};
    let mut prompts: toml::Value = toml::from_str(PROMPTS).unwrap();
    prompts["quick_actions"]["kinds"][3]["meaning"] = "Seek {{ 'evidence' }}".into();
    let templates =
        GenerationTemplates::from_sources(&toml::to_string(&prompts).unwrap(), STYLES, DOCS)
            .unwrap();
    let mut game = game();
    let mut actual = Vec::new();
    for stage in 0..3 {
        let request = templates
            .turn_request(&game, Some("Look around"), false)
            .unwrap();
        actual.push(serde_json::json!({"instructions":request.instructions().as_str(),"prompt":request.prompt().as_str()}));
        let chapter = if stage == 0 {
            ChapterMarker::Continue { title: None }
        } else {
            ChapterMarker::NewChapter {
                title: Some(ChapterTitle::new("Departure").unwrap()),
            }
        };
        let turn = StoryTurn::new(
            Narrative::new(if stage == 0 {
                "The lantern flickered."
            } else {
                "The ship departed."
            })
            .unwrap(),
            QuickActions::select([QuickAction::new(
                QuickActionText::new("Go").unwrap(),
                QuickActionKind::Bold,
            )])
            .unwrap(),
            None,
            Default::default(),
            chapter,
        );
        game.commit_turn(
            turn,
            TurnGenerationRecord {
                input: None,
                raw_response: RawResponse::new("{}"),
                provenance: Default::default(),
                prompt_trace: None,
            },
        );
    }
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/configured_requests.json"
    ))
    .unwrap();
    assert_eq!(serde_json::json!(actual), expected);
}

// These three existing contract tests moved from the integration target because
// arbitrary source/override construction is deliberately private until PROMPTS-003.
#[test]
fn toml_override_merges_per_key_not_per_file() {
    let base: toml::Value = toml::from_str(PROMPTS).unwrap();
    let override_: toml::Value =
        toml::from_str("[world]\ninstructions='Custom instructions.'").unwrap();
    let merged = super::super::prompts::merge_per_key(base.clone(), &override_);
    assert_eq!(
        merged["world"]["instructions"].as_str(),
        Some("Custom instructions.")
    );
    assert_eq!(merged["world"]["prompt"], base["world"]["prompt"]);
    assert_eq!(merged["cast"], base["cast"]);
}
#[test]
fn undefined_template_variable_is_a_render_error() {
    assert!(GenerationTemplates::bundled().is_ok());
    let prompts = changed(
        PROMPTS,
        &["world", "prompt"],
        Some("Hello {{ nonexistent_var }}".into()),
    );
    rejected(&prompts, STYLES, DOCS, "prompts.toml", "world.prompt");
}
#[test]
fn quick_action_instructions_change_when_kinds_table_is_overridden() {
    let templates = GenerationTemplates::bundled().unwrap();
    assert!(
        templates
            .quick_action_instructions()
            .unwrap()
            .contains("hold back, defend")
    );
    let base: toml::Value = toml::from_str(PROMPTS).unwrap();
    let override_:toml::Value=toml::from_str("[quick_actions]\nrequested=[['cautious']]\n[[quick_actions.kinds]]\nkey='cautious'\nlabel='Cautious'\nmeaning='TEST MEANING'").unwrap();
    let merged = super::super::prompts::merge_per_key(base, &override_);
    let templates =
        GenerationTemplates::from_sources(&toml::to_string(&merged).unwrap(), STYLES, DOCS)
            .unwrap();
    let text = templates.quick_action_instructions().unwrap();
    assert!(text.contains("TEST MEANING"));
    assert!(!text.contains("hold back, defend"));
}
