#![allow(dead_code)]
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};
pub(crate) fn fixture_executable() -> &'static Path {
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
pub(crate) fn fixture_story() -> Value {
    serde_json::from_str(include_str!(
        "../../../cyoa-infrastructure/tests/fixtures/phase0_story.json"
    ))
    .unwrap()
}
pub(crate) struct Fixture {
    pub(crate) home: tempfile::TempDir,
    pub(crate) claude: bool,
}
impl Fixture {
    pub(crate) fn new(claude: bool) -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            claude,
        }
    }
    pub(crate) fn report_path(&self) -> PathBuf {
        self.home.path().join("report.json")
    }
    pub(crate) fn set(&self, payload: &Value, exit: i32, hang: bool, preview: Option<&str>) {
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
    pub(crate) fn spawn(&self, read_stdout: bool) -> App {
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
    pub(crate) fn request(&self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !self.report_path().exists() {
            assert!(Instant::now() < deadline, "fixture request not launched");
            thread::sleep(Duration::from_millis(5));
        }
        serde_json::from_slice(&fs::read(self.report_path()).unwrap()).unwrap()
    }
    pub(crate) fn calls(&self) -> usize {
        fs::read_to_string(self.home.path().join("launches.jsonl"))
            .unwrap()
            .lines()
            .filter(|l| l.contains(if self.claude { "\"-p\"" } else { "\"exec\"" }))
            .count()
    }
}
pub(crate) struct App {
    pub(crate) child: Child,
    pub(crate) stdin: Option<ChildStdin>,
    rx: mpsc::Receiver<(bool, Vec<u8>)>,
    readers: Vec<thread::JoinHandle<()>>,
    pub(crate) out: Vec<u8>,
    pub(crate) err: Vec<u8>,
    pub(crate) _data: tempfile::TempDir,
}
impl App {
    pub(crate) fn spawn(mut command: Command, read_stdout: bool) -> Self {
        let data = tempfile::tempdir().unwrap();
        if !command.get_args().any(|arg| arg == "--data-dir") {
            command.arg("--data-dir").arg(data.path());
        }
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
            _data: data,
        }
    }
    pub(crate) fn demo() -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cyoa"));
        command
            .args(["play", "--headless", "--demo"])
            .env("PATH", "/absent-vendors");
        Self::spawn(command, true)
    }
    pub(crate) fn line(&mut self, line: &str) {
        writeln!(self.stdin.as_mut().unwrap(), "{line}").unwrap();
    }
    pub(crate) fn drain(&mut self) {
        while let Ok((control, bytes)) = self.rx.try_recv() {
            if control {
                self.err.extend(bytes);
            } else {
                self.out.extend(bytes);
            }
        }
    }
    pub(crate) fn mark(&mut self) -> usize {
        self.drain();
        self.err.len()
    }
    pub(crate) fn wait(&mut self, start: usize, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
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
    pub(crate) fn command(&mut self, line: &str, response: &str) {
        let start = self.mark();
        self.line(line);
        self.wait(start, response);
        if response.starts_with("[committed turn ") {
            self.wait(
                start,
                &format!(
                    "turns={} durability=Clean",
                    response
                        .trim_start_matches("[committed turn ")
                        .trim_end_matches(']')
                ),
            );
        }
    }
    pub(crate) fn signal(&self) {
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(self.child.id() as i32).unwrap(),
            rustix::process::Signal::INT,
        )
        .unwrap();
    }
    pub(crate) fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(30);
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
    pub(crate) fn stdout(&mut self) -> String {
        self.drain();
        String::from_utf8(self.out.clone()).unwrap()
    }
    pub(crate) fn stderr(&mut self) -> String {
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
pub(crate) fn assert_gone(report: &Value) {
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
pub(crate) fn begin(fixture: &Fixture, data: &Value) -> App {
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
pub(crate) fn request_text(report: &Value) -> String {
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
