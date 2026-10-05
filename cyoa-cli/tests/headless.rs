#![cfg(target_os = "linux")]
use cyoa_application::persistence::GameRepository;
use std::process::{Command, Stdio};

#[test]
fn chosen_backend_is_required_and_demo_never_resolves_a_vendor() {
    let data = tempfile::tempdir().unwrap();
    for args in [
        vec!["play", "--headless"],
        vec!["play", "--headless", "--demo", "--backend", "codex"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_cyoa"))
            .args(args)
            .arg("--data-dir")
            .arg(data.path())
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_cyoa"))
        .args(["play", "--headless", "--demo"])
        .arg("--data-dir")
        .arg(data.path())
        .env("PATH", "/definitely-no-vendor-here")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Autosave enabled"));
    assert!(
        !data.path().join("saves").exists(),
        "unselected draft must not save"
    );
}

use serde_json::json;
use std::{
    io::Write,
    thread,
    time::{Duration, Instant},
};

#[path = "support/headless.rs"]
mod support;
use support::*;

fn saved_canonical(app: &App) -> cyoa_application::persistence::StoredGame {
    use cyoa_application::{cancellation::CancellationSource, persistence::*};
    let mut repo =
        cyoa_infrastructure::persistence::repository::LocalRepository::new(app._data.path().into())
            .unwrap();
    let token = CancellationSource::default().token();
    let page = repo
        .list(SavePage::new(None, PageSize::new(100).unwrap()), &token)
        .unwrap();
    assert_eq!(
        page.entries.len(),
        1,
        "shutdown must preserve one canonical story slot"
    );
    let stored = repo
        .load(&page.entries[0].id, SaveCopy::Primary, &token)
        .unwrap();
    assert_eq!(stored.snapshot.source(), StorySource::Live);
    stored
}

#[test]
fn both_shipped_backend_commands_play_retry_and_inspect_canonical_story() {
    for claude in [false, true] {
        let fixture = Fixture::new(claude);
        let data = fixture_story();
        let mut app = begin(&fixture, &data);
        fixture.set(&data["turns"][0], 0, false, None);
        app.command("1", "[committed turn 1]");
        assert_eq!(fixture.calls(), 3, "opening must be exactly one call");
        let opening = request_text(&fixture.request());
        assert_eq!(
            opening
                .matches("Before writing the opening scene, check the supplied cast")
                .count(),
            1,
            "opening identity review regression"
        );
        app.command("/inspect", "Upcoming event: \"Depart\"");
        app.command("/unsupported", "Rejected:");
        app.command("/action 0", "Rejected:");
        app.command("/retry", "Rejected:");
        fixture.set(&data["turns"][1], 7, false, None);
        app.command("/action 1", "Generation failed:");
        let failed_prompt = request_text(&fixture.request());
        assert!(failed_prompt.contains("Go"));
        app.command("/inspect", "Turns: 1");
        app.command("/diagnostics", "Candidate:");
        fixture.set(&data["turns"][1], 0, false, None);
        app.command("/retry", "[committed turn 2]");
        assert_eq!(
            request_text(&fixture.request()),
            failed_prompt,
            "retry must repeat unchanged intent"
        );
        app.command("/inspect", "Chapter: 0 title=Some(\"Retitled\")");
        assert!(
            app.stderr()
                .contains("Identity: ajax name=\"Ajax\" state=Some(\"At the shop\")")
        );
        assert!(
            app.stderr()
                .contains("Identity: ajax-2 name=\"Ajax\" state=Some(\"At the gate\")")
        );
        fixture.set(&data["turns"][2], 0, false, None);
        app.command("2", "[committed turn 3]");
        assert!(request_text(&fixture.request()).contains("2"));
        app.command("/inspect", "Chapter: 1 title=Some(\"Voyage\")");
        fixture.set(&data["turns"][3], 0, false, None);
        app.command("//follow ‘雨’", "[committed turn 4]");
        assert!(request_text(&fixture.request()).contains("/follow ‘雨’"));
        fixture.set(&data["turns"][4], 0, false, None);
        app.command("", "[committed turn 5]");
        app.command("/inspect", "Chapter: 1 title=Some(\"At Sea\")");
        app.command("/quit", "Session closed");
        assert!(app.finish().success());
        let prose = app.stdout();
        for turn in data["turns"].as_array().unwrap() {
            assert_eq!(
                prose.matches(turn["narrative"].as_str().unwrap()).count(),
                1,
                "matching prose duplicated"
            );
        }
        assert_eq!(fixture.calls(), 8, "unexpected extra calls");
        let mut saves = cyoa_infrastructure::persistence::repository::LocalRepository::new(
            app._data.path().into(),
        )
        .unwrap();
        let token = cyoa_application::cancellation::CancellationSource::default().token();
        let page = saves
            .list(
                cyoa_application::persistence::SavePage::new(
                    None,
                    cyoa_application::persistence::PageSize::new(100).unwrap(),
                ),
                &token,
            )
            .unwrap();
        assert_eq!(
            page.entries.len(),
            1,
            "one story slot across five autosaves"
        );
        let stored = saves
            .load(
                &page.entries[0].id,
                cyoa_application::persistence::SaveCopy::Primary,
                &token,
            )
            .unwrap();
        assert_eq!(stored.snapshot.game().turns().len(), 5);
        for (record, expected) in stored
            .snapshot
            .game()
            .turns()
            .iter()
            .zip(data["turns"].as_array().unwrap())
        {
            assert_eq!(
                record.turn().narrative().as_str(),
                expected["narrative"].as_str().unwrap()
            );
            assert_eq!(
                record.raw_response().as_str(),
                expected.to_string(),
                "exact generated audit payload survives all autosaves"
            );
        }
        assert_eq!(
            stored
                .snapshot
                .game()
                .current_chapter()
                .unwrap()
                .title()
                .unwrap()
                .as_str(),
            "At Sea"
        );
    }
}
#[test]
fn malformed_turn_keeps_state_and_raw_candidate_until_explicit_retry() {
    for claude in [false, true] {
        let fixture = Fixture::new(claude);
        let data = fixture_story();
        let mut app = begin(&fixture, &data);
        fixture.set(
            &json!({"narrative":"broken 雨","quick_actions":[]}),
            0,
            false,
            None,
        );
        app.command("1", "Generation failed:");
        app.command("/inspect", "Turns: 0");
        app.command("/diagnostics", "broken 雨");
        fixture.set(&data["turns"][0], 0, false, None);
        app.command("/retry", "[committed turn 1]");
        app.command("/quit", "Session closed");
        assert!(app.finish().success());
        assert_eq!(fixture.calls(), 4);
    }
}
#[test]
fn idle_output_cancellation_eof_and_quit_join_real_children() {
    for claude in [false, true] {
        for ending in ["cancel", "eof", "quit"] {
            let fixture = Fixture::new(claude);
            let data = fixture_story();
            let mut app = begin(&fixture, &data);
            fixture.set(&data["turns"][0], 0, true, None);
            app.line("1");
            let report = fixture.request();
            let mark = app.mark();
            if ending == "cancel" {
                app.signal();
                app.line("/cancel");
                app.wait(mark, "Generation cancelled after cleanup");
                assert_gone(&report);
                app.command("/inspect", "Turns: 0");
                assert_eq!(fixture.calls(), 3, "cancellation silently retried");
                fixture.set(&data["turns"][0], 0, false, None);
                app.command("/retry", "[committed turn 1]");
                app.command("/quit", "Session closed");
            } else if ending == "eof" {
                app.stdin.take();
            } else {
                app.line("/quit");
            }
            assert!(app.finish().success(), "idle input cancellation regression");
            assert_gone(&report);
            let saved = saved_canonical(&app);
            assert_eq!(
                saved.snapshot.game().turns().len(),
                usize::from(ending == "cancel"),
                "cancelled candidate must never reach the final save"
            );
            if ending == "cancel" {
                assert_eq!(
                    saved.snapshot.game().turns()[0].raw_response().as_str(),
                    data["turns"][0].to_string()
                );
            }
        }
    }
}
#[test]
fn preview_failure_and_disagreement_never_masquerade_as_committed_prose() {
    let fixture = Fixture::new(true);
    let data = fixture_story();
    let mut app = begin(&fixture, &data);
    fixture.set(
        &data["turns"][0],
        9,
        false,
        Some("Transient 雨 that fails."),
    );
    app.command("1", "Generation failed:");
    assert!(app.stdout().contains("Transient 雨 that fails."));
    assert!(
        app.stderr()
            .contains("[tentative preview discarded; no turn committed]")
    );
    app.command("/inspect", "Turns: 0");
    fixture.set(&data["turns"][0], 0, false, Some("Wrong opening."));
    app.command("/retry", "[committed turn 1]");
    assert!(
        app.stderr()
            .contains("[authoritative final replaces the tentative preview]")
    );
    assert_eq!(
        app.stdout()
            .matches("Opening: the lantern flickered.")
            .count(),
        1
    );
    fixture.set(&data["turns"][1], 0, false, Some("Second: the bell rang."));
    app.command("", "[committed turn 2]");
    assert_eq!(
        app.stdout().matches("Second: the bell rang.").count(),
        1,
        "preview duplication regression"
    );
    app.command("/quit", "Session closed");
    assert!(app.finish().success());
}
#[test]
fn broken_stdout_closes_worker_and_reports_io_failure() {
    for claude in [false, true] {
        let fixture = Fixture::new(claude);
        let data = fixture_story();
        fixture.set(&data["outline"], 0, false, None);
        let mut app = fixture.spawn(false);
        app.wait(0, "Brief:");
        app.command("harbour", "Outline review:");
        fixture.set(&data["cast"], 0, false, None);
        app.command("", "Character selection:");
        fixture.set(
            &data["turns"][0],
            0,
            false,
            if claude {
                Some("Streaming preview.")
            } else {
                None
            },
        );
        app.line("1");
        let report = fixture.request();
        assert!(!app.finish().success());
        assert!(app.stderr().contains("Broken pipe"));
        assert_gone(&report);
        let saved = saved_canonical(&app);
        assert!(saved.snapshot.game().turns().len() <= 1);
        if let Some(turn) = saved.snapshot.game().turns().first() {
            assert_eq!(
                turn.raw_response().as_str(),
                data["turns"][0].to_string(),
                "output failure must save only the authoritative accepted payload"
            );
        }
    }
}
#[test]
fn demo_full_story_exhaustion_and_idle_interrupt_are_credential_free() {
    let mut app = App::demo();
    app.wait(0, "Brief:");
    app.command("", "Rejected:");
    app.command("/retry", "Rejected:");
    app.command("A harbour", "Outline review:");
    app.command("", "Character selection:");
    app.command("wrong", "Rejected:");
    app.command("1", "[committed turn 1]");
    for n in 2..=5 {
        app.command("", &format!("[committed turn {n}]"));
    }
    app.command("", "script exhausted");
    app.command("/inspect", "Turns: 5");
    app.signal();
    assert!(app.finish().success());
    let mut app = App::demo();
    app.wait(0, "Brief:");
    app.signal();
    assert!(app.finish().success());
}
#[test]
fn missing_chosen_executable_does_not_probe_other_backend() {
    let data = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cyoa"))
        .args([
            "play",
            "--headless",
            "--backend",
            "codex",
            "--executable",
            "/missing/codex",
        ])
        .arg("--data-dir")
        .arg(data.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Codex"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Claude"));
}

#[test]
fn sigint_alone_cancels_idle_generation_before_any_fallback_command() {
    for claude in [false, true] {
        let fixture = Fixture::new(claude);
        let data = fixture_story();
        let mut app = begin(&fixture, &data);
        fixture.set(&data["turns"][0], 0, true, None);
        app.line("1");
        let report = fixture.request();
        app.signal();
        let deadline = Instant::now() + Duration::from_secs(10);
        let cancelled = loop {
            app.drain();
            if app.stderr().contains("Generation cancelled after cleanup") {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            thread::sleep(Duration::from_millis(5));
        };
        // A mutant must fail a bounded behavioral assertion, with cleanup, rather
        // than relying on the test watchdog or leaving a sleeping fixture behind.
        app.command("/quit", "Session closed");
        assert!(app.finish().success());
        assert_gone(&report);
        assert!(cancelled, "idle SIGINT cancellation regression");
    }
}
#[test]
fn invalid_utf8_input_closes_active_worker_without_committing() {
    let fixture = Fixture::new(false);
    let data = fixture_story();
    let mut app = begin(&fixture, &data);
    fixture.set(&data["turns"][0], 0, true, None);
    app.line("1");
    let report = fixture.request();
    app.stdin
        .as_mut()
        .unwrap()
        .write_all(&[255, b'\n'])
        .unwrap();
    assert!(!app.finish().success());
    assert!(app.stderr().contains("invalid utf-8"));
    assert_gone(&report);
    assert!(!app.stderr().contains("[committed turn"));
    assert!(
        saved_canonical(&app).snapshot.game().turns().is_empty(),
        "input failure must retain the selected zero-turn save after joining generation"
    );
}
#[test]
fn quotes_unicode_multiline_narrative_and_numeric_player_intent_survive_binary_io() {
    for claude in [false, true] {
        let fixture = Fixture::new(claude);
        let mut data = fixture_story();
        let narrative = "Opening: \"雨\" and \\ sails.\n\nAnother paragraph.";
        data["turns"][0]["narrative"] = json!(narrative);
        let mut app = begin(&fixture, &data);
        fixture.set(&data["turns"][0], 0, false, None);
        app.command("1", "[committed turn 1]");
        fixture.set(&data["turns"][1], 0, false, None);
        app.command("2", "[committed turn 2]");
        let request = request_text(&fixture.request());
        assert!(
            request.contains("The reader directs: 2"),
            "numeric prose became action selection"
        );
        app.command("/quit", "Session closed");
        assert!(app.finish().success());
        assert_eq!(app.stdout().matches(narrative).count(), 1);
    }
}
#[test]
fn buffered_multiple_lines_are_read_without_waiting_for_more_kernel_bytes() {
    let mut app = App::demo();
    app.wait(0, "Brief:");
    let mark = app.mark();
    // Larger than one read chunk: exercises buffered lines with no later write.
    let commands = " ".repeat(2048) + "/help\n/inspect\n/quit\n";
    app.stdin
        .as_mut()
        .unwrap()
        .write_all(commands.as_bytes())
        .unwrap();
    app.wait(mark, "Session closed");
    assert!(app.finish().success());
    let control = String::from_utf8_lossy(&app.err[mark..]);
    assert_eq!(control.matches("Commands: /help").count(), 1);
    assert!(control.contains("Inspection: phase=Ready revision=0"));
}
#[test]
fn command_surface_rejects_demo_vendor_settings_and_unimplemented_ui() {
    let data = tempfile::tempdir().unwrap();
    for args in [
        vec!["play", "--demo"],
        vec!["play", "--headless", "--demo", "--model", "sonnet"],
        vec!["play", "--headless", "--demo", "--executable", "/bin/false"],
        vec!["play", "--headless", "--backend", "other"],
    ] {
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_cyoa"))
                .args(args)
                .arg("--data-dir")
                .arg(data.path())
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[test]
fn input_line_bound_ignores_read_ahead_and_rejects_oversized_lines() {
    // Regular files make read-ahead deterministic, including trailing commands.
    for (length, newline, valid) in [
        (65_530, true, true),
        (65_535, true, true),
        (65_536, true, false),
        (65_536, false, true),
        (65_537, false, false),
    ] {
        let data = tempfile::tempdir().unwrap();
        let input = data.path().join("input");
        let mut bytes = b"/help\n".to_vec();
        // A padded command avoids generation while exercising the byte limit.
        bytes.extend_from_slice(b"/help");
        bytes.resize(bytes.len() + length - 5, b' ');
        if newline {
            bytes.extend_from_slice(b"\n/inspect\n/quit\n");
        }
        std::fs::write(&input, bytes).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_cyoa"))
            .args(["play", "--headless", "--demo"])
            .arg("--data-dir")
            .arg(data.path())
            .stdin(std::fs::File::open(input).unwrap())
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.success(),
            valid,
            "input line bound must ignore read-ahead: length={length} newline={newline}: {stderr}"
        );
        if valid {
            assert_eq!(stderr.matches("Commands: /help").count(), 2);
            assert!(stderr.contains("Session closed"));
            if newline {
                assert!(stderr.contains("Inspection: phase=Ready revision=0"));
            }
        } else {
            assert!(stderr.contains("input line exceeds 64 KiB"));
            assert_eq!(stderr.matches("Commands: /help").count(), 1);
        }
    }
}

#[test]
fn full_undrained_stderr_does_not_block_error_exit() {
    let data = tempfile::tempdir().unwrap();
    use rustix::{
        fs::{OFlags, fcntl_getfl, fcntl_setfl},
        pipe::{PipeFlags, pipe_with},
    };
    let (reader, writer) = pipe_with(PipeFlags::NONBLOCK).unwrap();
    let bytes = [b'x'; 1024];
    let mut filled = 0;
    loop {
        match rustix::io::write(&writer, &bytes) {
            Ok(n) => filled += n,
            Err(rustix::io::Errno::AGAIN) => break,
            result => panic!("could not fill controlled stderr pipe: {result:?}"),
        }
    }
    assert!(filled > 0);
    // The child inherits a blocking descriptor, just like ordinary piped stderr.
    // Keep the reader open and never drain it until the process has exited.
    let flags = fcntl_getfl(&writer).unwrap();
    fcntl_setfl(&writer, flags & !OFlags::NONBLOCK).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_cyoa"))
        .args(["play", "--headless", "--demo"])
        .arg("--data-dir")
        .arg(data.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(writer))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        thread::sleep(Duration::from_millis(5));
    };
    // A mutant gets a bounded assertion after explicit cleanup, not a watchdog.
    if status.is_none() {
        child.kill().unwrap();
        child.wait().unwrap();
    }
    drop(reader);
    assert!(
        status.is_some(),
        "stderr shutdown regression: error reporting blocked on a full pipe"
    );
    assert_eq!(status.unwrap().code(), Some(1));
}
