//! Controlled subprocess for exercising a real process boundary in tests,
//! without an installed vendor CLI, credentials, or network.
//!
//! Not part of the shipped command surface: it is an ungated `[[bin]]` of
//! `cyoa-infrastructure`, reached directly via `CARGO_BIN_EXE_subprocess_fixture`
//! by this crate's own tests (and, later, via `escargot` by `cyoa-cli`'s binary
//! tests). It is never invoked by `cyoa-cli`'s shipped commands.
//!
//! Scenario protocol: one JSON scenario document passed as argv[1] (never a
//! shell string). Originally kept intentionally minimal for Phase 1 item 2
//! (no polling/cancellation wake of its own — this process is the *child*
//! under test, not a supervisor); item 3 added `spawn_descendant_holding_stdout_ms`
//! exercise, this process's own pid, and its full environment to the
//! `Report`, for the vendor-neutral process supervisor's inherited-pipe,
//! process-group-cleanup and env-policy tests. A `report_path` file is
//! test-only scratch data the reading test deletes immediately
//! (`read_report`) — this binary is never installed by `cargo install
//! cyoa-cli`, so its environment dump never reaches a shipped artifact.

use serde::Deserialize;
use std::io::{Read, Write};

#[derive(Debug, Deserialize)]
struct Scenario {
    /// Bytes to write to stdout, in order.
    #[serde(default)]
    stdout: Vec<Chunk>,
    /// Bytes to write to stderr, in order.
    #[serde(default)]
    stderr: Vec<Chunk>,
    /// If true, read and discard all of stdin before writing any output
    /// (simulates a well-behaved child that drains its request payload).
    #[serde(default)]
    drain_stdin: bool,
    /// If set, spawn a detached descendant that sleeps this many milliseconds
    /// before exiting, inheriting this process's stdout/stderr handles. Used
    /// by later inherited-pipe tests (item 3); not exercised by item 2's own
    /// tests, but implemented now so the field is not dead protocol surface.
    #[serde(default)]
    spawn_descendant_holding_stdout_ms: Option<u64>,
    /// If set, write a `Report` (this process's argv and, if `drain_stdin` is
    /// true, the exact bytes read from stdin) to this file path before
    /// exiting. A handshake file, not a timing-based proof: lets a test
    /// confirm exactly what the child received, matching item 2's own
    /// "can report argv/stdin" requirement. Written atomically (temp file
    /// plus rename) so a test polling for its existence never observes a
    /// partial write.
    #[serde(default)]
    report_path: Option<String>,
    /// If set, sleep this many milliseconds after writing the report (if
    /// any) and before writing any output/exiting. Gives a test a real
    /// handshake for "the child is alive and idle, waiting to be
    /// cancelled" instead of a fixed sleep guessing at timing (the plan
    /// forbids "timing-only sleeps as proof a child started/stopped").
    #[serde(default)]
    hang_ms: Option<u64>,
    #[serde(default)]
    hang_after_output_ms: Option<u64>,
    #[serde(default = "subscription_status")]
    auth_status: String,
    #[serde(default)]
    auth_exit_code: i32,
    #[serde(default)]
    auth_stdout: bool,
    /// Vendor-fixture-only obstruction for real workspace-cleanup failures.
    #[serde(default)]
    replace_workspace_with_file: bool,
    exit_code: i32,
}

