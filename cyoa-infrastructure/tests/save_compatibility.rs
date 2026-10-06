use cyoa_application::persistence::{SaveCopy, SaveId, SaveSnapshot};
use cyoa_core::{summary::UpcomingEventsUpdate, turn::QuickActionKind};
use cyoa_infrastructure::{
    generation::templates::GenerationTemplates,
    persistence::codec::{SaveCodecErrorKind, decode, encode},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
fn id() -> SaveId {
    SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap()
}
fn full() -> Value {
    serde_json::from_str(include_str!("fixtures/saves/v1-full-story.json")).unwrap()
}
fn read(value: &Value) -> cyoa_application::persistence::StoredGame {
    decode(
        &serde_json::to_vec(value).unwrap(),
        &id(),
        SaveCopy::Primary,
    )
    .unwrap()
}
#[test]
fn only_shipped_version_one_is_supported_and_reads_leave_input_bytes_unchanged() {
    for value in [
        json!({}),
        json!({"version":0}),
        json!({"version":-1}),
        json!({"version":1.5}),
        json!({"version":"1"}),
    ] {
        assert!(
            decode(
                &serde_json::to_vec(&value).unwrap(),
                &id(),
                SaveCopy::Primary
            )
            .is_err()
        );
    }
    let future = decode(
        include_bytes!("fixtures/saves/invalid-future-version.json"),
        &id(),
        SaveCopy::Primary,
    )
    .unwrap_err();
    assert_eq!(
        future.kind(),
        &SaveCodecErrorKind::FutureVersion {
            found: cyoa_application::persistence::SaveFormatVersion::new(2).unwrap(),
            supported: cyoa_application::persistence::SaveFormatVersion::new(1).unwrap(),
        }
    );
    assert!(future.to_string().contains("use a newer cyoa"));
    let bytes = include_bytes!("fixtures/saves/v1-minimal.json").to_vec();
    let before = bytes.clone();
    decode(&bytes, &id(), SaveCopy::Backup).unwrap();
    assert_eq!(bytes, before);
}
#[test]
fn optional_omissions_nulls_and_empty_replacements_keep_distinct_meanings() {
    let minimal = decode(
        include_bytes!("fixtures/saves/v1-minimal.json"),
        &id(),
        SaveCopy::Primary,
    )
    .unwrap();
    assert!(minimal.snapshot.game().turns().is_empty());
    assert!(minimal.snapshot.game().world().cast().npcs().is_empty());
    assert_eq!(minimal.snapshot.game().style(), &Default::default());
    assert_eq!(minimal.snapshot.game().selected_world().position().get(), 1);
    let mut value = full();
    for record in value["game"]["turns"].as_array_mut().unwrap() {
        record.as_object_mut().unwrap().remove("provenance");
        record.as_object_mut().unwrap().remove("prompt_trace");
        record["turn"]["summary_update"]
            .as_object_mut()
            .unwrap()
            .remove("world");
        record["turn"]["summary_update"]
            .as_object_mut()
            .unwrap()
            .remove("consolidated_major_events");
        for character in record["summary"]["characters"].as_array_mut().unwrap() {
            character.as_object_mut().unwrap().remove("current_state");
        }
    }
    value["game"]["turns"][0]["turn"]["summary_update"]
        .as_object_mut()
        .unwrap()
        .remove("upcoming_events");
    let loaded = read(&value);
    let turns = loaded.snapshot.game().turns();
    assert!(matches!(
        turns[0].turn().summary_update().upcoming_events,
        UpcomingEventsUpdate::Keep
    ));
    assert!(matches!(
        turns[1].turn().summary_update().upcoming_events,
        UpcomingEventsUpdate::Keep
    ));
    assert!(
        matches!(&turns[2].turn().summary_update().upcoming_events,UpcomingEventsUpdate::Replace(events) if events.is_empty())
    );
    assert_eq!(
        turns[0].turn().quick_actions().as_slice()[0].kind(),
        QuickActionKind::Other
    );
    assert_eq!(
        turns[1].turn().quick_actions().as_slice()[0].kind(),
        QuickActionKind::Other
    );
    for record in turns {
        assert_eq!(record.provenance(), &Default::default());
        assert!(record.prompt_trace().is_none());
        assert!(
            record
                .summary()
                .characters()
                .iter()
                .all(|c| c.details().current_state().is_none())
        );
    }
    value["game"]["turns"][0]["summary"]["characters"][0]
        .as_object_mut()
        .unwrap()
        .remove("id");
    assert!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            &id(),
            SaveCopy::Primary
        )
        .is_err(),
        "defaulted empty IDs cannot establish durable identity"
    );
}
#[test]
fn additive_optional_fields_warn_and_are_omitted_on_resave_while_known_fields_survive() {
    let extra = decode(
        include_bytes!("fixtures/saves/v1-optional-extra.json"),
        &id(),
        SaveCopy::Primary,
    )
    .unwrap();
    assert_eq!(extra.snapshot, read(&full()).snapshot);
    for expected in [
        "future_optional",
        "game.future_style",
        "game.world.characters.0.optional_portrait",
    ] {
        assert!(
            extra
                .unrecognized_fields
                .iter()
                .any(|p| p.location() == expected),
            "missing unknown-field warning {expected}: {:?}",
            extra.unrecognized_fields
        );
    }
    let bytes = encode(&extra.snapshot, &extra.metadata).unwrap();
    let saved: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(saved.get("future_optional").is_none());
    assert!(saved["game"].get("future_style").is_none());
    let again = decode(&bytes, &id(), SaveCopy::Primary).unwrap();
    assert_eq!(again.snapshot, extra.snapshot);
    assert!(again.unrecognized_fields.is_empty());
    #[derive(Serialize, Deserialize)]
    struct LaterOptionalStyle {
        #[serde(default)]
        future_style: String,
    }
    assert_eq!(
        serde_json::from_value::<LaterOptionalStyle>(json!({}))
            .unwrap()
            .future_style,
        ""
    );
    assert!(serde_json::from_value::<LaterOptionalStyle>(json!({"future_style":null})).is_err());
}

