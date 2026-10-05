use cyoa_application::{cancellation::CancellationSource, generation::*, persistence::*};
use cyoa_core::{
    game::{GameState, TurnCount},
    limits::*,
    style::StoryStyle,
    text::*,
    turn::*,
    world::PlayablePosition,
};
use cyoa_infrastructure::{
    generation::{
        engine::GenerationEngine,
        scripted::{ChunkedBackend, ScriptedBackend},
        templates::GenerationTemplates,
    },
    persistence::{
        codec::{decode, encode, stamp},
        repository::LocalRepository,
    },
};
use serde_json::Value;
fn id() -> SaveId {
    SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap()
}
fn data() -> Value {
    serde_json::from_str(include_str!("fixtures/phase0_story.json")).unwrap()
}
fn events(game: &GameState) -> Vec<String> {
    game.current_summary()
        .major_events()
        .events()
        .iter()
        .map(|e| e.as_str().to_owned())
        .collect()
}
fn source() -> CancellationSource {
    CancellationSource::default()
}
#[test]
fn frozen_full_story_roundtrip_preserves_complete_state_exact_audit_and_next_rewind_request() {
    let data = data();
    let mut responses = vec![
        Ok(data["outline"].to_string()),
        Ok(data["cast"].to_string()),
    ];
    for i in 0..5 {
        if i == 2 {
            responses.push(Ok(" \r\n{\"narrative\":\"Broken preview 🌊".into()));
        }
        if i == 3 {
            responses.push(Ok(data["turns"][i].to_string()));
        }
        responses.push(Ok(data["turns"][i].to_string()));
    }
    responses.push(Ok(data["turns"][4].to_string()));
    let mut cases = StoryUseCases::new(GenerationEngine::new(
        ChunkedBackend::new(responses, 42),
        GenerationTemplates::bundled().unwrap(),
    ));
    let brief = Brief::new("Two Ajaxes, a harbour and a voyage").unwrap();
    let cancel = source();
    let token = cancel.token();
    let outline = cases
        .generate_outline(&brief, &token)
        .unwrap()
        .into_parts()
        .0;
    let limits = Limits {
        max_major_events: MajorEventLimit::new(4).unwrap(),
        ..Limits::default()
    };
    let world = cases
        .generate_world(&brief, outline, &limits, &token)
        .unwrap();
    let style = StoryStyle {
        art_style: Some(ArtStyleKey::new("unknown-art").unwrap()),
        pace: Some(PaceKey::new("unknown-pace").unwrap()),
        tone: Some(ToneKey::new("unknown-tone").unwrap()),
        narration: Some(NarrationKey::new("unknown-narration").unwrap()),
    };
    let mut game = GameState::start(
        brief,
        world.select(PlayablePosition::new(1)).unwrap(),
        style,
        limits,
    );
    for i in 0..5 {
        if i == 2 {
            let before = game.clone();
            let error = cases
                .take_turn(
                    &mut game,
                    TurnDirection::Player(PlayerInput::new("Sail onward").unwrap()),
                    &token,
                    &mut |_| {},
                )
                .unwrap_err();
            assert_eq!(error.kind(), FailureKind::Transport);
            assert_eq!(game, before);
            let snapshot = SaveSnapshot::new(game.clone(), StorySource::Live).unwrap();
            let metadata = SaveMetadata {
                id: id(),
                revision: SaveRevision::new(1).unwrap(),
                saved_at: SavedAt::new(0, 0).unwrap(),
            };
            let bytes = encode(&snapshot, &metadata).unwrap();
            assert_eq!(
                decode(&bytes, &id(), SaveCopy::Primary).unwrap().snapshot,
                snapshot
            );
        }
        if i == 3 {
            let before = game.clone();
            let cancellation = source();
            let error = cases
                .take_turn(
                    &mut game,
                    TurnDirection::Player(PlayerInput::new("Sail onward").unwrap()),
                    &cancellation.token(),
                    &mut |_| cancellation.cancel(),
                )
                .unwrap_err();
            assert_eq!(error.kind(), FailureKind::Cancelled);
            assert_eq!(game, before);
        }
        let direction = if i == 0 {
            TurnDirection::Continue
        } else {
            TurnDirection::Player(PlayerInput::new("Sail onward").unwrap())
        };
        cases
            .take_turn(&mut game, direction, &token, &mut |_| {})
            .unwrap();
    }
    // Enrich existing records with independently specified historical audit
    // values, including traces that default generation deliberately omits.
    let raws = [
        "",
        " \t\r\n",
        "\r\nquoted \"reply\" 🌊\r\n",
        "response four",
        "response five",
    ];
    let turns = game
        .turns()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            TurnRecord::new(
                r.turn().clone(),
                r.summary().clone(),
                TurnGenerationRecord {
                    input: r.input().cloned(),
                    raw_response: RawResponse::new(raws[i]),
                    provenance: if i == 1 {
                        GenerationProvenance {
                            provider: Some(ProviderName::new("observed-provider").unwrap()),
                            model: Some(ModelName::new("observed-model").unwrap()),
                            cost: Some(ListPriceEstimate::new(
                                CostAmount::new(0.0).unwrap(),
                                CurrencyCode::new("USD").unwrap(),
                            )),
                        }
                    } else {
                        Default::default()
                    },
                    prompt_trace: if i == 2 {
                        Some(PromptTrace {
                            instructions: Instructions::new(" \r\nInstructions 🌊\t"),
                            prompt: RenderedPrompt::new("\r\nPrompt \"quoted\"\r\n"),
                        })
                    } else {
                        None
                    },
                },
            )
        })
        .collect();
    game = GameState::restore(
        game.brief().clone(),
        game.selected_world().clone(),
        game.style().clone(),
        game.original_limits(),
        RestoreLimits::Current(game.limits()),
        turns,
    );
    let frozen = decode(
        include_bytes!("fixtures/saves/v1-full-story.json"),
        &id(),
        SaveCopy::Primary,
    )
    .unwrap();
    assert_eq!(
        game,
        *frozen.snapshot.game(),
        "independent frozen full-story fixture differs from actual generated story"
    );
    assert_eq!(events(&game), ["C", "D", "E", "F"]);
    assert_eq!(
        game.chapters()
            .iter()
            .map(|c| c.title().unwrap().as_str())
            .collect::<Vec<_>>(),
        ["Retitled", "At Sea"]
    );
    let bytes = encode(&frozen.snapshot, &frozen.metadata).unwrap();
    assert!(bytes.ends_with(b"\n"));
    assert!(!bytes.ends_with(b"\n\n"));
    let loaded = decode(&bytes, &id(), SaveCopy::Backup).unwrap();
    assert_eq!(loaded.snapshot, frozen.snapshot);
    assert_eq!(loaded.metadata, frozen.metadata);
    assert_eq!(loaded.stamp, stamp(&bytes));
    assert_eq!(loaded.copy, SaveCopy::Backup);
    for (record, raw) in loaded.snapshot.game().turns().iter().zip(raws) {
        assert_eq!(record.raw_response().as_str().as_bytes(), raw.as_bytes());
    }
    assert_eq!(
        loaded.snapshot.game().turns()[2]
            .prompt_trace()
            .unwrap()
            .prompt
            .as_str()
            .as_bytes(),
        b"\r\nPrompt \"quoted\"\r\n"
    );
    assert!(
        loaded.snapshot.game().turns()[0]
            .provenance()
            .cost
            .is_none()
    );
    assert_eq!(
        loaded.snapshot.game().turns()[1]
            .provenance()
            .cost
            .as_ref()
            .unwrap()
            .amount()
            .get(),
        0.0
    );
    // Compose the complete generated story with actual atomic disk storage.
    // The frozen fixture remains the independent oracle for every game field.
    let root = tempfile::tempdir().unwrap();
    let mut repository = LocalRepository::new(root.path().into()).unwrap();
    let receipt = repository
        .create(
            loaded.snapshot.clone(),
            &PreparedWriteEvidence::default(),
            &token,
        )
        .unwrap();
    let primary = root
        .path()
        .join("saves")
        .join(format!("{}.json", receipt.metadata.id.as_str()));
    let backup = primary.with_extension("json.bak");
    let before_rewind = std::fs::read(&primary).unwrap();
    let disk_game = repository
        .load(&receipt.metadata.id, SaveCopy::Primary, &token)
        .unwrap();
    assert_eq!(
        disk_game.snapshot, frozen.snapshot,
        "complete story disk reconstruction must retain all fields"
    );
    let mut restored = disk_game.snapshot.into_game();
    cases
        .rewind(&mut restored, TurnCount::new(3).unwrap())
        .unwrap();
    let rewound = SaveSnapshot::new(restored.clone(), StorySource::Live).unwrap();
    let receipt = repository
        .replace(
            SaveTarget {
                id: receipt.metadata.id,
                expected_stamp: receipt.stamp,
            },
            rewound.clone(),
            &PreparedWriteEvidence::default(),
            &token,
        )
        .unwrap();
    assert_eq!(
        std::fs::read(&backup).unwrap(),
        before_rewind,
        "rewind backup must retain the exact complete story document"
    );
    let reloaded = repository
        .load(&receipt.metadata.id, SaveCopy::Primary, &token)
        .unwrap();
    assert_eq!(reloaded.snapshot, rewound);
    restored = reloaded.snapshot.into_game();
    assert_eq!(events(&restored), ["A", "B", "C"]);
    assert_eq!(
        restored
            .current_chapter()
            .unwrap()
            .title()
            .unwrap()
            .as_str(),
        "Retitled"
    );
    let expected_request = GenerationTemplates::bundled()
        .unwrap()
        .turn_request(&restored, Some("Sail onward"), false)
        .unwrap();
    assert!(
        expected_request
            .prompt()
            .as_str()
            .contains("Second: the bell rang.")
    );
    assert!(
        !expected_request
            .prompt()
            .as_str()
            .contains("Voyage: the ship sailed.")
    );
    for expected in [
        "\"id\": \"protagonist\"",
        "\"id\": \"ajax\"",
        "\"id\": \"ajax-2\"",
        "\"B\"",
        "\"C\"",
        "Depart",
    ] {
        assert!(
            expected_request.prompt().as_str().contains(expected),
            "next prompt missing {expected}"
        );
    }
    cases
        .take_turn(
            &mut restored,
            TurnDirection::Player(PlayerInput::new("Sail onward").unwrap()),
            &token,
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(events(&restored), ["A", "B", "C", "F"]);
    let before_continuation = std::fs::read(&primary).unwrap();
    let continued = SaveSnapshot::new(restored.clone(), StorySource::Live).unwrap();
    let receipt = repository
        .replace(
            SaveTarget {
                id: receipt.metadata.id,
                expected_stamp: receipt.stamp,
            },
            continued.clone(),
            &PreparedWriteEvidence::default(),
            &token,
        )
        .unwrap();
    assert_eq!(receipt.metadata.revision.get(), 3);
    assert_eq!(std::fs::read(&backup).unwrap(), before_continuation);
    assert_eq!(
        repository
            .load(&receipt.metadata.id, SaveCopy::Primary, &token)
            .unwrap()
            .snapshot,
        continued,
        "post-restart continuation must persist exact accepted canonical state"
    );
    let backend = cases.into_generator().into_backend();
    assert_eq!(backend.requests().len(), 10);
    assert_eq!(
        backend.requests()[4].prompt(),
        backend.requests()[5].prompt()
    );
    assert_eq!(
        backend.requests()[6].prompt(),
        backend.requests()[7].prompt()
    );
    let last = backend.requests().last().unwrap();
    assert_eq!(last.prompt(), expected_request.prompt().as_str());
    assert_eq!(
        last.instructions(),
        expected_request.instructions().as_str()
    );
    assert_eq!(last.schema(), expected_request.schema());
}
#[test]
fn zero_turn_selected_second_playable_roundtrip_restores_one_opening_request() {
    let frozen = decode(
        include_bytes!("fixtures/saves/v1-minimal.json"),
        &id(),
        SaveCopy::Primary,
    )
    .unwrap();
    let bytes = encode(&frozen.snapshot, &frozen.metadata).unwrap();
    let mut game = decode(&bytes, &id(), SaveCopy::Primary)
        .unwrap()
        .snapshot
        .into_game();
    assert_eq!(game.protagonist().description().as_str(), "A mason");
    assert!(game.turns().is_empty());
    let mut cases = StoryUseCases::new(GenerationEngine::new(
        ScriptedBackend::new([Ok(data()["turns"][0].to_string())]),
        GenerationTemplates::bundled().unwrap(),
    ));
    cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &source().token(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(game.turns().len(), 1);
    assert_eq!(game.current_chapter().unwrap().number().get(), 0);
    let backend = cases.into_generator().into_backend();
    assert_eq!(backend.requests().len(), 1);
    assert_eq!(
        backend.requests()[0]
            .prompt()
            .matches("Before writing the opening scene")
            .count(),
        1
    );
}
#[test]
fn content_stamps_cover_exact_bytes_and_have_independent_sha256_evidence() {
    assert_eq!(
        stamp(b"abc").bytes(),
        &[
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad
        ]
    );
    assert_ne!(stamp(b"abc"), stamp(b"abc\n"));
}
