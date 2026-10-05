use cyoa_application::persistence::{SaveCopy, SaveId};
use cyoa_infrastructure::persistence::codec::{SaveCodecErrorKind, decode};
use serde_json::Value;
fn id() -> SaveId {
    SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap()
}
fn minimal() -> Value {
    serde_json::from_str(include_str!("fixtures/saves/v1-minimal.json")).unwrap()
}
#[test]
fn malformed_versions_metadata_and_world_are_rejected_without_repair() {
    for (path, value) in [
        ("/version", Value::from(0)),
        ("/revision", Value::from(0)),
        ("/turn_count", Value::from(1)),
        ("/title", Value::from("Other")),
        ("/id", Value::from("other-0123456789abcdef0123456789abcdef")),
        ("/saved_at", Value::from("not a date")),
        ("/game/character_index", Value::from(2)),
        ("/game/original_limits/max_major_events", Value::from(0)),
        ("/game/world/characters/0/name", Value::from(" Ajax ")),
        ("/game/world/characters/0/backstory", Value::from("")),
    ] {
        let mut value_json = minimal();
        *value_json.pointer_mut(path).unwrap() = value;
        assert!(
            decode(
                &serde_json::to_vec(&value_json).unwrap(),
                &id(),
                SaveCopy::Primary
            )
            .is_err(),
            "accepted invalid field {path}"
        );
    }
    let mut value = minimal();
    value["game"]["world"]["characters"][1] = value["game"]["world"]["characters"][0].clone();
    assert!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            &id(),
            SaveCopy::Primary
        )
        .is_err()
    );
    value["version"] = 2.into();
    assert!(matches!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            &id(),
            SaveCopy::Primary
        )
        .unwrap_err()
        .kind(),
        SaveCodecErrorKind::FutureVersion {
            found: 2,
            supported: 1
        }
    ));
}
#[test]
fn invalid_json_utf8_and_duplicate_keys_are_never_last_key_wins() {
    for bytes in [
        b"{".as_slice(),
        b"\xff",
        b"{\"version\":2,\"version\":1}",
        b"{\"version\":1} trailing",
    ] {
        assert!(decode(bytes, &id(), SaveCopy::Primary).is_err());
    }
}