#[derive(Debug, serde::Serialize)]
struct Report {
    schema: Option<Vec<u8>>,
    cwd: std::path::PathBuf,
    argv: Vec<String>,
    stdin: Vec<u8>,
    /// This process's own environment variable NAMES ONLY (never values), so
    /// a test can assert the supervisor's `EnvPolicy` is an explicit
    /// allowlist rather than silent inheritance of the calling process's
    /// full environment (item 3, Decision 3), without ever writing a real
    /// environment's values (host tokens/credentials) into a report file
    /// on disk. Built from `vars_os` (never `vars()`, which panics on any
    /// non-UTF-8 value) with a lossy string conversion for the keys only.
    env_keys: std::collections::BTreeSet<String>,
    /// This process's own pid, so a test can confirm cleanup actually
    /// terminated (and, once the reap grace period elapses, reaped) this
    /// exact process rather than only observing that `run` returned.
    pid: u32,
    /// The PID of the descendant spawned for
    /// `spawn_descendant_holding_stdout_ms`, if any, so a test can confirm
    /// the supervisor's process-group cleanup actually terminated it
    /// instead of only observing that the call returned.
    descendant_pid: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Chunk {
    #[serde(default)]
    repeat_delay_ms: u64,
    /// Raw bytes as a JSON array of u8, so arbitrary bytes (including invalid
    /// UTF-8) round-trip through JSON/argv cleanly with no extra dependency.
    bytes: Vec<u8>,
    /// If true, block reading a single byte from stdin before writing this
    /// chunk, letting a test control ordering against the parent's writes.
    #[serde(default)]
    wait_for_stdin_byte_first: bool,
    /// Write `bytes` this many times in a row. Lets a small JSON scenario
    /// (argv has its own OS size limit — `MAX_ARG_STRLEN` on Linux) still
    /// produce enough total output to exceed a pipe's buffer, for real
    /// backpressure tests.
    #[serde(default = "one")]
    repeat: u32,
}

fn one() -> u32 {
    1
}

fn subscription_status() -> String {
    "Logged in using ChatGPT\n".into()
}

/// Recognized by a descendant re-exec (see `spawn_descendant_holding_stdout_ms`):
/// sleep for the given number of milliseconds, then exit 0, without parsing a
/// scenario at all.
const SLEEP_MS_ENV: &str = "SUBPROCESS_FIXTURE_SLEEP_MS";

fn main() {
    if let Ok(sleep_ms) = std::env::var(SLEEP_MS_ENV) {
        let millis: u64 = sleep_ms.parse().unwrap_or(0);
        std::thread::sleep(std::time::Duration::from_millis(millis));
        std::process::exit(0);
    }

    let argv: Vec<String> = std::env::args().collect();
    // Vendor-fixture mode keeps the production argv/stdin contract intact.
    // Only this test binary reads a scenario from its selected fake HOME.
    let vendor_mode = matches!(argv.get(1).map(String::as_str), Some("exec" | "login"));
    let scenario_json = if vendor_mode {
        std::fs::read_to_string(
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join("cyoa-fixture.json"),
        )
        .unwrap()
    } else {
        argv.get(1)
            .cloned()
            .expect("subprocess_fixture requires a scenario")
    };
    let scenario: Scenario =
        serde_json::from_str(&scenario_json).expect("argv[1] must be a valid Scenario JSON");

    if vendor_mode {
        let path =
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join("launches.jsonl");
        let mut calls = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(
            calls,
            "{}",
            serde_json::json!({"command":argv[1],"pid":std::process::id()})
        )
        .unwrap();
    }

    if argv.get(1).map(String::as_str) == Some("login") {
        assert_eq!(&argv[1..], &["login", "status"]);
        if scenario.auth_stdout {
            print!("{}", scenario.auth_status);
        } else {
            eprint!("{}", scenario.auth_status);
        }
        std::process::exit(scenario.auth_exit_code);
    }

    let mut descendant_pid = None;
    if let Some(sleep_ms) = scenario.spawn_descendant_holding_stdout_ms {
        let exe = std::env::current_exe().expect("resolve current_exe for descendant re-exec");
        if let Ok(child) = std::process::Command::new(exe)
            .env(SLEEP_MS_ENV, sleep_ms.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
        {
            descendant_pid = Some(child.id());
        }
        // Deliberately not waited on: the whole point is a descendant that
        // can outlive this process while still holding the inherited pipes.
    }

    let mut received_stdin = Vec::new();
    if scenario.drain_stdin {
        let _ = std::io::stdin().read_to_end(&mut received_stdin);
    }

    if let Some(report_path) = &scenario.report_path {
        let report = Report {
            schema: if vendor_mode {
                Some(std::fs::read("schema.json").expect("prepared schema exists"))
            } else {
                None
            },
            cwd: std::env::current_dir().expect("fixture working directory"),
            argv,
            stdin: received_stdin,
            env_keys: std::env::vars_os()
                .map(|(k, _)| k.to_string_lossy().into_owned())
                .collect(),
            pid: std::process::id(),
            descendant_pid,
        };
        let json = serde_json::to_string(&report).expect("serialize report");
        // Write-then-rename: a test polling for this file's existence must
        // never observe a partially written report.
        let tmp_path = format!("{report_path}.tmp");
        let _ = std::fs::write(&tmp_path, json);
        let _ = std::fs::rename(&tmp_path, report_path);
    }

    if let Some(hang_ms) = scenario.hang_ms {
        std::thread::sleep(std::time::Duration::from_millis(hang_ms));
    }

    write_chunks(&scenario.stdout, std::io::stdout().lock());
    write_chunks(&scenario.stderr, std::io::stderr().lock());

    if scenario.replace_workspace_with_file {
        assert!(vendor_mode, "cleanup obstruction is vendor-fixture-only");
        let cwd = std::env::current_dir().unwrap();
        assert!(
            cwd.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("cyoa-request-")
        );
        // Remove only the known prepared file and then an empty directory. Never
        // recursively delete an arbitrary path. The parent test removes the marker.
        std::fs::remove_file(cwd.join("schema.json")).unwrap();
        std::env::set_current_dir(std::env::var_os("HOME").unwrap()).unwrap();
        std::fs::remove_dir(&cwd).unwrap();
        std::fs::write(&cwd, b"fixture cleanup obstruction").unwrap();
    }

    if let Some(ms) = scenario.hang_after_output_ms {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }

    std::process::exit(scenario.exit_code);
}

fn write_chunks(chunks: &[Chunk], mut out: impl Write) {
    for chunk in chunks {
        if chunk.wait_for_stdin_byte_first {
            let mut ack = [0u8; 1];
            let _ = std::io::stdin().read_exact(&mut ack);
        }
        for _ in 0..chunk.repeat.max(1) {
            let _ = out.write_all(&chunk.bytes);
            if chunk.repeat_delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(chunk.repeat_delay_ms));
            }
        }
        let _ = out.flush();
    }
}
