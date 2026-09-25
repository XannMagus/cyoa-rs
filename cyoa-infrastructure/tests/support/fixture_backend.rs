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
use std::process::{Command, Stdio};

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
        _request: GenerationRequest<'_>,
        cancel: &cyoa_application::cancellation::CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        let exe = std::env::var("CARGO_BIN_EXE_subprocess_fixture")
            .expect("cargo sets CARGO_BIN_EXE_subprocess_fixture for this crate's tests");
        let mut child = Command::new(exe)
            .arg(&self.scenario_json)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn subprocess_fixture");
        // Item 2's fixture backend does not drive scenarios needing an
        // acknowledgement byte; close stdin immediately so a `drain_stdin`
        // scenario is not left blocked waiting for input.
        drop(child.stdin.take());
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
    let chunk = |bytes: &[u8]| {
        serde_json::json!({
            "bytes": bytes,
            "wait_for_stdin_byte_first": false,
        })
    };
    serde_json::json!({
        "stdout": stdout.iter().map(|b| chunk(b)).collect::<Vec<_>>(),
        "stderr": stderr.iter().map(|b| chunk(b)).collect::<Vec<_>>(),
        "drain_stdin": false,
        "spawn_descendant_holding_stdout_ms": null,
        "exit_code": exit_code,
    })
    .to_string()
}
