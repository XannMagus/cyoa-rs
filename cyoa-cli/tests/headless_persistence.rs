#![cfg(target_os = "linux")]
#[path = "support/headless.rs"]
mod support;
use cyoa_application::{cancellation::CancellationSource, persistence::*};
use cyoa_infrastructure::persistence::{codec, repository::LocalRepository};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};
use support::*;
fn snapshot(source: StorySource) -> SaveSnapshot {
    let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
    let snapshot = codec::decode(
        include_bytes!("../../cyoa-infrastructure/tests/fixtures/saves/v1-minimal.json"),
        &id,
        SaveCopy::Primary,
    )
    .unwrap()
    .snapshot;
    SaveSnapshot::new(snapshot.into_game(), source).unwrap()
}
fn create(root: &Path, snapshot: SaveSnapshot) -> SaveReceipt {
    LocalRepository::new(root.into())
        .unwrap()
        .create(
            snapshot,
            &PreparedWriteEvidence::default(),
            &CancellationSource::default().token(),
        )
        .unwrap()
}
fn path(root: &Path, id: &SaveId) -> std::path::PathBuf {
    root.join("saves").join(format!("{}.json", id.as_str()))
}
fn demo(root: &Path, id: Option<&SaveId>, backup: bool) -> App {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cyoa"));
    cmd.args(["play", "--headless", "--demo", "--data-dir"])
        .arg(root)
        .env("PATH", "/absent-vendors");
    if let Some(id) = id {
        cmd.args(["--load", id.as_str(), "--limits", "original"]);
    }
    if backup {
        cmd.arg("--backup");
    }
    App::spawn(cmd, true)
}
fn query(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cyoa"))
        .args(["--data-dir"])
        .arg(root)
        .args(args)
        .env("PATH", "/absent-vendors")
        .env_remove("HOME")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
#[test]
fn auth_free_list_inspect_and_invalid_resume_leave_save_bytes_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let empty = query(root.path(), &["list"]);
    assert!(empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stdout).contains("No saves"));
    assert!(!root.path().join("saves").exists());
    let receipt = create(root.path(), snapshot(StorySource::Live));
    let before = fs::read(path(root.path(), &receipt.metadata.id)).unwrap();
    for args in [
        vec!["list", "--limit", "1"],
        vec!["inspect", receipt.metadata.id.as_str()],
    ] {
        let out = query(root.path(), &args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains(receipt.metadata.id.as_str()));
    }
    let mismatch = query(
        root.path(),
        &[
            "play",
            "--headless",
            "--demo",
            "--load",
            receipt.metadata.id.as_str(),
            "--limits",
            "original",
        ],
    );
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("source does not match"));
    assert_eq!(
        fs::read(path(root.path(), &receipt.metadata.id)).unwrap(),
        before
    );
    let out = query(
        root.path(),
        &[
            "play",
            "--headless",
            "--backend",
            "codex",
            "--executable",
            "/missing/auth-canary",
            "--load",
            "missing-0123456789abcdef0123456789abcdef",
            "--limits",
            "current",
        ],
    );
    assert!(!out.status.success());
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("Codex"),
        "invalid save must precede vendor resolution"
    );
}
#[test]
fn zero_turn_resume_waits_for_explicit_opening_and_rewind_persists_before_next_request() {
    let root = tempfile::tempdir().unwrap();
    let receipt = create(
        root.path(),
        snapshot(StorySource::Demo {
            scenario: DemoScenarioId::HarbourV1,
        }),
    );
    let mut app = demo(root.path(), Some(&receipt.metadata.id), false);
    app.wait(0, "Resumed game");
    app.command("/inspect", "Turns: 0");
    assert!(app.stdout().is_empty());
    for command in [
        "/load invalid --limits original",
        "/load",
        "/load missing-0123456789abcdef0123456789abcdef",
        "/rewind 0",
        "/rewind 184467440737095516160",
        "/save unexpected",
    ] {
        app.command(command, "Rejected:");
    }
    app.command("", "[committed turn 1]");
    app.command("/rewind 1", "turns=0 durability=Clean");
    let stored = codec::decode(
        &fs::read(path(root.path(), &receipt.metadata.id)).unwrap(),
        &receipt.metadata.id,
        SaveCopy::Primary,
    )
    .unwrap();
    assert!(stored.snapshot.game().turns().is_empty());
    app.command("", "[committed turn 1]");
    app.command("/quit", "Session closed");
    assert!(app.finish().success());
    assert_eq!(
        app.stdout()
            .matches("Opening: the lantern flickered.")
            .count(),
        2
    );
}
#[test]
fn explicit_backup_recovery_creates_a_fresh_slot_and_preserves_damaged_originals() {
    for damage in ["corrupt", "future", "missing"] {
        let root = tempfile::tempdir().unwrap();
        let receipt = create(
            root.path(),
            snapshot(StorySource::Demo {
                scenario: DemoScenarioId::HarbourV1,
            }),
        );
        let primary = path(root.path(), &receipt.metadata.id);
        let backup = primary.with_extension("json.bak");
        let bytes = fs::read(&primary).unwrap();
        fs::write(&backup, &bytes).unwrap();
        let damaged = match damage {
            "corrupt" => Some(b"damaged primary".to_vec()),
            "future" => {
                let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                document["version"] = serde_json::json!(99);
                Some(serde_json::to_vec(&document).unwrap())
            }
            _ => None,
        };
        if let Some(bytes) = &damaged {
            fs::write(&primary, bytes).unwrap();
        } else {
            fs::remove_file(&primary).unwrap();
        }
        let failed = query(
            root.path(),
            &[
                "play",
                "--headless",
                "--demo",
                "--load",
                receipt.metadata.id.as_str(),
                "--limits",
                "original",
            ],
        );
        assert!(!failed.status.success());
        assert_eq!(fs::read(&backup).unwrap(), bytes);
        let mut app = demo(root.path(), Some(&receipt.metadata.id), true);
        app.wait(0, "Recovered backup from");
        app.wait(0, "Saved:");
        app.command("/inspect", "Turns: 0");
        app.command("/quit", "Session closed");
        assert!(app.finish().success());
        assert_eq!(
            fs::read(&primary).ok(),
            damaged,
            "backup recovery must preserve {damage} primary state"
        );
        assert_eq!(fs::read(backup).unwrap(), bytes);
        let mut repo = LocalRepository::new(root.path().into()).unwrap();
        let page = repo
            .list(
                SavePage::new(None, 100).unwrap(),
                &CancellationSource::default().token(),
            )
            .unwrap();
        assert_eq!(page.entries.len(), 2);
        assert!(page.entries.iter().any(|e| e.id != receipt.metadata.id
            && matches!(e.status, SaveListingStatus::Valid { turn_count: 0, .. })));
    }
}
#[test]
fn both_fixture_backends_restart_from_disk_and_live_saves_are_vendor_neutral() {
    for claude in [false, true] {
        let fixture = Fixture::new(claude);
        let data = fixture_story();
        let mut app = begin(&fixture, &data);
        fixture.set(&data["turns"][0], 0, false, None);
        app.command("1", "[committed turn 1]");
        app.command("/quit", "Session closed");
        assert!(app.finish().success());
        let mut repo = LocalRepository::new(app._data.path().into()).unwrap();
        let page = repo
            .list(
                SavePage::new(None, 100).unwrap(),
                &CancellationSource::default().token(),
            )
            .unwrap();
        let id = page.entries[0].id.clone();
        let original = repo
            .load(
                &id,
                SaveCopy::Primary,
                &CancellationSource::default().token(),
            )
            .unwrap();
        assert_eq!(original.snapshot.source(), StorySource::Live);
        assert_eq!(original.snapshot.game().turns().len(), 1);
        // Resume with the peer backend, not the one that created the save.
        let peer = Fixture::new(!claude);
        peer.set(&data["turns"][1], 0, false, None);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cyoa"));
        cmd.args([
            "play",
            "--headless",
            "--backend",
            if peer.claude { "claude" } else { "codex" },
            "--load",
            id.as_str(),
            "--limits",
            "original",
            "--executable",
        ])
        .arg(fixture_executable())
        .arg("--home")
        .arg(peer.home.path())
        .arg("--config-dir")
        .arg(peer.home.path())
        .arg("--data-dir")
        .arg(app._data.path())
        .env("PATH", "/bin");
        let mut resumed = App::spawn(cmd, true);
        resumed.wait(0, "Resumed game");
        resumed.command("/inspect", "Turns: 1");
        assert!(resumed.stdout().is_empty());
        resumed.command("", "[committed turn 2]");
        let request = request_text(&peer.request());
        assert!(request.contains("Opening: the lantern flickered."));
        assert!(!request.contains("Before writing the opening scene, check the supplied cast"));
        assert_eq!(peer.calls(), 1);
        resumed.command("/quit", "Session closed");
        assert!(resumed.finish().success());
        let stored = repo
            .load(
                &id,
                SaveCopy::Primary,
                &CancellationSource::default().token(),
            )
            .unwrap();
        assert_eq!(stored.snapshot.game().turns().len(), 2);
        assert_eq!(
            stored.snapshot.game().turns()[0],
            original.snapshot.game().turns()[0]
        );
        assert_eq!(
            stored.snapshot.game().world(),
            original.snapshot.game().world()
        );
    }
}

