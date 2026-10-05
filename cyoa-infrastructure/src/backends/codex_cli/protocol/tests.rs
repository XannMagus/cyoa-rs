use super::*;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reviews/2026-09-26-codex-profile")
}
fn fixture(name: &str) -> Vec<u8> {
    fs::read(root().join("synthetic").join(name)).unwrap()
}
fn payload() -> String {
    let manifest: Value = serde_json::from_slice(&fixture("expectations.json")).unwrap();
    manifest["success_payload"].as_str().unwrap().into()
}
fn decode(bytes: &[u8]) -> Result<Completion, Failure> {
    let mut protocol = Protocol::default();
    // Match the supervisor contract: remove LF only, retain CR and final record.
    for record in bytes.split_inclusive(|b| *b == b'\n') {
        let record = record.strip_suffix(b"\n").unwrap_or(record);
        if protocol.record(record).is_err() {
            break;
        }
    }
    protocol.finish()
}
fn events() -> Vec<Value> {
    fixture("success.jsonl")
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).unwrap())
        .collect()
}
fn transcript(events: Vec<Value>) -> Vec<u8> {
    events
        .into_iter()
        .flat_map(|v| {
            let mut b = serde_json::to_vec(&v).unwrap();
            b.push(b'\n');
            b
        })
        .collect()
}

#[test]
fn frozen_error_transcripts_reject_with_located_reasons_and_candidate_evidence() {
    use ErrorKind::*;
    for (file, kind, record, retained) in [
        ("missing-terminal.jsonl", Incomplete, 4, true),
        ("duplicate-terminal.jsonl", DuplicateTerminal, 5, true),
        ("conflicting-terminal.jsonl", ConflictingTerminal, 5, true),
        ("failed-after-candidate.jsonl", VendorFailure, 4, true),
        ("ambiguous.jsonl", MultipleCandidates, 4, true),
        ("commentary-then-json.jsonl", MultipleCandidates, 4, true),
        ("ineligible.jsonl", Unsupported, 3, false),
        ("wrong-order.jsonl", Order, 3, false),
        ("unknown-event.jsonl", Unsupported, 3, false),
        ("reconnect-notice.jsonl", ErrorNotice, 3, false),
        ("missing-text.jsonl", InvalidField, 3, false),
        ("truncated.jsonl", InvalidJson, 4, true),
        ("invalid-utf8.jsonl", InvalidUtf8, 1, false),
    ] {
        let failure = decode(&fixture(file)).unwrap_err();
        assert_eq!(failure.error.kind, kind, "{file}");
        assert_eq!(failure.error.record, record, "{file}");
        assert!(failure.error.location.starts_with('$'), "{file}");
        let expected = if file == "commentary-then-json.jsonl" {
            "Working on it".into()
        } else {
            payload()
        };
        assert_eq!(
            failure.candidate.as_ref().map(|c| c.payload.as_str()),
            retained.then_some(expected.as_str()),
            "{file}"
        );
    }
}

