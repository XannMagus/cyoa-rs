use super::*;
use serde_json::Value;
use std::{fs, path::PathBuf};

fn review() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reviews/2026-10-02-claude-step1")
}

fn json(path: PathBuf) -> Value {
    serde_json::from_slice(&fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
        .unwrap()
}

/// Splits like the supervisor: on LF only, keeping a trailing CR, delivering a
/// final record without a newline and no phantom record after a final LF.
fn records(data: &[u8]) -> Vec<&[u8]> {
    let mut parts: Vec<&[u8]> = data.split(|b| *b == b'\n').collect();
    if data.ends_with(b"\n") {
        parts.pop();
    }
    parts
}

struct Replay {
    previews: Vec<String>,
    outcome: Result<Completion, Failure>,
}

/// Feeds records until the first rejection, exactly as the supervisor stops
/// delivering after a consumer rejection, then reports end of stream.
fn replay(data: &[u8]) -> Replay {
    let mut protocol = Protocol::default();
    let mut previews = Vec::new();
    for record in records(data) {
        match protocol.record(record) {
            Ok(Some(fragment)) => previews.push(fragment),
            Ok(None) => {}
            Err(_) => break,
        }
    }
    Replay {
        previews,
        outcome: protocol.finish(),
    }
}

fn kind(failure: &Failure) -> String {
    format!("{:?}", failure.error.kind)
}

fn usage_of(done: &Completion) -> (Option<u64>, Option<u64>, Option<u64>) {
    (
        done.usage.input.map(|i| i.total()),
        done.usage.input.and_then(|i| i.cached()),
        done.usage.output,
    )
}

const INIT: &str =
    r#"{"type":"system","subtype":"init","apiKeySource":"none","model":"claude-sonnet-5-5"}"#;

fn result_with(fields: &str) -> String {
    format!(r#"{{"type":"result","subtype":"success","is_error":false,{fields}}}"#)
}

fn run(lines: &[&str]) -> Replay {
    replay(lines.join("\n").as_bytes())
}

#[test]
fn frozen_error_variants_reject_with_located_reasons_and_candidate_evidence() {
    let expectations = json(review().join("synthetic/expectations.json"));
    let payload = expectations["_payload"].as_str().unwrap();
    let mut checked = 0;
    for (name, want) in expectations["variants"].as_object().unwrap() {
        if want["outcome"] != "rejected" {
            continue;
        }
        let data = fs::read(review().join(format!("synthetic/{name}.jsonl"))).unwrap();
        let failure = replay(&data)
            .outcome
            .expect_err(&format!("{name} must not complete"));
        assert_eq!(kind(&failure), want["error"].as_str().unwrap(), "{name}");
        assert_eq!(
            failure.error.record as u64,
            want["error_record"].as_u64().unwrap(),
            "{name}"
        );
        let retained = match want["candidate"].as_str().unwrap() {
            "retained" if name == "structured-output-duplicate-keys" => {
                Some(r#"{"narrative":"a","narrative":"b"}"#)
            }
            "retained" => Some(payload),
            "none" => None,
            other => panic!("{other}"),
        };
        assert_eq!(
            failure.candidate.as_ref().map(|c| c.payload.as_str()),
            retained,
            "{name}: candidate evidence"
        );
        checked += 1;
    }
    assert_eq!(checked, 19, "every rejected variant is exercised");
}

#[test]
fn frozen_completed_variants_emit_expected_previews_payload_usage_and_provenance() {
    let expectations = json(review().join("synthetic/expectations.json"));
    let payload = expectations["_payload"].as_str().unwrap();
    let preview: Vec<&str> = expectations["_preview"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let full = &expectations["_usage_full"];
    let mut checked = 0;
    for (name, want) in expectations["variants"].as_object().unwrap() {
        if want["outcome"] != "completed" {
            continue;
        }
        let data = fs::read(review().join(format!("synthetic/{name}.jsonl"))).unwrap();
        let replay = replay(&data);
        let done = replay.outcome.unwrap_or_else(|f| panic!("{name}: {f:?}"));
        assert_eq!(
            done.candidate.payload, payload,
            "{name}: exact payload bytes"
        );
        assert_eq!(replay.previews, preview, "{name}: correlated preview only");
        assert!(
            replay.previews.iter().all(|p| !p.contains("SECOND")),
            "{name}: a later payload block never previews"
        );
        let expected = match want.get("usage") {
            Some(usage) => (
                usage["input_total"].as_u64(),
                usage["input_cached"].as_u64(),
                usage["output"].as_u64(),
            ),
            None => (
                full["input_total"].as_u64(),
                full["input_cached"].as_u64(),
                full["output"].as_u64(),
            ),
        };
        assert_eq!(usage_of(&done), expected, "{name}: usage");
        assert_eq!(
            done.provenance.model.as_ref().map(|m| m.as_str()),
            Some("claude-sonnet-5-5"),
            "{name}"
        );
        assert_eq!(
            done.provenance.provider.as_ref().map(|p| p.as_str()),
            Some("firstParty")
        );
        let cost = done.provenance.cost.as_ref().expect("list cost");
        assert_eq!(
            (cost.amount().get(), cost.currency().as_str()),
            (0.0125, "USD")
        );
        checked += 1;
    }
    assert_eq!(checked, 10, "every completed variant is exercised");
}

#[test]
fn frozen_live_captures_match_independent_payload_preview_usage_and_provenance() {
    let expectations = json(review().join("expected/live-expectations.json"));
    for (name, want) in expectations["captures"].as_object().unwrap() {
        let data = fs::read(review().join(want["file"].as_str().unwrap())).unwrap();
        let replay = replay(&data);
        let done = replay.outcome.unwrap_or_else(|f| panic!("{name}: {f:?}"));
        let payload =
            fs::read_to_string(review().join(format!("expected/{name}.payload.json"))).unwrap();
        let preview =
            fs::read_to_string(review().join(format!("expected/{name}.preview.txt"))).unwrap();
        assert_eq!(done.candidate.payload, payload, "{name}: exact span");
        assert_eq!(replay.previews.concat(), preview, "{name}: preview text");
        assert!(replay.previews.iter().all(|p| !p.is_empty()), "{name}");
        assert_ne!(
            replay.previews.concat(),
            done.candidate.payload,
            "{name}: previews differ textually from the CLI's compact serialization"
        );
        assert_eq!(
            usage_of(&done),
            (
                want["input_total"].as_u64(),
                want["input_cached"].as_u64(),
                want["output"].as_u64()
            ),
            "{name}: usage"
        );
        assert_eq!(
            done.provenance.model.as_ref().map(|m| m.as_str()),
            want["model"].as_str(),
            "{name}: observed model"
        );
        let cost = done
            .provenance
            .cost
            .as_ref()
            .expect("list cost")
            .amount()
            .get();
        assert!(
            (cost - want["list_cost_usd"].as_f64().unwrap()).abs() < 1e-9,
            "{name}: {cost}"
        );
    }
}

#[test]
fn live_failure_captures_are_rejected_at_the_expected_record() {
    let expectations = json(review().join("expected/live-expectations.json"));
    for (name, want) in expectations["failures"].as_object().unwrap() {
        let data = fs::read(review().join(want["file"].as_str().unwrap())).unwrap();
        let failure = replay(&data).outcome.expect_err(name);
        assert_eq!(kind(&failure), want["error"].as_str().unwrap(), "{name}");
        assert_eq!(
            failure.error.record as u64,
            want["error_record"].as_u64().unwrap(),
            "{name}"
        );
        assert!(failure.candidate.is_none(), "{name}");
    }
}

#[test]
fn result_payload_is_the_exact_span_not_a_reserialization() {
    // Odd spacing, key order and an escape the compact serializer would rewrite.
    let span = r#"{"b" : 1,  "a":"é",  "c":[1 , 2]}"#;
    let result = result_with(&format!(r#""structured_output":{span}"#));
    let done = run(&[INIT, &result]).outcome.unwrap();
    assert_eq!(done.candidate.payload, span);
}

#[test]
fn preview_is_forwarded_at_the_delta_record_and_never_waits_for_the_result() {
    let mut protocol = Protocol::default();
    for line in [
        INIT,
        r#"{"type":"stream_event","event":{"type":"message_start"}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","name":"StructuredOutput"}}}"#,
    ] {
        assert_eq!(protocol.record(line.as_bytes()).unwrap(), None);
    }
    let delta = |text: &str| {
        format!(
            r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"input_json_delta","partial_json":{}}}}}}}"#,
            serde_json::to_string(text).unwrap()
        )
    };
    assert_eq!(
        protocol.record(delta("").as_bytes()).unwrap(),
        None,
        "empty fragments are not forwarded"
    );
    assert_eq!(
        protocol
            .record(delta("{\"narr").as_bytes())
            .unwrap()
            .as_deref(),
        Some("{\"narr")
    );
    assert!(protocol.finish().is_err(), "no result yet means incomplete");
}