#[test]
fn additive_source_fields_are_reported_for_both_source_variants() {
    for source in [
        json!({"kind":"live","extra":true}),
        json!({"kind":"demo","scenario":"harbour-v1","extra":true}),
    ] {
        let mut value = full();
        value["source"] = source;
        let loaded = read(&value);
        assert!(
            loaded
                .unrecognized_fields
                .iter()
                .any(|p| p.location() == "source.extra"),
            "source additions must warn: {:?}",
            loaded.unrecognized_fields
        );
    }
}
#[test]
fn valid_authoritative_snapshots_and_unknown_style_keys_are_preserved_without_delta_replay() {
    let mut value = full();
    value["game"]["turns"][0]["summary"]["world"] =
        "Edited memory independent of the proposal".into();
    value["game"]["turns"][0]["summary"]["characters"][0]["name"] =
        "Durable renamed protagonist".into();
    // Duplicated proposals remain evidence: only summary identities must be unique.
    let delta = value["game"]["turns"][0]["turn"]["summary_update"]["character_updates"][0].clone();
    value["game"]["turns"][0]["turn"]["summary_update"]["character_updates"]
        .as_array_mut()
        .unwrap()
        .push(delta);
    let stored = read(&value);
    assert_eq!(
        stored.snapshot.game().turns()[0].summary().world().as_str(),
        "Edited memory independent of the proposal"
    );
    assert_eq!(
        stored.snapshot.game().turns()[0]
            .summary()
            .characters()
            .iter()
            .next()
            .unwrap()
            .name()
            .as_str(),
        "Durable renamed protagonist"
    );
    let game = stored.snapshot.game();
    assert_eq!(game.style().pace.as_ref().unwrap().as_str(), "unknown-pace");
    let templates = GenerationTemplates::bundled().unwrap();
    let unknown = templates.turn_request(game, None, false).unwrap();
    let default_game = cyoa_core::game::GameState::restore(
        game.brief().clone(),
        game.selected_world().clone(),
        Default::default(),
        game.original_limits(),
        cyoa_core::limits::RestoreLimits::Current(game.limits()),
        game.turns().to_vec(),
    );
    let defaults = templates.turn_request(&default_game, None, false).unwrap();
    assert_eq!(unknown.instructions(), defaults.instructions());
    assert_eq!(unknown.prompt(), defaults.prompt());
    let bytes = encode(&stored.snapshot, &stored.metadata).unwrap();
    assert_eq!(
        decode(&bytes, &id(), SaveCopy::Primary).unwrap().snapshot,
        stored.snapshot
    );
}

