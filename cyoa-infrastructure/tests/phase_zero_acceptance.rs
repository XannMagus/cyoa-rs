use cyoa_application::{cancellation::CancellationSource, generation::*};
use cyoa_core::{
    game::{GameState, TurnCount},
    ids::CharacterId,
    limits::{Limits, MajorEventLimit},
    style::StoryStyle,
    text::Brief,
    turn::QuickActionKind,
    world::PlayablePosition,
};
use cyoa_infrastructure::generation::{
    engine::GenerationEngine, scripted::ChunkedBackend, templates::GenerationTemplates,
};
use serde_json::Value;

type UseCases = StoryUseCases<GenerationEngine<ChunkedBackend>>;
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/phase0_story.json")).unwrap()
}
fn start(turns: Vec<String>) -> (UseCases, GameState) {
    let fixture = fixture();
    let responses = [fixture["outline"].to_string(), fixture["cast"].to_string()]
        .into_iter()
        .chain(turns)
        .map(Ok);
    let mut cases = StoryUseCases::new(GenerationEngine::new(
        ChunkedBackend::new(responses, 42),
        GenerationTemplates::bundled().unwrap(),
    ));
    let source = CancellationSource::default();
    let brief = Brief::new("Two Ajaxes, a harbour and a voyage").unwrap();
    let outline = cases
        .generate_outline(&brief, &source.token())
        .unwrap()
        .into_parts()
        .0;
    let limits = Limits {
        max_major_events: MajorEventLimit::new(2).unwrap(),
        ..Limits::default()
    };
    let world = cases
        .generate_world(&brief, outline, &limits, &source.token())
        .unwrap();
    assert_eq!(world.cast().playable().len(), 2);
    assert_eq!(world.cast().npcs().len(), 2);
    let game = GameState::start(
        brief,
        world.select(PlayablePosition::new(1)).unwrap(),
        StoryStyle::default(),
        limits,
    );
    (cases, game)
}
fn advance(cases: &mut UseCases, game: &mut GameState) {
    cases
        .take_turn(
            game,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |_| {},
        )
        .unwrap();
}

#[test]
fn invalid_rewind_is_an_error_without_state_changes_or_generation() {
    let (mut cases, mut game) = start(vec![]);
    let before = game.clone();
    assert!(cases.rewind(&mut game, TurnCount::new(1).unwrap()).is_err());
    assert_eq!(game, before);
    assert_eq!(cases.into_generator().into_backend().requests().len(), 2);
}

#[test]
fn rewinding_the_only_turn_restores_opening_context_without_an_extra_call() {
    let opening = fixture()["turns"][0].to_string();
    let (mut cases, mut game) = start(vec![opening.clone(), opening]);
    let initial = game.clone();
    advance(&mut cases, &mut game);
    cases.rewind(&mut game, TurnCount::new(1).unwrap()).unwrap();
    assert_eq!(game, initial);
    advance(&mut cases, &mut game);
    assert_eq!(game.turns().len(), 1);
    let backend = cases.into_generator().into_backend();
    assert_eq!(backend.requests().len(), 4);
    assert_eq!(
        backend.requests()[2].prompt(),
        backend.requests()[3].prompt()
    );
    assert_eq!(
        backend.requests()[3]
            .prompt()
            .matches("Before writing the opening scene")
            .count(),
        1
    );
}