#[test]
fn startup_policy_changes_persist_after_auth_and_failed_auth_never_rewrites_a_save() {
    for claude in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let receipt = create(root.path(), snapshot(StorySource::Live));
        let original = fs::read(path(root.path(), &receipt.metadata.id)).unwrap();
        let failed = Command::new(env!("CARGO_BIN_EXE_cyoa"))
            .args([
                "play",
                "--headless",
                "--backend",
                if claude { "claude" } else { "codex" },
                "--executable",
                "/missing/auth-canary",
                "--load",
                receipt.metadata.id.as_str(),
                "--limits",
                "current",
                "--data-dir",
            ])
            .arg(root.path())
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!failed.status.success());
        assert_eq!(
            fs::read(path(root.path(), &receipt.metadata.id)).unwrap(),
            original
        );
        let fixture = Fixture::new(claude);
        fixture.set(&fixture_story()["turns"][0], 0, false, None);
        for (policy, cap) in [("current", 30), ("original", 4)] {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_cyoa"));
            cmd.args([
                "play",
                "--headless",
                "--backend",
                if claude { "claude" } else { "codex" },
                "--executable",
            ])
            .arg(fixture_executable())
            .arg("--home")
            .arg(fixture.home.path())
            .arg("--config-dir")
            .arg(fixture.home.path())
            .args([
                "--load",
                receipt.metadata.id.as_str(),
                "--limits",
                policy,
                "--data-dir",
            ])
            .arg(root.path())
            .env("PATH", "/bin");
            let mut app = App::spawn(cmd, true);
            app.wait(0, "Resumed game");
            app.wait(0, "Saved:");
            app.command("/inspect", "Turns: 0");
            app.command("/quit", "Session closed");
            assert!(app.finish().success());
            assert!(app.stdout().is_empty());
            let loaded = codec::decode(
                &fs::read(path(root.path(), &receipt.metadata.id)).unwrap(),
                &receipt.metadata.id,
                SaveCopy::Primary,
            )
            .unwrap();
            assert_eq!(loaded.snapshot.game().limits().max_major_events.get(), cap);
            assert_eq!(
                loaded
                    .snapshot
                    .game()
                    .original_limits()
                    .max_major_events
                    .get(),
                4
            );
        }
        let launches = fs::read_to_string(fixture.home.path().join("launches.jsonl")).unwrap();
        assert!(
            !launches
                .lines()
                .any(|line| line.contains(if claude { "\"-p\"" } else { "\"exec\"" })),
            "zero-turn load requested inference"
        );
    }
}