#[test]
fn stored_unicode_matching_preserves_sharp_s_distinctions_and_rejects_lowercase_duplicates() {
    let mut value = full();
    value["game"]["turns"][0]["summary"]["major_events"] = json!(["Straße", "STRASSE"]);
    value["game"]["turns"][0]["turn"]["summary_update"]["new_major_events"] =
        json!(["Straße", "STRASSE"]);
    value["game"]["turns"][0]["turn"]["quick_actions"] =
        json!([{"text":"Straße"},{"text":"STRASSE"}]);
    let loaded = read(&value);
    assert_eq!(
        loaded.snapshot.game().turns()[0]
            .turn()
            .quick_actions()
            .as_slice()
            .len(),
        2
    );
    assert_eq!(
        loaded.snapshot.game().turns()[0]
            .summary()
            .major_events()
            .events()
            .iter()
            .map(|e| e.as_str())
            .collect::<Vec<_>>(),
        ["Straße", "STRASSE"]
    );
    value["game"]["turns"][0]["summary"]["major_events"] = json!(["Straße", "STRAẞE"]);
    assert!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            &id(),
            SaveCopy::Primary
        )
        .is_err()
    );
}
#[test]
fn supported_demo_source_roundtrips_and_rejects_impossible_passage_counts() {
    let mut value = full();
    value["source"] = json!({"kind":"demo","scenario":"harbour-v1"});
    let loaded = read(&value);
    assert_eq!(
        loaded.snapshot.source(),
        cyoa_application::persistence::StorySource::Demo {
            scenario: cyoa_application::persistence::DemoScenarioId::HarbourV1
        }
    );
    let bytes = encode(&loaded.snapshot, &loaded.metadata).unwrap();
    assert_eq!(
        decode(&bytes, &id(), SaveCopy::Primary).unwrap().snapshot,
        loaded.snapshot
    );
    let record = value["game"]["turns"][0].clone();
    value["game"]["turns"].as_array_mut().unwrap().push(record);
    value["turn_count"] = 6.into();
    assert!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            &id(),
            SaveCopy::Primary
        )
        .is_err()
    );
    value["source"] = json!({"kind":"live"});
    let live = read(&value);
    // Such a snapshot can no longer be built, so encode never sees one.
    assert_eq!(
        SaveSnapshot::new(live.snapshot.into_game(), loaded.snapshot.source()).unwrap_err(),
        cyoa_application::persistence::DemoTooLong {
            scenario: cyoa_application::persistence::DemoScenarioId::HarbourV1,
            turns: 6,
        }
    );
}

#[test]
fn audit_strings_including_empty_whitespace_crlf_and_unicode_roundtrip_without_normalization() {
    for audit in ["", " \t\r\n", "\r\nquoted \"reply\" 🌊\r\n"] {
        let mut value = full();
        value["game"]["turns"][0]["raw_response"] = audit.into();
        value["game"]["turns"][0]["prompt_trace"] = json!({"instructions":audit,"prompt":audit});
        let loaded = read(&value);
        let encoded = encode(&loaded.snapshot, &loaded.metadata).unwrap();
        let again = decode(&encoded, &id(), SaveCopy::Primary).unwrap();
        let record = &again.snapshot.game().turns()[0];
        assert_eq!(record.raw_response().as_str().as_bytes(), audit.as_bytes());
        let trace = record.prompt_trace().unwrap();
        assert_eq!(trace.instructions.as_str().as_bytes(), audit.as_bytes());
        assert_eq!(trace.prompt.as_str().as_bytes(), audit.as_bytes());
    }
}
