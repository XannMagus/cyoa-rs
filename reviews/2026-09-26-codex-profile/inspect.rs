//! Offline boundary inspection of recorded live payloads, not a vendor codec.
use cyoa_core::{
    limits::Limits,
    turn::StoryTurn,
    world::{WorldCast, WorldOutline},
};
use cyoa_infrastructure::generation::wire::{
    GeneratedCastWire, StoryTurnWire, WorldOutlineWire, playable_and_npcs_from_wire,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("review directory"));
    let mut reports = vec![];
    for (label, kind) in [
        ("world", "world"),
        ("cast", "cast"),
        ("opening-ref-union", "turn"),
        ("continuation-hardened", "turn"),
        ("zero-npcs-hardened", "zero"),
        ("null-hardened", "null"),
        ("empty-hardened", "empty"),
    ] {
        let dir = root.join("evidence").join(label);
        let result: Value =
            serde_json::from_slice(&fs::read(dir.join("result.json")).unwrap()).unwrap();
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["workspace_removed"], true);
        let raw = fs::read_to_string(dir.join("stdout.jsonl")).unwrap();
        let events: Vec<Value> = raw
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        // Explicit observed sequence, not a proposed general parser implementation.
        assert_eq!(
            events
                .iter()
                .map(|e| e["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "thread.started",
                "turn.started",
                "item.completed",
                "turn.completed"
            ]
        );
        assert_eq!(events[2]["item"]["type"], "agent_message");
        let payload = events[2]["item"]["text"].as_str().unwrap();
        let value: Value = serde_json::from_str(payload).unwrap();
        match kind {
            "world" => {
                let _: WorldOutline = serde_json::from_str::<WorldOutlineWire>(payload)
                    .unwrap()
                    .try_into()
                    .unwrap();
            }
            "cast" | "zero" => {
                let limits = Limits::default();
                let (players, npcs) = playable_and_npcs_from_wire(
                    serde_json::from_str::<GeneratedCastWire>(payload).unwrap(),
                );
                if kind == "zero" {
                    assert!(npcs.is_empty());
                    assert_eq!(value["npcs"], json!([]));
                }
                WorldCast::new(players, npcs, &limits).unwrap();
            }
            _ => {
                let _: StoryTurn = serde_json::from_str::<StoryTurnWire>(payload)
                    .unwrap()
                    .try_into()
                    .unwrap();
                if kind == "null" {
                    assert_eq!(value["chapter_title"], Value::Null);
                    assert_eq!(value["summary_update"]["upcoming_events"], Value::Null);
                }
                if kind == "empty" {
                    assert_eq!(value["chapter_title"], Value::Null);
                    assert_eq!(value["summary_update"]["upcoming_events"], json!([]));
                }
            }
        }
        fs::write(dir.join("payload.json"), payload.as_bytes()).unwrap();
        reports.push(json!({"label":label, "wire_and_domain_boundary":"accepted", "payload_bytes":payload.len(), "usage":events[3]["usage"], "narrative_whitespace_words":value["narrative"].as_str().map(|s| s.split_whitespace().count()), "observed_model":null, "application_or_adapter_acceptance": "not exercised"}));
    }
    fs::write(
        root.join("boundary-inspection.json"),
        serde_json::to_vec_pretty(&reports).unwrap(),
    )
    .unwrap();
    println!(
        "Seven recorded payloads passed existing wire/domain boundaries; no adapter or live story lifecycle claimed."
    );
}
