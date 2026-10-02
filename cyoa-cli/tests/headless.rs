#![cfg(target_os = "linux")]
use std::process::{Command, Stdio};

#[test]
fn chosen_backend_is_required_and_demo_never_resolves_a_vendor() {
    for args in [
        vec!["play", "--headless"],
        vec!["play", "--headless", "--demo", "--backend", "codex"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_cyoa"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_cyoa"))
        .args(["play", "--headless", "--demo"])
        .env("PATH", "/definitely-no-vendor-here")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("memory only"));
}

use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ExitStatus},
    sync::{OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

fn fixture_executable() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        escargot::CargoBuild::new()
            .manifest_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml"))
            .package("cyoa-infrastructure")
            .bin("subprocess_fixture")
            .args(["--locked", "--offline"])
            .run()
            .unwrap()
            .path()
            .to_path_buf()
    })
}
fn fixture_story() -> Value {
    serde_json::from_str(include_str!(
        "../../cyoa-infrastructure/tests/fixtures/phase0_story.json"
    ))
    .unwrap()
}
struct Fixture {
    home: tempfile::TempDir,
    claude: bool,
}
impl Fixture {
    fn new(claude: bool) -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            claude,
        }
    }
    fn report_path(&self) -> PathBuf {
        self.home.path().join("report.json")
    }
    fn set(&self, payload: &Value, exit: i32, hang: bool, preview: Option<&str>) {
        let lines = if self.claude {
            let mut lines = vec![
                json!({"type":"system","subtype":"init","apiKeySource":"none","model":"fixture"}),
            ];
            if let Some(preview) = preview {
                lines.extend([
                    json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"m","role":"assistant"}}}),
                    json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool","name":"StructuredOutput","input":{}}}}),
                    json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":format!("{{\"narrative\":{}", serde_json::to_string(preview).unwrap())}}}),
                    json!({"type":"stream_event","event":{"type":"content_block_stop","index":0}}),
                    json!({"type":"stream_event","event":{"type":"message_stop"}}),
                ]);
            }
            lines.push(json!({"type":"result","subtype":"success","is_error":false,"structured_output":payload}));
            lines
        } else {
            vec![
                json!({"type":"thread.started","thread_id":"fixture"}),
                json!({"type":"turn.started"}),
                json!({"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":payload.to_string()}}),
                json!({"type":"turn.completed"}),
            ]
        };
        // Delay the terminal after a preview so the UI must print transient text.
        let chunks: Vec<_> = if preview.is_some() {
            let prefix = lines[..lines.len() - 1]
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            vec![
                json!({"bytes":prefix.as_bytes()}),
                json!({"bytes":(lines.last().unwrap().to_string()+"\n").as_bytes(),"repeat_delay_ms":200}),
            ]
        } else {
            vec![
                json!({"bytes":(lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")+"\n").as_bytes()}),
            ]
        };
        let mut scenario = json!({"stdout":chunks,"stderr":[{"bytes":[255,13,10]}],"drain_stdin":true,"report_path":self.report_path(),"exit_code":exit,"auth_stdout":self.claude,"auth_status":if self.claude {r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty"}"#} else {"Logged in using ChatGPT\n"}});
        if hang {
            scenario["hang_ms"] = json!(10000);
        }
        let _ = fs::remove_file(self.report_path());
        fs::write(
            self.home.path().join("cyoa-fixture.json"),
            serde_json::to_vec(&scenario).unwrap(),
        )
        .unwrap();
    }
    fn spawn(&self, read_stdout: bool) -> App {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cyoa"));
        command
            .args([
                "play",
                "--headless",
                "--backend",
                if self.claude { "claude" } else { "codex" },
                "--executable",
            ])
            .arg(fixture_executable())
            .arg("--home")
            .arg(self.home.path())
            .arg("--config-dir")
            .arg(self.home.path())
            .env("PATH", "/bin");
        App::spawn(command, read_stdout)
    }
    fn request(&self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(4);
        while !self.report_path().exists() {
            assert!(Instant::now() < deadline, "fixture request not launched");
            thread::sleep(Duration::from_millis(5));
        }
        serde_json::from_slice(&fs::read(self.report_path()).unwrap()).unwrap()
    }
    fn calls(&self) -> usize {
        fs::read_to_string(self.home.path().join("launches.jsonl"))
            .unwrap()
            .lines()
            .filter(|l| l.contains(if self.claude { "\"-p\"" } else { "\"exec\"" }))
            .count()
    }
}
struct App {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: mpsc::Receiver<(bool, Vec<u8>)>,
    readers: Vec<thread::JoinHandle<()>>,
    out: Vec<u8>,
    err: Vec<u8>,
}
impl App {
    fn spawn(mut command: Command, read_stdout: bool) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let (tx, rx) = mpsc::channel();
        fn reader(
            mut stream: impl Read + Send + 'static,
            control: bool,
            tx: mpsc::Sender<(bool, Vec<u8>)>,
        ) -> thread::JoinHandle<()> {
            thread::spawn(move || {
                let mut buf = [0; 1024];
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send((control, buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
        }
        let mut readers = vec![reader(child.stderr.take().unwrap(), true, tx.clone())];
        let stdout = child.stdout.take().unwrap();
        if read_stdout {
            readers.push(reader(stdout, false, tx));
        } else {
            drop(stdout);
        }
        Self {
            child,
            stdin,
            rx,
            readers,
            out: vec![],
            err: vec![],
        }
    }
    fn demo() -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cyoa"));
        command
            .args(["play", "--headless", "--demo"])
            .env("PATH", "/absent-vendors");
        Self::spawn(command, true)
    }
    fn line(&mut self, line: &str) {
        writeln!(self.stdin.as_mut().unwrap(), "{line}").unwrap();
    }
    fn drain(&mut self) {
        while let Ok((control, bytes)) = self.rx.try_recv() {
            if control {
                self.err.extend(bytes);
            } else {
                self.out.extend(bytes);
            }
        }
    }
    fn mark(&mut self) -> usize {
        self.drain();
        self.err.len()
    }
    fn wait(&mut self, start: usize, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            self.drain();
            if String::from_utf8_lossy(&self.err[start..]).contains(needle) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "waiting for {needle:?}: {}",
                String::from_utf8_lossy(&self.err)
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn command(&mut self, line: &str, response: &str) {
        let start = self.mark();
        self.line(line);
        self.wait(start, response);
    }
    fn signal(&self) {
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(self.child.id() as i32).unwrap(),
            rustix::process::Signal::INT,
        )
        .unwrap();
    }
    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            self.drain();
            if let Some(status) = self.child.try_wait().unwrap() {
                for reader in self.readers.drain(..) {
                    reader.join().unwrap();
                }
                self.drain();
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "headless exit did not finish: {}",
                String::from_utf8_lossy(&self.err)
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn stdout(&mut self) -> String {
        self.drain();
        String::from_utf8(self.out.clone()).unwrap()
    }
    fn stderr(&mut self) -> String {
        self.drain();
        String::from_utf8(self.err.clone()).unwrap()
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stdin.take();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}
fn assert_gone(report: &Value) {
    let pid = rustix::process::Pid::from_raw(report["pid"].as_u64().unwrap() as i32).unwrap();
    assert_eq!(
        rustix::process::test_kill_process(pid),
        Err(rustix::io::Errno::SRCH),
        "child cleanup regression"
    );
    assert!(
        !Path::new(report["cwd"].as_str().unwrap()).exists(),
        "workspace remains"
    );
}
fn begin(fixture: &Fixture, data: &Value) -> App {
    fixture.set(&data["outline"], 0, false, None);
    let mut app = fixture.spawn(true);
    app.wait(0, "Brief:");
    app.command("  雨の港 with \\\"quotes\\\"  ", "Outline review:");
    app.command("/edit", "Title:");
    app.command("  Edited ‘Harbour’  ", "Description:");
    app.command(
        "  Two distinct Ajaxes meet in a 雨 harbour.  ",
        "Outline review:",
    );
    fixture.set(&data["cast"], 0, false, None);
    app.command("", "Character selection:");
    assert!(request_text(&fixture.request()).contains("Two distinct Ajaxes"));
    app.command("0", "Rejected:");
    app.command("99", "Rejected:");
    app.command("/retry", "Rejected:");
    assert_eq!(fixture.calls(), 2, "invalid selection regenerated cast");
    app
}
fn request_text(report: &Value) -> String {
    String::from_utf8(
        report["stdin"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect(),
    )
    .unwrap()
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
    let output = Command::new(env!("CARGO_BIN_EXE_cyoa"))
        .args([
            "play",
            "--headless",
            "--backend",
            "codex",
            "--executable",
            "/missing/codex",
        ])
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
        let deadline = Instant::now() + Duration::from_secs(2);
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
    let commands = "/help\n".repeat(400) + "/quit\n";
    app.stdin
        .as_mut()
        .unwrap()
        .write_all(commands.as_bytes())
        .unwrap();
    app.wait(mark, "Session closed");
    assert!(app.finish().success());
}
#[test]
fn command_surface_rejects_demo_vendor_settings_and_unimplemented_ui() {
    for args in [
        vec!["play", "--demo"],
        vec!["play", "--headless", "--demo", "--model", "sonnet"],
        vec!["play", "--headless", "--demo", "--executable", "/bin/false"],
        vec!["play", "--headless", "--backend", "other"],
    ] {
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_cyoa"))
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[test]
fn full_undrained_stderr_does_not_block_error_exit() {
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
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(writer))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
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