#[test]
fn malformed_required_fields_and_unsupported_items_are_not_candidates() {
    for (index, replacement, location, kind) in [
        (
            0,
            json!({"type":"thread.started"}),
            "$.thread_id",
            ErrorKind::InvalidField,
        ),
        (
            0,
            json!({"type":"thread.started","thread_id":"  "}),
            "$.thread_id",
            ErrorKind::InvalidField,
        ),
        (0, json!({"type":3}), "$.type", ErrorKind::InvalidField),
        (0, json!([]), "$", ErrorKind::InvalidField),
        (
            2,
            json!({"type":"item.completed","item":null}),
            "$.item",
            ErrorKind::InvalidField,
        ),
        (
            2,
            json!({"type":"item.completed","item":{"id":"","type":"agent_message","text":"{}"}}),
            "$.item.id",
            ErrorKind::InvalidField,
        ),
        (
            2,
            json!({"type":"item.completed","item":{"id":2,"type":"agent_message","text":"{}"}}),
            "$.item.id",
            ErrorKind::InvalidField,
        ),
        (
            2,
            json!({"type":"item.completed","item":{"id":"i","type":"agent_message","text":null}}),
            "$.item.text",
            ErrorKind::InvalidField,
        ),
        (
            2,
            json!({"type":"item.completed","item":{"id":"i","type":"command_execution","text":"{}"}}),
            "$.item.type",
            ErrorKind::Unsupported,
        ),
        (
            2,
            json!({"type":"item.started","item":{"id":"i"}}),
            "$.type",
            ErrorKind::Unsupported,
        ),
        (
            3,
            json!({"type":"turn.failed","error":{}}),
            "$.error.message",
            ErrorKind::InvalidField,
        ),
    ] {
        let mut input = events();
        input[index] = replacement;
        let failure = decode(&transcript(input)).unwrap_err();
        assert_eq!(failure.error.kind, kind);
        assert_eq!(failure.error.record, index + 1);
        assert_eq!(failure.error.location, location);
    }
}

#[test]
fn order_failures_and_end_of_stream_never_authorize_completion() {
    let input = events();
    for prefix in 0..4 {
        let failure = decode(&transcript(input[..prefix].to_vec())).unwrap_err();
        assert_eq!(failure.error.kind, ErrorKind::Incomplete);
        assert_eq!(failure.candidate.is_some(), prefix == 3);
    }
    for indices in [
        vec![1],
        vec![2],
        vec![3],
        vec![0, 0],
        vec![0, 1, 1],
        vec![0, 1, 3],
        vec![0, 1, 2, 3, 2],
    ] {
        let failure = decode(&transcript(
            indices.into_iter().map(|i| input[i].clone()).collect(),
        ))
        .unwrap_err();
        assert_eq!(failure.error.kind, ErrorKind::Order);
    }
    let failure = decode(&transcript(vec![
        input[0].clone(),
        input[1].clone(),
        input[2].clone(),
        input[2].clone(),
    ]))
    .unwrap_err();
    assert_eq!(
        failure.error.kind,
        ErrorKind::MultipleCandidates,
        "reused item identity must fail too"
    );
}

#[test]
fn invalid_payload_is_retained_and_never_repaired_or_accepted() {
    for text in [
        "",
        " \r\n",
        "Working on it",
        "{",
        "{} trailing",
        "```json\n{}\n```",
    ] {
        let mut input = events();
        input[2]["item"]["text"] = json!(text);
        let failure = decode(&transcript(input)).unwrap_err();
        assert_eq!(failure.error.kind, ErrorKind::InvalidPayload);
        assert_eq!(failure.error.record, 3);
        assert_eq!(failure.error.location, "$.item.text");
        assert_eq!(failure.candidate.unwrap().payload, text);
    }
}

#[test]
fn optional_telemetry_preserves_unknown_zero_and_normalizes_invalid_cache() {
    for (file, total, cached, output) in [
        ("success.jsonl", Some(10), Some(0), Some(5)),
        ("absent-usage.jsonl", None, None, None),
        ("zero-usage.jsonl", Some(0), Some(0), Some(0)),
        ("invalid-cache.jsonl", Some(10), None, Some(5)),
    ] {
        let completion = decode(&fixture(file)).unwrap();
        assert_eq!(completion.usage.input.map(|v| v.total()), total, "{file}");
        assert_eq!(
            completion.usage.input.and_then(|v| v.cached()),
            cached,
            "{file}"
        );
        assert_eq!(completion.usage.output, output, "{file}");
    }
    for usage in [
        Value::Null,
        json!("bad"),
        json!([]),
        json!({}),
        json!({"input_tokens":-1,"output_tokens":1.5}),
        json!({"input_tokens":"10","cached_input_tokens":0,"output_tokens":true}),
    ] {
        let mut input = events();
        input[3]["usage"] = usage;
        assert_eq!(
            decode(&transcript(input)).unwrap().usage,
            TokenUsage::default()
        );
    }
    for cached in [Value::Null, json!(-1), json!("0"), json!(1.5), json!(11)] {
        let mut input = events();
        input[3]["usage"]["cached_input_tokens"] = cached;
        input[3]["usage"]["reasoning_output_tokens"] = json!(100);
        input[3]["usage"]["cache_write_input_tokens"] = json!(200);
        let usage = decode(&transcript(input)).unwrap().usage;
        assert_eq!(usage.input.unwrap().total(), 10);
        assert_eq!(usage.input.unwrap().cached(), None);
        assert_eq!(usage.output, Some(5));
    }
    let mut input = events();
    input[3]["usage"] = json!({"cached_input_tokens":4,"output_tokens":0});
    assert_eq!(
        decode(&transcript(input)).unwrap().usage,
        TokenUsage {
            input: None,
            output: Some(0)
        }
    );
}