#[test]
fn full_story_preserves_contracts_through_failure_retry_cancellation_and_rewind() {
    let data = fixture();
    let turns: Vec<String> = data["turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(Value::to_string)
        .collect();
    let malformed = " \r\n{\"narrative\":\"Broken preview 🎭";
    let (mut cases, mut game) = start(vec![
        turns[0].clone(),
        turns[1].clone(),
        malformed.into(),
        turns[2].clone(),
        turns[3].clone(),
        turns[3].clone(),
        turns[4].clone(),
    ]);
    advance(&mut cases, &mut game);
    assert_eq!(game.current_chapter().unwrap().number().get(), 0);
    assert_eq!(
        game.turns()[0].turn().quick_actions().as_slice()[0].kind(),
        QuickActionKind::Other
    );
    advance(&mut cases, &mut game);
    assert_eq!(game.turns().len(), 2);
    assert_eq!(
        game.current_chapter().unwrap().title().unwrap().as_str(),
        "Retitled"
    );
    assert_eq!(events(&game), ["B", "C"]);
    assert_eq!(upcoming(&game), ["Depart"]);
    for (id, expected) in [("ajax", "At the shop"), ("ajax-2", "At the gate")] {
        assert_eq!(
            game.current_summary()
                .characters()
                .get(&CharacterId::new(id).unwrap())
                .unwrap()
                .details()
                .current_state()
                .unwrap()
                .as_str(),
            expected
        );
    }
    let before_error = game.clone();
    let source = CancellationSource::default();
    let mut preview = String::new();
    let error = cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source.token(),
            &mut |text| preview.push_str(text),
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Transport);
    assert_eq!(
        error.raw_response().as_str().as_bytes(),
        malformed.as_bytes()
    );
    assert_eq!(preview, "Broken preview 🎭");
    assert_eq!(game, before_error);
    advance(&mut cases, &mut game); // Explicit retry, exactly one further request.
    assert_eq!(events(&game), ["C", "D"]);
    assert!(upcoming(&game).is_empty());
    assert_eq!(game.current_chapter().unwrap().number().get(), 1);
    assert_eq!(game.prose_context().0.len(), 2);
    let before_cancel = game.clone();
    let mut preview = String::new();
    let error = cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source.token(),
            &mut |text| {
                preview.push_str(text);
                source.cancel();
            },
        )
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Cancelled);
    assert!(!preview.is_empty());
    assert!(turns[3].starts_with(error.raw_response().as_str()));
    assert!(error.raw_response().as_str().len() < turns[3].len());
    assert_eq!(game, before_cancel);
    advance(&mut cases, &mut game);
    assert_eq!(game.turns().len(), 4);
    assert_eq!(events(&game), ["D", "E"]);
    assert_eq!(
        game.current_chapter().unwrap().title().unwrap().as_str(),
        "At Sea"
    );
    cases.rewind(&mut game, TurnCount::new(2).unwrap()).unwrap();
    assert_eq!(game, before_error);
    advance(&mut cases, &mut game);
    assert_eq!(game.turns().len(), 3);
    assert_eq!(
        game.current_chapter().unwrap().title().unwrap().as_str(),
        "Retitled"
    );
    assert_eq!(events(&game), ["C", "F"]);
    assert_eq!(upcoming(&game), ["Depart"]);
    for (record, expected) in game.turns().iter().zip([&turns[0], &turns[1], &turns[4]]) {
        assert_eq!(record.raw_response().as_str(), expected);
        assert!(record.prompt_trace().is_none());
    }
    let backend = cases.into_generator().into_backend();
    let requests = backend.requests();
    assert_eq!(requests.len(), 9); // World, cast, five successes, malformed, cancelled.
    assert!(
        requests[0].schema()["properties"]
            .get("world_description")
            .is_some()
    );
    assert!(
        requests[1].schema()["properties"]
            .get("characters")
            .is_some()
    );
    for (index, request) in requests.iter().enumerate().skip(2) {
        assert_eq!(
            request
                .prompt()
                .matches("Before writing the opening scene")
                .count(),
            usize::from(index == 2)
        );
        assert!(request.instructions().contains("at most 2 major events"));
        assert!(request.schema()["$defs"]["SummaryUpdate"]["properties"]["consolidated_major_events"]["description"].as_str().unwrap().contains("2 major events"));
        for id in ["protagonist", "ajax", "ajax-2"] {
            assert!(request.prompt().contains(&format!("\"id\": \"{id}\"")));
        }
    }
    assert_eq!(requests[4].prompt(), requests[5].prompt()); // Failed response never became context.
    assert_eq!(requests[6].prompt(), requests[7].prompt()); // Nor did a cancelled preview.
    for prose in [
        "Opening: the lantern flickered.",
        "Second: the bell rang.",
        "Voyage: the ship sailed.",
    ] {
        assert!(requests[6].prompt().contains(prose));
    }
    assert!(
        requests[6]
            .prompt()
            .contains("The closing prose of the previous chapter")
    );
    assert_eq!(requests[8].prompt(), requests[4].prompt()); // Exact restored context after rewind.
    assert!(!requests[8].prompt().contains("Voyage: the ship sailed."));
    assert!(!requests[8].prompt().contains("Fourth: the waves rose."));
}

fn events(game: &GameState) -> Vec<String> {
    game.current_summary()
        .major_events()
        .events()
        .as_slice()
        .iter()
        .map(|event| event.as_str().to_owned())
        .collect()
}
fn upcoming(game: &GameState) -> Vec<String> {
    game.current_summary()
        .upcoming_events()
        .as_slice()
        .iter()
        .map(|event| event.as_str().to_owned())
        .collect()
}
