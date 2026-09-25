//! A minimal, test-only `Backend` that drives the real `subprocess_fixture`
//! process boundary. This is deliberately NOT the vendor-neutral process
//! supervisor (Phase 1 item 3): blocking `std::process::Command` +
//! `wait_with_output()`, no cancellation wake, no incremental streaming.
//! It exists only to prove `BackendError`/`TransportDiagnostics` mapping and
//! `TokenUsage` normalization survive a real subprocess boundary.

use cyoa_core::text::TransportDiagnostics;
use cyoa_infrastructure::backend::{
    Backend, BackendError, GenerationRequest, GenerationResponse, TokenUsage,
    normalize_input_tokens,
};
use std::io::Write;
use std::process::{Command, Stdio};

/// Compile-time, per the standard convention (`std::env::var` at runtime only
/// happens to work because cargo sets this for every test binary of the
/// owning crate).
const FIXTURE_EXE: &str = env!("CARGO_BIN_EXE_subprocess_fixture");

/// Vendor-reported usage the fixture "backend" applies to a successful
/// response, exercising `normalize_input_tokens` over the real process path.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReportedUsage {
    pub input_total: u64,
    pub input_cached: Option<u64>,
    pub output: Option<u64>,
}

pub struct FixtureBackend {
    scenario_json: String,
    usage: ReportedUsage,
}

impl FixtureBackend {
    pub fn new(scenario_json: impl Into<String>) -> Self {
        Self {
            scenario_json: scenario_json.into(),
            usage: ReportedUsage::default(),
        }
    }

    pub fn with_usage(mut self, usage: ReportedUsage) -> Self {
        self.usage = usage;
        self
    }
}

impl Backend for FixtureBackend {
    fn generate(
        &mut self,
        request: GenerationRequest<'_>,
        cancel: &cyoa_application::cancellation::CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        let mut child = Command::new(FIXTURE_EXE)
            .arg(&self.scenario_json)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn subprocess_fixture");
        // Real backends write the request to stdin and close it (see
        // PLAN.md's confirmed `claude -p`/`codex exec` invocations); mirror
        // that here so a scenario with `drain_stdin`/`report_path` set can
        // prove the exact prompt bytes it received, not just its own output.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(request.prompt.as_bytes());
            // Dropping `stdin` here closes the write end.
        }
        let output = child
            .wait_with_output()
            .expect("wait for subprocess_fixture");

        let diagnostics = TransportDiagnostics::new(output.stdout.clone(), output.stderr.clone());

        if let Ok(text) = std::str::from_utf8(&output.stdout) {
            on_json(text);
        }

        if cancel.is_cancelled() {
            return Err(BackendError::Cancelled { diagnostics });
        }

        if !output.status.success() {
            return Err(if output.stdout.is_empty() {
                // No candidate payload at all: the process could not produce
                // anything, which is closer to "unavailable" than a
                // malformed-but-present response.
                BackendError::Unavailable {
                    message: format!(
                        "subprocess_fixture exited with {} and no stdout",
                        output.status
                    ),
                    diagnostics,
                }
            } else {
                // A candidate payload was emitted but the process still
                // failed: distinct from the no-output case above.
                BackendError::Generation {
                    message: format!("subprocess_fixture exited with {}", output.status),
                    raw_response: String::from_utf8_lossy(&output.stdout).into_owned(),
                    diagnostics,
                }
            });
        }

        let raw = String::from_utf8(output.stdout)
            .expect("nominal fixture scenarios emit valid UTF-8 stdout");
        let usage = TokenUsage {
            input: Some(normalize_input_tokens(
                self.usage.input_total,
                self.usage.input_cached,
            )),
            output: self.usage.output,
        };
        GenerationResponse::from_json(raw, usage).map_err(|error| match error {
            BackendError::Generation {
                message,
                raw_response,
                ..
            } => BackendError::Generation {
                message,
                raw_response,
                diagnostics,
            },
            other => other,
        })
    }
}

/// Builds a scenario JSON document from raw byte chunks, matching
/// `subprocess_fixture`'s `Scenario`/`Chunk` shape without pulling its
/// (private, bin-local) types into this test crate.
pub fn scenario(stdout: &[&[u8]], stderr: &[&[u8]], exit_code: i32) -> String {
    scenario_with_report(stdout, stderr, exit_code, false, None)
}

/// Same as `scenario`, additionally asking the fixture to drain stdin and
/// report exactly what it received (argv and stdin bytes) to `report_path`.
pub fn scenario_with_report(
    stdout: &[&[u8]],
    stderr: &[&[u8]],
    exit_code: i32,
    drain_stdin: bool,
    report_path: Option<&std::path::Path>,
) -> String {
    scenario_full(stdout, stderr, exit_code, drain_stdin, report_path, None)
}

/// Full builder, additionally able to ask the fixture to spawn a detached
/// descendant that holds the inherited stdout/stderr pipes open for
/// `spawn_descendant_holding_stdout_ms` after this process exits — used by
/// the process supervisor's inherited-pipe/group-cleanup tests.
pub fn scenario_full(
    stdout: &[&[u8]],
    stderr: &[&[u8]],
    exit_code: i32,
    drain_stdin: bool,
    report_path: Option<&std::path::Path>,
    spawn_descendant_holding_stdout_ms: Option<u64>,
) -> String {
    let chunk = |bytes: &[u8]| {
        serde_json::json!({
            "bytes": bytes,
            "wait_for_stdin_byte_first": false,
        })
    };
    serde_json::json!({
        "stdout": stdout.iter().map(|b| chunk(b)).collect::<Vec<_>>(),
        "stderr": stderr.iter().map(|b| chunk(b)).collect::<Vec<_>>(),
        "drain_stdin": drain_stdin,
        "spawn_descendant_holding_stdout_ms": spawn_descendant_holding_stdout_ms,
        "report_path": report_path.map(|p| p.to_string_lossy().into_owned()),
        "exit_code": exit_code,
    })
    .to_string()
}

/// A unique path in the system temp directory for one test's report file.
/// Test-only scratch data, removed by `read_report` once read.
pub fn report_path(label: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "cyoa-subprocess-fixture-report-{label}-{}-{n}.json",
        std::process::id()
    ))
}

#[derive(Debug, serde::Deserialize)]
pub struct Report {
    pub argv: Vec<String>,
    pub stdin: Vec<u8>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub descendant_pid: Option<u32>,
}

/// Reads and deletes the report file written by a `report_path` scenario.
pub fn read_report(path: &std::path::Path) -> Report {
    let json = std::fs::read_to_string(path).expect("subprocess_fixture wrote its report");
    let _ = std::fs::remove_file(path);
    serde_json::from_str(&json).expect("report is valid JSON")
}