#[test]
fn framing_metadata_and_escaped_text_preserve_exact_candidate_bytes() {
    for file in [
        "success.jsonl",
        "extra-metadata.jsonl",
        "no-final-newline.jsonl",
        "crlf.jsonl",
    ] {
        let completion = decode(&fixture(file)).unwrap();
        assert_eq!(completion.candidate.payload, payload(), "{file}");
    }
    let raw = " \r\n{\"narrative\":\"A \\\"quote\\\", \\u706f and 😀; \\\\ path\"}\t";
    let mut input = events();
    input[2]["item"]["text"] = json!(raw);
    input[2]["item"]["future_metadata"] = json!({"ignored":true});
    assert_eq!(decode(&transcript(input)).unwrap().candidate.payload, raw);
    // JSON syntax only: domain/schema validation remains the engine's job.
    for raw in ["null", "[]", "0", "\"\""] {
        let mut input = events();
        input[2]["item"]["text"] = json!(raw);
        assert_eq!(decode(&transcript(input)).unwrap().candidate.payload, raw);
    }
}

#[test]
fn first_failure_is_sticky_and_keeps_candidate_after_later_records() {
    let input = events();
    let mut protocol = Protocol::default();
    for event in &input[..3] {
        protocol
            .record(&serde_json::to_vec(event).unwrap())
            .unwrap();
    }
    let error = protocol.record(b"{broken").unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidJson);
    for event in &input {
        assert_eq!(
            protocol
                .record(&serde_json::to_vec(event).unwrap())
                .unwrap_err(),
            error
        );
    }
    let failure = protocol.finish().unwrap_err();
    assert_eq!(failure.error, error);
    assert_eq!(failure.candidate.unwrap().payload, payload());
    // A failure followed by success is never repaired by a later terminal.
    let mut input = events();
    input.insert(
        3,
        json!({"type":"turn.failed","error":{"message":"failed"}}),
    );
    assert_eq!(
        decode(&transcript(input)).unwrap_err().error.kind,
        ErrorKind::VendorFailure
    );
}

#[test]
fn frozen_live_story_captures_match_independent_payload_and_usage_expectations() {
    // Counts transcribed from the committed discovery envelopes, not derived
    // by this codec. payload.json predates this implementation.
    for (name, total, output) in [
        ("world", 14411, 827),
        ("cast", 14818, 1726),
        ("opening-ref-union", 17225, 1411),
        ("continuation-hardened", 17189, 1540),
        ("zero-npcs-hardened", 14735, 896),
        ("null-hardened", 17245, 957),
        ("empty-hardened", 17249, 756),
    ] {
        let dir = root().join("evidence").join(name);
        let completion = decode(&fs::read(dir.join("stdout.jsonl")).unwrap()).unwrap();
        assert_eq!(
            completion.candidate.payload.as_bytes(),
            fs::read(dir.join("payload.json")).unwrap(),
            "{name}"
        );
        assert_eq!(completion.usage.input.unwrap().total(), total, "{name}");
        assert_eq!(completion.usage.input.unwrap().cached(), Some(0), "{name}");
        assert_eq!(completion.usage.output, Some(output), "{name}");
    }
    // Completion has no model/cost fields: this profile observes neither.
}

