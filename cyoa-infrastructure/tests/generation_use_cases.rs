use cyoa_application::{cancellation::CancellationSource, generation::*};
use cyoa_core::{game::GameState, limits::Limits, style::StoryStyle, text::*, world::*};
use cyoa_infrastructure::generation::scripted::ScriptedBackend;
use cyoa_infrastructure::{
    backend::*,
    generation::{engine::GenerationEngine, templates::GenerationTemplates},
};

fn use_cases(
    responses: impl IntoIterator<Item = Result<String, BackendError>>,
) -> StoryUseCases<GenerationEngine<ScriptedBackend>> {
    StoryUseCases::new(GenerationEngine::new(
        ScriptedBackend::new(responses),
        GenerationTemplates::bundled().unwrap(),
    ))
}
fn game() -> GameState {
    let players = ["Ajax", "Blair"].map(|name| {
        PlayerCharacter::new(
            CharacterName::new(name).unwrap(),
            CharacterDescription::new("A sailor").unwrap(),
            Backstory::new("A history").unwrap(),
        )
    });
    let limits = Limits::default();
    let world = World::new(
        WorldOutline::new(
            WorldTitle::new("Harbour").unwrap(),
            WorldDescription::new("A sheltered harbour").unwrap(),
        ),
        WorldCast::new(players, [], &limits).unwrap(),
    );
    GameState::start(
        Brief::new("An adventure").unwrap(),
        world.select(PlayablePosition::new(0)).unwrap(),
        StoryStyle::default(),
        limits,
    )
}
fn turn_json() -> serde_json::Value {
    serde_json::json!({"narrative":"The lantern flickered.","quick_actions":[{"text":"Go"}],"scene_description":"","summary_update":{"current_situation":"At sea"},"starts_new_chapter":false,"chapter_title":null})
}

#[test]
fn cancelled_commands_make_no_backend_calls_and_preserve_state() {
    let source = CancellationSource::default();
    source.cancel();
    let mut use_cases = use_cases([]);
    let mut game = game();
    let before = game.clone();
    let error = use_cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source.token(),
            &mut |_| panic!("no preview expected"),
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert_eq!(game, before);
    assert_eq!(
        use_cases
            .generate_outline(game.brief(), &source.token())
            .unwrap_err()
            .kind(),
        FailureKind::Cancelled
    );
    assert_eq!(
        use_cases
            .generate_world(
                game.brief(),
                game.world().outline().clone(),
                &game.limits(),
                &source.token()
            )
            .unwrap_err()
            .kind(),
        FailureKind::Cancelled
    );
    assert!(
        use_cases
            .into_generator()
            .into_backend()
            .requests()
            .is_empty()
    );
}

#[test]
fn failed_turns_preserve_diagnostics_state_and_single_attempt() {
    let mut missing = turn_json();
    missing.as_object_mut().unwrap().remove("scene_description");
    let mut blank = turn_json();
    blank["narrative"] = "   ".into();
    let mut no_actions = turn_json();
    no_actions["quick_actions"] = serde_json::json!([]);
    let source = CancellationSource::default();
    for (response, kind, raw) in [
        (
            Err(BackendError::Generation {
                message: "network failed".into(),
                raw_response: " \r\npartial 🎭 ".into(),
            }),
            FailureKind::Transport,
            " \r\npartial 🎭 ".into(),
        ),
        (
            Ok("invalid JSON \r\n".into()),
            FailureKind::Transport,
            "invalid JSON \r\n".into(),
        ),
        (
            Ok(missing.to_string()),
            FailureKind::InvalidResponse,
            missing.to_string(),
        ),
        (
            Ok(blank.to_string()),
            FailureKind::InvalidResponse,
            blank.to_string(),
        ),
        (
            Ok(no_actions.to_string()),
            FailureKind::InvalidResponse,
            no_actions.to_string(),
        ),
    ] {
        let mut use_cases = use_cases([response]);
        let mut game = game();
        let before = game.clone();
        let error = use_cases
            .take_turn(
                &mut game,
                TurnDirection::Continue,
                &source.token(),
                &mut |_| {},
            )
            .unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(error.raw_response().as_str(), raw);
        assert_eq!(game, before);
        assert_eq!(
            use_cases.into_generator().into_backend().requests().len(),
            1
        );
    }
}
#[test]
fn invalid_outline_and_insufficient_cast_are_returned_with_raw_output() {
    let source = CancellationSource::default();
    let raw = "{\"title\":\"\",\"world_description\":\"A world\"}";
    let mut use_cases = use_cases([Ok(raw.into()), Ok("{\"characters\":[],\"npcs\":[]}".into())]);
    let game = game();
    let error = use_cases
        .generate_outline(game.brief(), &source.token())
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::InvalidResponse);
    assert_eq!(error.raw_response().as_str(), raw);
    let error = use_cases
        .generate_world(
            game.brief(),
            game.world().outline().clone(),
            &game.limits(),
            &source.token(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::InvalidResponse);
    assert_eq!(
        error.raw_response().as_str(),
        "{\"characters\":[],\"npcs\":[]}"
    );
    assert_eq!(
        use_cases.into_generator().into_backend().requests().len(),
        2
    );
}

