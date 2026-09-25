//! Controlled subprocess for exercising a real process boundary in tests,
//! without an installed vendor CLI, credentials, or network.
//!
//! Not part of the shipped command surface: it is an ungated `[[bin]]` of
//! `cyoa-infrastructure`, reached directly via `CARGO_BIN_EXE_subprocess_fixture`
//! by this crate's own tests (and, later, via `escargot` by `cyoa-cli`'s binary
//! tests). It is never invoked by `cyoa-cli`'s shipped commands.
//!
//! Scenario protocol: one JSON scenario document passed as argv[1] (never a
//! shell string). Kept intentionally minimal for Phase 1 item 2 — no polling,
//! no cancellation wake, no incremental streaming control beyond an optional
//! stdin acknowledgement per chunk.

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
    exit_code: i32,
}

#[derive(Debug, Deserialize)]
struct Chunk {
    /// Raw bytes as a JSON array of u8, so arbitrary bytes (including invalid
    /// UTF-8) round-trip through JSON/argv cleanly with no extra dependency.
    bytes: Vec<u8>,
    /// If true, block reading a single byte from stdin before writing this
    /// chunk, letting a test control ordering against the parent's writes.
    #[serde(default)]
    wait_for_stdin_byte_first: bool,
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

    let scenario_json = std::env::args()
        .nth(1)
        .expect("subprocess_fixture requires a JSON scenario as argv[1]");
    let scenario: Scenario =
        serde_json::from_str(&scenario_json).expect("argv[1] must be a valid Scenario JSON");

    if let Some(sleep_ms) = scenario.spawn_descendant_holding_stdout_ms {
        let exe = std::env::current_exe().expect("resolve current_exe for descendant re-exec");
        let _ = std::process::Command::new(exe)
            .env(SLEEP_MS_ENV, sleep_ms.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn();
        // Deliberately not waited on: the whole point is a descendant that
        // can outlive this process while still holding the inherited pipes.
    }

    if scenario.drain_stdin {
        let mut discarded = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut discarded);
    }

    write_chunks(&scenario.stdout, std::io::stdout().lock());
    write_chunks(&scenario.stderr, std::io::stderr().lock());

    std::process::exit(scenario.exit_code);
}

fn write_chunks(chunks: &[Chunk], mut out: impl Write) {
    for chunk in chunks {
        if chunk.wait_for_stdin_byte_first {
            let mut ack = [0u8; 1];
            let _ = std::io::stdin().read_exact(&mut ack);
        }
        let _ = out.write_all(&chunk.bytes);
        let _ = out.flush();
    }
}