#[test]
fn live_tool_canary_is_unsupported_even_though_the_cli_exited_successfully() {
    let dir = root().join("evidence/tool-hardened");
    let receipt: Value =
        serde_json::from_slice(&fs::read(dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(receipt["exit_code"], 0);
    let failure = decode(&fs::read(dir.join("stdout.jsonl")).unwrap()).unwrap_err();
    assert_eq!(failure.error.kind, ErrorKind::Unsupported);
    assert_eq!(failure.error.record, 4);
    assert!(
        failure.candidate.is_some(),
        "commentary evidence survives tool rejection"
    );
}

#[test]
fn duplicated_control_keys_reject_the_record_instead_of_picking_one() {
    let success = String::from_utf8(fixture("success.jsonl")).unwrap();
    let records: Vec<&str> = success.lines().collect();
    for (index, duplicated, location, retained) in [
        (
            0,
            r#"{"type":"thread.started","thread_id":"a","thread_id":"b"}"#,
            "$",
            false,
        ),
        (
            2,
            r#"{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"{}","text":"{\"x\":1}"}}"#,
            "$.item",
            false,
        ),
        (
            2,
            r#"{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"{}"},"item":{"id":"j","type":"agent_message","text":"{}"}}"#,
            "$",
            false,
        ),
        // The last occurrence would have read as a successful completion.
        (
            3,
            r#"{"type":"turn.failed","type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#,
            "$",
            true,
        ),
        (
            3,
            r#"{"type":"turn.completed","usage":{"output_tokens":5,"output_tokens":9}}"#,
            "$.usage",
            true,
        ),
    ] {
        let mut input = records.clone();
        input[index] = duplicated;
        let bytes = input.join("\n").into_bytes();
        let failure = decode(&bytes).unwrap_err();
        assert_eq!(
            failure.error.kind,
            ErrorKind::DuplicateField,
            "{duplicated}"
        );
        assert_eq!(failure.error.record, index + 1, "{duplicated}");
        assert_eq!(failure.error.location, location, "{duplicated}");
        assert_eq!(
            failure.candidate.map(|c| c.payload),
            retained.then(payload),
            "{duplicated}"
        );
    }
}

#[test]
fn duplicated_payload_keys_are_invalid_payload_with_the_candidate_retained() {
    let text = r#"{"title":"Harbour","title":"Elsewhere","world_description":"x"}"#;
    let mut input = events();
    input[2]["item"]["text"] = json!(text);
    let failure = decode(&transcript(input)).unwrap_err();
    assert_eq!(failure.error.kind, ErrorKind::InvalidPayload);
    assert_eq!(failure.error.location, "$.item.text");
    assert_eq!(failure.candidate.map(|c| c.payload).as_deref(), Some(text));
}

#[test]
fn duplicates_outside_control_objects_are_tolerated_for_minor_cli_evolution() {
    // Unknown metadata may change across minor CLI versions; only the control
    // objects this profile interprets (record, item, error, usage) are strict.
    let mut records: Vec<String> = events().iter().map(ToString::to_string).collect();
    records[1] = r#"{"type":"turn.started","meta":{"trace":"a","trace":"b"}}"#.into();
    records[2] = records[2].replacen(
        "\"item\":{",
        "\"extra\":{\"x\":1,\"x\":2},\"item\":{\"note\":{\"k\":1,\"k\":2},",
        1,
    );
    let bytes = records.join("\n").into_bytes();
    let completion = decode(&bytes).unwrap();
    assert_eq!(completion.candidate.payload, payload());
}