#[test]
fn restored_limits_reach_captured_prompt_schema_and_committed_memory() {
    use cyoa_core::{limits::MajorEventLimit, limits::RestoreLimits};
    use cyoa_infrastructure::generation::scripted::ScriptedBackend as Replay;
    let source = CancellationSource::default();
    for (policy, cap) in [
        (
            RestoreLimits::Current(Limits {
                max_major_events: MajorEventLimit::new(1).unwrap(),
                ..Limits::default()
            }),
            1,
        ),
        (RestoreLimits::Original, 30),
    ] {
        let original = game();
        let mut restored = GameState::restore(
            original.brief().clone(),
            original.selected_world().clone(),
            original.style().clone(),
            original.original_limits(),
            policy,
            vec![],
        );
        let mut response = turn_json();
        response["summary_update"]["new_major_events"] = serde_json::json!(["A", "B", "C"]);
        let backend = Replay::new([Ok(response.to_string())]);
        let mut use_cases = StoryUseCases::new(GenerationEngine::new(
            backend,
            GenerationTemplates::bundled().unwrap(),
        ));
        use_cases
            .take_turn(
                &mut restored,
                TurnDirection::Continue,
                &source.token(),
                &mut |_| {},
            )
            .unwrap();
        let backend = use_cases.into_generator().into_backend();
        assert_eq!(backend.requests().len(), 1);
        let request = &backend.requests()[0];
        assert!(
            request
                .instructions()
                .contains(&format!("at most {cap} major events"))
        );
        assert!(request.schema()["$defs"]["SummaryUpdate"]["properties"]["consolidated_major_events"]["description"].as_str().unwrap().contains(&format!("{cap} major events")));
        assert_eq!(
            restored
                .current_summary()
                .major_events()
                .events()
                .as_slice()
                .len(),
            cap.min(3)
        );
        assert_eq!(restored.original_limits(), original.original_limits());
    }
}

#[test]
fn scripted_lifecycle_uses_edited_outline_and_preserves_namesake_ids_without_extra_calls() {
    use cyoa_core::ids::CharacterId;
    use cyoa_infrastructure::generation::scripted::ScriptedBackend as Replay;
    let source = CancellationSource::default();
    let brief = Brief::new("The two Ajaxes").unwrap();
    let outline = serde_json::json!({"title":"Harbour","world_description":"Original harbour"});
    let cast = serde_json::json!({"characters":[
        {"name":"Ajax","description":"A sailor","backstory":"Grew up at sea"},
        {"name":"Ajax","description":"A mason","backstory":"Built the wall"}],"npcs":[
        {"name":"Ajax","description":"A merchant","backstory":"Sells sails","relationships":"Knows the sailor"},
        {"name":"Ajax","description":"A merchant","backstory":"Sells sails","relationships":"Knows the sailor"},
        {"name":"Ajax","description":"A guard","backstory":"Guards the wall","relationships":"Knows the mason"}]});
    let mut opening = turn_json();
    opening["summary_update"]["character_updates"] =
        serde_json::json!([{"id":"ajax-2","current_state":"At the gate"}]);
    let backend =
        Replay::new([outline, cast, opening, turn_json()].map(|value| Ok(value.to_string())));
    let mut use_cases = StoryUseCases::new(GenerationEngine::new(
        backend,
        GenerationTemplates::bundled().unwrap(),
    ));
    let generated = use_cases.generate_outline(&brief, &source.token()).unwrap();
    let accepted = WorldOutline::new(
        generated.value().title().clone(),
        WorldDescription::new("Edited harbour").unwrap(),
    );
    let world = use_cases
        .generate_world(&brief, accepted, &Limits::default(), &source.token())
        .unwrap();
    assert_eq!(world.cast().playable().len(), 2);
    assert_eq!(world.cast().npcs().len(), 2);
    let mut game = GameState::start(
        brief,
        world.select(PlayablePosition::new(1)).unwrap(),
        StoryStyle::default(),
        Limits::default(),
    );
    assert_eq!(game.protagonist().description().as_str(), "A mason");
    use_cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source.token(),
            &mut |_| {},
        )
        .unwrap();
    use_cases
        .take_turn(
            &mut game,
            TurnDirection::Player(PlayerInput::new("Go to the gate").unwrap()),
            &source.token(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(game.turns().len(), 2);
    assert_eq!(
        game.current_summary()
            .characters()
            .get(&CharacterId::new("ajax-2").unwrap())
            .unwrap()
            .details()
            .current_state()
            .unwrap()
            .as_str(),
        "At the gate"
    );
    let backend = use_cases.into_generator().into_backend();
    let requests = backend.requests();
    assert_eq!(requests.len(), 4);
    assert!(requests[1].prompt().contains("Edited harbour"));
    assert!(!requests[1].prompt().contains("Original harbour"));
    assert_eq!(
        requests[2]
            .prompt()
            .matches("Before writing the opening scene")
            .count(),
        1
    );
    assert!(
        !requests[3]
            .prompt()
            .contains("Before writing the opening scene")
    );
    for id in ["protagonist", "ajax", "ajax-2"] {
        assert!(requests[2].prompt().contains(&format!("\"id\": \"{id}\"")));
        assert!(requests[3].prompt().contains(&format!("\"id\": \"{id}\"")));
    }
    assert!(requests[3].prompt().contains("At the gate"));
    assert_eq!(game.turns()[1].input().unwrap().as_str(), "Go to the gate");
    assert!(game.turns()[1].prompt_trace().is_none());
}

#[test]
fn preview_then_transport_failure_leaves_state_untouched() {
    struct BrokenStream;
    impl Backend for BrokenStream {
        fn generate(
            &mut self,
            _: GenerationRequest<'_>,
            _: &cyoa_application::cancellation::CancellationToken,
            on_json: &mut dyn FnMut(&str),
        ) -> Result<GenerationResponse, BackendError> {
            let partial = "{\"narrative\":\"Visible preview";
            on_json(partial);
            Err(BackendError::Generation {
                message: "stream failed".into(),
                raw_response: partial.into(),
            })
        }
    }
    let mut use_cases = StoryUseCases::new(GenerationEngine::new(
        BrokenStream,
        GenerationTemplates::bundled().unwrap(),
    ));
    let source = CancellationSource::default();
    let mut game = game();
    let before = game.clone();
    let mut preview = String::new();
    let error = use_cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source.token(),
            &mut |text| preview.push_str(text),
        )
        .unwrap_err();
    assert_eq!(preview, "Visible preview");
    assert_eq!(
        error.raw_response().as_str(),
        "{\"narrative\":\"Visible preview"
    );
    assert_eq!(game, before);
}