#[test]
fn first_failure_is_sticky_and_later_records_cannot_repair_it() {
    let data = fs::read(review().join("synthetic/is-error-true-with-payload.jsonl")).unwrap();
    let mut protocol = Protocol::default();
    let mut first = None;
    for record in records(&data) {
        if let Err(error) = protocol.record(record) {
            first = Some(error);
            break;
        }
    }
    let first = first.expect("the is_error result is rejected");
    let ok = fs::read(review().join("synthetic/success-minimal.jsonl")).unwrap();
    for record in records(&ok) {
        assert_eq!(protocol.record(record).unwrap_err(), first, "latched");
    }
    let failure = protocol.finish().unwrap_err();
    assert_eq!(failure.error, first);
    assert_eq!(
        failure.candidate.map(|c| c.payload).as_deref(),
        Some(r#"{"narrative":"A \"quoted\" é雪","n":1}"#)
    );
}

#[test]
fn ping_is_tolerated_and_a_stream_error_event_is_a_vendor_error() {
    // UNVERIFIED: `ping` and `error` stream events come from Anthropic's published
    // Messages streaming documentation; neither occurred in any live capture.
    let message = r#"{"type":"stream_event","event":{"type":"message_start"}}"#;
    let ping = r#"{"type":"stream_event","event":{"type":"ping"}}"#;
    let error =
        r#"{"type":"stream_event","event":{"type":"error","error":{"type":"overloaded_error"}}}"#;
    let result = result_with(r#""structured_output":{"a":1}"#);
    assert!(run(&[INIT, message, ping, &result]).outcome.is_ok());
    let failure = run(&[INIT, message, error, &result]).outcome.unwrap_err();
    assert_eq!(
        (kind(&failure).as_str(), failure.error.record),
        ("VendorError", 3)
    );
}

#[test]
fn usage_and_provenance_distinguish_unknown_from_zero_and_never_invent_cost() {
    let zero = r#""usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0}"#;
    let list = r#""modelUsage":{"claude-sonnet-5-5":{"costBasis":"list","provider":"firstParty"}}"#;
    let done = run(&[
        INIT,
        &result_with(&format!(
            r#""structured_output":{{}},{zero},{list},"total_cost_usd":0"#
        )),
    ])
    .outcome
    .unwrap();
    assert_eq!(
        usage_of(&done),
        (Some(0), Some(0), Some(0)),
        "reported zero is not unknown"
    );
    assert_eq!(done.provenance.cost.unwrap().amount().get(), 0.0);

    for (label, extra) in [
        ("no usage, no cost data", String::new()),
        ("non-list basis", r#""total_cost_usd":0.5,"modelUsage":{"claude-sonnet-5-5":{"costBasis":"billed"}}"#.into()),
        ("negative cost", format!(r#""total_cost_usd":-1,{list}"#)),
        ("cost without basis evidence", r#""total_cost_usd":0.5"#.into()),
        ("one non-list entry among list entries", r#""total_cost_usd":0.5,"modelUsage":{"claude-sonnet-5-5":{"costBasis":"list"},"other":{"costBasis":"api"}}"#.into()),
    ] {
        let separator = if extra.is_empty() { "" } else { "," };
        let result = result_with(&format!(r#""structured_output":{{}}{separator}{extra}"#));
        let done = run(&[INIT, &result]).outcome.unwrap();
        assert!(done.provenance.cost.is_none(), "{label}");
        assert_eq!(usage_of(&done), (None, None, None), "{label}");
    }
    // Requested model is not evidence of the observed one: the model comes from init only.
    let done = run(&[
        r#"{"type":"system","subtype":"init","apiKeySource":"none"}"#,
        &result_with(r#""structured_output":{}"#),
    ])
    .outcome
    .unwrap();
    assert!(done.provenance.model.is_none() && done.provenance.provider.is_none());
}

#[test]
fn malformed_required_fields_and_unsupported_records_are_rejected_not_ignored() {
    let cases: [(&str, &[&str], &str, usize); 7] = [
        (
            "missing type",
            &[r#"{"subtype":"init"}"#],
            "InvalidField",
            1,
        ),
        (
            "system without subtype",
            &[r#"{"type":"system"}"#],
            "InvalidField",
            1,
        ),
        (
            "init without apiKeySource",
            &[r#"{"type":"system","subtype":"init"}"#],
            "InvalidField",
            1,
        ),
        ("second init", &[INIT, INIT], "Order", 2),
        (
            "is_error not a bool",
            &[
                INIT,
                r#"{"type":"result","subtype":"success","is_error":"no"}"#,
            ],
            "InvalidField",
            2,
        ),
        (
            "result without subtype",
            &[INIT, r#"{"type":"result","is_error":false}"#],
            "InvalidField",
            2,
        ),
        (
            "stream_event without event",
            &[INIT, r#"{"type":"stream_event"}"#],
            "InvalidField",
            2,
        ),
    ];
    for (label, lines, expected, record) in cases {
        let failure = run(lines).outcome.unwrap_err();
        assert_eq!(
            (kind(&failure).as_str(), failure.error.record),
            (expected, record),
            "{label}"
        );
    }
    let unsupported = run(&[
        INIT,
        r#"{"type":"stream_event","event":{"type":"surprise"}}"#,
    ])
    .outcome
    .unwrap_err();
    assert_eq!(kind(&unsupported), "Unsupported");
}
