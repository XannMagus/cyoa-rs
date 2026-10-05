#![cfg(target_os = "linux")]
#[path = "support/headless.rs"]
mod support;
use cyoa_application::{cancellation::CancellationSource, persistence::*};
use cyoa_infrastructure::persistence::{codec, repository::LocalRepository};
use std::{
    fs,
    process::{Command, Stdio},
};
use support::*;
#[test]
fn harbour_restart_rewind_and_exhaustion_preserve_fixed_passages_without_vendor_access() {
    let mut app = App::demo();
    app.wait(0, "Brief:");
    app.command("A harbour", "Outline review:");
    app.command("", "Character selection:");
    app.command("1", "[committed turn 1]");
    app.command("", "[committed turn 2]");
    app.command("/quit", "Session closed");
    assert!(app.finish().success());
    let mut repo = LocalRepository::new(app._data.path().into()).unwrap();
    let id = repo
        .list(
            SavePage::new(None, PageSize::new(100).unwrap()),
            &CancellationSource::default().token(),
        )
        .unwrap()
        .entries[0]
        .id
        .clone();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cyoa"));
    cmd.args([
        "play",
        "--headless",
        "--demo",
        "--load",
        id.as_str(),
        "--limits",
        "original",
        "--data-dir",
    ])
    .arg(app._data.path())
    .env("PATH", "/no-vendors");
    let mut resumed = App::spawn(cmd, true);
    resumed.wait(0, "Resumed game");
    resumed.command("/inspect", "Turns: 2");
    assert!(resumed.stdout().is_empty());
    resumed.command("", "[committed turn 3]");
    resumed.command("/rewind 1", "turns=2 durability=Clean");
    resumed.command("", "[committed turn 3]");
    resumed.command("", "[committed turn 4]");
    resumed.command("", "[committed turn 5]");
    resumed.command("", "script exhausted");
    resumed.command("/retry", "script exhausted");
    resumed.command("/inspect", "Turns: 5");
    resumed.command("/quit", "Session closed");
    assert!(resumed.finish().success());
    let prose = resumed.stdout();
    assert!(!prose.contains("Opening: the lantern flickered."));
    assert_eq!(prose.matches("Voyage: the ship sailed.").count(), 2);
    assert_eq!(prose.matches("Fourth: the waves rose.").count(), 1);
    assert_eq!(prose.matches("Rewritten: the ship stayed.").count(), 1);
    let stored = repo
        .load(
            &id,
            SaveCopy::Primary,
            &CancellationSource::default().token(),
        )
        .unwrap();
    assert_eq!(stored.snapshot.game().turns().len(), 5);
    assert_eq!(
        stored.snapshot.source(),
        StorySource::Demo {
            scenario: DemoScenarioId::HarbourV1
        }
    );
}
#[test]
fn unknown_scenario_future_version_source_mismatch_and_excess_demo_turns_fail_before_auth() {
    let root = tempfile::tempdir().unwrap();
    let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
    let original: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../cyoa-infrastructure/tests/fixtures/saves/v1-full-story.json"
    ))
    .unwrap();
    let mut repo = LocalRepository::new(root.path().into()).unwrap();
    let original_snapshot = codec::decode(
        &serde_json::to_vec(&original).unwrap(),
        &id,
        SaveCopy::Primary,
    )
    .unwrap()
    .snapshot;
    let receipt = repo
        .create(
            original_snapshot,
            &PreparedWriteEvidence::default(),
            &CancellationSource::default().token(),
        )
        .unwrap();
    let path = root
        .path()
        .join("saves")
        .join(format!("{}.json", receipt.metadata.id.as_str()));
    let base: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let mut variants = vec![];
    let mut unknown = base.clone();
    unknown["source"] = serde_json::json!({"kind":"demo","scenario":"unknown-v2"});
    variants.push(unknown);
    let mut future = base.clone();
    future["version"] = serde_json::json!(2);
    variants.push(future);
    let mut mismatch = base.clone();
    mismatch["source"] = serde_json::json!({"kind":"live"});
    variants.push(mismatch);
    let mut excess = base.clone();
    excess["source"] = serde_json::json!({"kind":"demo","scenario":"harbour-v1"});
    let first = excess["game"]["turns"][0].clone();
    while excess["game"]["turns"].as_array().unwrap().len() < 6 {
        excess["game"]["turns"]
            .as_array_mut()
            .unwrap()
            .push(first.clone());
    }
    excess["turn_count"] = serde_json::json!(6);
    variants.push(excess);
    for document in variants {
        let bytes = serde_json::to_vec(&document).unwrap();
        fs::write(&path, &bytes).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_cyoa"))
            .args([
                "play",
                "--headless",
                "--demo",
                "--load",
                receipt.metadata.id.as_str(),
                "--limits",
                "original",
                "--data-dir",
            ])
            .arg(root.path())
            .env("PATH", "/absent-vendors")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success(), "unsupported save admitted");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(String::from_utf8_lossy(&output.stdout).is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Autosave enabled"));
    }
}