#[test]
fn cancelling_after_a_streamed_preview_preserves_state_and_partial_diagnostics() {
    use cyoa_infrastructure::generation::scripted::ChunkedBackend;
    let raw = turn_json().to_string();
    let backend = ChunkedBackend::new([Ok(raw.clone())], 42);
    let mut use_cases = StoryUseCases::new(GenerationEngine::new(
        backend,
        GenerationTemplates::bundled().unwrap(),
    ));
    let source = CancellationSource::default();
    let mut game = game();
    let before = game.clone();
    let mut preview = String::new();
    let error = use_cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source.token(),
            &mut |text| {
                preview.push_str(text);
                source.cancel()
            },
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert!(!preview.is_empty());
    assert!("The lantern flickered.".starts_with(&preview));
    assert!(raw.starts_with(error.raw_response().as_str()));
    assert!(error.raw_response().as_str().len() < raw.len());
    assert_eq!(game, before);
    assert_eq!(
        use_cases.into_generator().into_backend().requests().len(),
        1
    );
}

#[test]
fn chunked_and_complete_transports_commit_the_same_turn_without_duplicate_preview() {
    use cyoa_infrastructure::generation::scripted::ChunkedBackend;
    let narrative = "A \"quoted\" scene: 🎭\n界.";
    let mut value = turn_json();
    value["narrative"] = narrative.into();
    let raw = value.to_string();
    let source = CancellationSource::default();
    let mut whole = use_cases([Ok(raw.clone())]);
    let mut expected = game();
    let mut preview = String::new();
    whole
        .take_turn(
            &mut expected,
            TurnDirection::Continue,
            &source.token(),
            &mut |text| preview.push_str(text),
        )
        .unwrap();
    assert_eq!(preview, narrative);
    for seed in 0..32 {
        let backend = ChunkedBackend::new([Ok(raw.clone())], seed);
        let mut cases = StoryUseCases::new(GenerationEngine::new(
            backend,
            GenerationTemplates::bundled().unwrap(),
        ));
        let mut actual = game();
        let mut preview = String::new();
        let mut chunks = 0;
        cases
            .take_turn(
                &mut actual,
                TurnDirection::Continue,
                &source.token(),
                &mut |text| {
                    preview.push_str(text);
                    chunks += 1
                },
            )
            .unwrap();
        assert!(chunks > 1, "seed {seed} must exercise incremental output");
        assert_eq!(preview, narrative);
        assert_eq!(actual, expected);
        assert_eq!(actual.turns()[0].raw_response().as_str(), raw);
        assert_eq!(cases.into_generator().into_backend().requests().len(), 1);
    }
}