#[test]
fn in_session_load_keeps_failed_edit_buffers_and_save_copy_switches_slots_only_after_success() {
    let mut app = App::demo();
    app.wait(0, "Brief:");
    app.command("A harbour", "Outline review:");
    app.command("/edit", "Title:");
    app.command(
        "/load missing-0123456789abcdef0123456789abcdef --limits original",
        "Storage failed:",
    );
    app.command("Edited after failed load", "Description:");
    let receipt = create(
        app._data.path(),
        snapshot(StorySource::Demo {
            scenario: DemoScenarioId::HarbourV1,
        }),
    );
    app.command(
        &format!("/load {} --limits original", receipt.metadata.id.as_str()),
        "Loaded:",
    );
    app.command("/inspect", "Turns: 0");
    assert!(app.stdout().is_empty());
    app.command("/save-copy", "Saved:");
    let mut reader = LocalRepository::new(app._data.path().into()).unwrap();
    let token = CancellationSource::default().token();
    let page = reader
        .list(SavePage::new(None, 100).unwrap(), &token)
        .unwrap();
    assert_eq!(page.entries.len(), 2);
    let copy_id = page
        .entries
        .iter()
        .find(|e| e.id != receipt.metadata.id)
        .unwrap()
        .id
        .clone();
    app.command("/list", receipt.metadata.id.as_str());
    app.command("/inspect", &format!("Save: {}", copy_id.as_str()));
    app.command("", "[committed turn 1]");
    app.command("/quit", "Session closed");
    assert!(app.finish().success());
    assert_eq!(
        reader
            .load(&receipt.metadata.id, SaveCopy::Primary, &token)
            .unwrap()
            .snapshot
            .game()
            .turns()
            .len(),
        0
    );
    assert_eq!(
        reader
            .load(&copy_id, SaveCopy::Primary, &token)
            .unwrap()
            .snapshot
            .game()
            .turns()
            .len(),
        1
    );
}