fn full() -> Value {
    serde_json::from_str(include_str!("fixtures/saves/v1-full-story.json")).unwrap()
}
fn reject(value: &Value, reason: &str) {
    assert!(
        decode(
            &serde_json::to_vec(value).unwrap(),
            &id(),
            SaveCopy::Primary
        )
        .is_err(),
        "accepted corrupt save: {reason}"
    );
}
#[test]
fn stored_history_cannot_be_normalized_capped_or_reidentified() {
    for (path, replacement) in [
        (
            "/game/turns/0/summary/characters/1/id",
            Value::from("protagonist"),
        ),
        ("/game/turns/0/summary/characters/1/id", Value::from("")),
        (
            "/game/turns/0/summary/characters/1/name",
            Value::from(" Ajax"),
        ),
        (
            "/game/turns/0/summary/characters/1/description",
            Value::from(""),
        ),
        ("/game/turns/0/turn/narrative", Value::from(" ")),
        ("/game/turns/0/turn/scene_description", Value::from(" ")),
        ("/game/turns/0/turn/chapter_title", Value::from("")),
        ("/game/turns/0/player_input", Value::from(" ")),
        (
            "/game/turns/0/summary/major_events",
            serde_json::json!(["A", "a"]),
        ),
        (
            "/game/turns/0/summary/major_events",
            serde_json::json!(["A", "B", "C", "D", "E"]),
        ),
        (
            "/game/turns/0/summary/upcoming_events",
            serde_json::json!(["Depart", ""]),
        ),
        (
            "/game/turns/0/turn/summary_update/new_major_events",
            serde_json::json!([" A"]),
        ),
        (
            "/game/turns/0/turn/summary_update/consolidated_major_events",
            serde_json::json!(["A", "a"]),
        ),
        (
            "/game/turns/0/turn/summary_update/upcoming_events",
            serde_json::json!(["A", "a"]),
        ),
        ("/game/turns/0/turn/quick_actions", serde_json::json!([])),
        (
            "/game/turns/0/turn/quick_actions",
            serde_json::json!([{"text":"Go"},{"text":"go"}]),
        ),
        (
            "/game/turns/0/turn/quick_actions",
            serde_json::json!([{"text":"A"},{"text":"B"},{"text":"C"},{"text":"D"}]),
        ),
        (
            "/game/turns/1/provenance/list_price_estimate/amount",
            Value::from(-1),
        ),
        (
            "/game/turns/1/provenance/list_price_estimate/currency",
            Value::from(""),
        ),
        ("/game/turns/1/provenance/provider", Value::from("")),
        (
            "/game/active_limits/min_playable_characters",
            Value::from(0),
        ),
        ("/game/active_limits/prose_bridge_turns", Value::from(-1)),
        ("/revision", serde_json::json!(1.5)),
        (
            "/source",
            serde_json::json!({"kind":"demo","scenario":"unknown"}),
        ),
        ("/source", serde_json::json!({"kind":"unknown"})),
        ("/saved_at", Value::from("2026-10-04T12:00:00+01:00")),
    ] {
        let mut value = full();
        // The full frozen fixture intentionally omits defaulted delta fields.
        let (parent, key) = path.rsplit_once('/').unwrap();
        value
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), replacement);
        if path.ends_with("characters/1/description") {
            value["game"]["turns"][0]["summary"]["characters"][1]["backstory"] = "".into();
        }
        reject(&value, path);
    }
    let mut value = full();
    value["game"]["world"]["npcs"][1] = value["game"]["world"]["npcs"][0].clone();
    reject(&value, "duplicate NPC");
    value["game"]["world"]["characters"] = serde_json::json!([]);
    reject(&value, "no playable");
    value = full();
    value["game"]["turns"][0]["summary"]["characters"] = serde_json::json!([]);
    reject(&value, "empty summary cast");
}
#[test]
fn required_fields_and_known_field_types_cannot_hide_behind_defaults() {
    for path in [
        "/id",
        "/version",
        "/revision",
        "/title",
        "/saved_at",
        "/turn_count",
        "/source",
        "/game",
        "/game/brief",
        "/game/world",
        "/game/character_index",
        "/game/original_limits",
        "/game/active_limits",
        "/game/world/title",
        "/game/world/world_description",
        "/game/world/characters/0/name",
        "/game/world/characters/0/description",
        "/game/world/characters/0/backstory",
        "/game/world/npcs/0/relationships",
        "/game/active_limits/max_major_events",
        "/game/active_limits/max_generated_npcs",
        "/game/active_limits/prose_bridge_turns",
        "/game/active_limits/min_playable_characters",
        "/game/turns/0/player_input",
        "/game/turns/0/raw_response",
        "/game/turns/0/turn",
        "/game/turns/0/summary",
        "/game/turns/0/turn/narrative",
        "/game/turns/0/turn/quick_actions",
        "/game/turns/0/turn/scene_description",
        "/game/turns/0/turn/summary_update",
        "/game/turns/0/turn/starts_new_chapter",
        "/game/turns/0/turn/chapter_title",
        "/game/turns/0/turn/summary_update/current_situation",
        "/game/turns/0/turn/summary_update/character_updates",
        "/game/turns/0/turn/summary_update/new_major_events",
        "/game/turns/0/turn/summary_update/character_updates/0/id",
        "/game/turns/0/turn/summary_update/character_updates/0/current_state",
        "/game/turns/0/turn/quick_actions/0/text",
        "/game/turns/0/summary/world",
        "/game/turns/0/summary/major_events",
        "/game/turns/0/summary/characters",
        "/game/turns/0/summary/current_situation",
        "/game/turns/0/summary/upcoming_events",
        "/game/turns/0/summary/characters/0/name",
        "/game/turns/0/summary/characters/0/description",
        "/game/turns/0/summary/characters/0/backstory",
        "/game/turns/0/summary/characters/0/relationships",
        "/game/turns/1/provenance/list_price_estimate/amount",
        "/game/turns/1/provenance/list_price_estimate/currency",
        "/game/turns/2/prompt_trace/instructions",
        "/game/turns/2/prompt_trace/prompt",
    ] {
        let (parent, key) = path.rsplit_once('/').unwrap();
        let mut value = full();
        assert!(
            value
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key)
                .is_some(),
            "invalid test path {path}"
        );
        reject(&value, &format!("missing {path}"));
        let mut value = full();
        *value.pointer_mut(path).unwrap() = serde_json::json!({"wrong":"type"});
        reject(&value, &format!("wrong type {path}"));
    }
    for path in [
        "/game/turns/0/turn/scene_description",
        "/game/world/npcs/0/relationships",
        "/game/pace",
        "/game/turns/0/summary/characters/0/id",
    ] {
        let mut value = full();
        *value.pointer_mut(path).unwrap() = Value::Null;
        reject(&value, path);
    }
}
#[test]
fn size_depth_overflow_and_nested_duplicate_keys_are_bounded_errors() {
    let oversized = vec![b' '; cyoa_application::persistence::MAX_SAVE_BYTES + 1];
    assert_eq!(
        decode(&oversized, &id(), SaveCopy::Primary)
            .unwrap_err()
            .kind(),
        &SaveCodecErrorKind::TooLarge
    );
    let deep = format!("{}0{}", "[".repeat(129), "]".repeat(129));
    assert!(decode(deep.as_bytes(), &id(), SaveCopy::Primary).is_err());
    for bytes in [
        include_bytes!("fixtures/saves/invalid-duplicate-key.json").as_slice(),
        b"{\"version\":1,\"extra\":{\"x\":1,\"x\":2}}",
        b"{\"version\":18446744073709551616}",
    ] {
        assert!(decode(bytes, &id(), SaveCopy::Primary).is_err());
    }
    let mut value = minimal();
    value["version"] = Value::from(u64::from(u32::MAX) + 1);
    reject(&value, "version overflow");
    if usize::BITS < 64 {
        value = minimal();
        value["game"]["character_index"] = Value::from(u64::MAX);
        reject(&value, "platform integer overflow");
    }
}
#[test]
fn cast_and_stored_character_ids_are_required_fields_not_failing_defaults() {
    // Both used to default to empty and then fail later validation; v1 always
    // writes them, so a missing one is reported as the missing field itself.
    for path in [
        "/game/world/characters",
        "/game/turns/0/summary/characters/0/id",
    ] {
        let (parent, key) = path.rsplit_once('/').unwrap();
        let mut value = full();
        value
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key)
            .unwrap();
        let error = decode(
            &serde_json::to_vec(&value).unwrap(),
            &id(),
            SaveCopy::Primary,
        )
        .unwrap_err()
        .to_string();
        let field = key.trim_start_matches('/');
        assert!(
            error.contains(&format!("missing field `{field}`")),
            "{path}: {error}"
        );
    }
}
