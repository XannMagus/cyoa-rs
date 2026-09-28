use super::*;
use crate::backend::{Backend, BackendError, GenerationRequest, GenerationResponse};
use crate::backends::process::{self, ProcessOutcome, SupervisorError};
use cyoa_application::{cancellation::CancellationToken, diagnostics::TransportDiagnostics};

/// Fixed-profile complete-only Codex adapter. Construction verifies saved
/// subscription authentication using the same executable and environment.
/// Authentication can expire/change afterward; generation never changes login
/// mode or passes metered-auth environment overrides.
#[derive(Debug)]
pub struct CodexCliBackend {
    config: CodexInvocationConfig,
}
impl CodexCliBackend {
    pub fn connect(
        config: CodexInvocationConfig,
        cancel: &CancellationToken,
    ) -> Result<Self, BackendError> {
        if cancel.is_cancelled() {
            return Err(cancelled(None, TransportDiagnostics::empty()));
        }
        let workspace =
            RequestWorkspace::new().map_err(|e| unavailable(format!("auth workspace: {e}")))?;
        let result = process::run(
            ProcessSpec {
                workspace,
                program: config.executable.as_path().into(),
                args: vec!["login".into(), "status".into()],
                env: explicit_environment(&config),
                stdin: vec![],
                bounds: config.bounds,
            },
            cancel,
            &mut |_| Ok(()),
        );
        let outcome = result.map_err(|error| match error {
            // Auth output is status evidence, never a generation candidate.
            SupervisorError::NonzeroExit {
                exit_code,
                diagnostics,
            } => BackendError::Unavailable {
                message: format!("Codex authentication check exited with status {exit_code}"),
                diagnostics,
            },
            other => transport_error(other, None),
        })?;
        if cancel.is_cancelled() {
            return Err(cancelled(None, outcome.diagnostics));
        }
        if !subscription_auth(&outcome.diagnostics) {
            return Err(BackendError::Unavailable { message: "Codex subscription authentication was not confirmed; use a ChatGPT login in the selected home".into(), diagnostics: outcome.diagnostics });
        }
        Ok(Self { config })
    }
}
impl Backend for CodexCliBackend {
    fn generate(
        &mut self,
        request: GenerationRequest<'_>,
        cancel: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        if cancel.is_cancelled() {
            return Err(cancelled(None, TransportDiagnostics::empty()));
        }
        let prepared = PreparedCodexRequest::prepare_parts(
            &self.config,
            request.instructions,
            request.prompt,
            request.schema,
        )
        .map_err(|e| unavailable(format!("Codex request preparation: {e}")))?;
        let mut protocol = protocol::Protocol::default();
        let transport = process::run(prepared.into_process_spec(), cancel, &mut |record| {
            protocol.record(record).map_err(|e| e.to_string())
        });
        reconcile(transport, protocol.finish(), cancel, on_json)
    }
}

fn subscription_auth(diagnostics: &TransportDiagnostics) -> bool {
    let mut confirmed = 0;
    for bytes in [diagnostics.stdout(), diagnostics.stderr()] {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return false;
        };
        for line in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
            if line == "Logged in using ChatGPT" {
                confirmed += 1;
            }
            // This warning accompanied the frozen authenticated status capture.
            else if !line
                .starts_with("WARNING: proceeding, even though we could not create PATH aliases:")
            {
                return false;
            }
        }
    }
    confirmed == 1
}

fn reconcile(
    transport: Result<ProcessOutcome, SupervisorError>,
    protocol: Result<protocol::Completion, protocol::Failure>,
    cancel: &CancellationToken,
    on_json: &mut dyn FnMut(&str),
) -> Result<GenerationResponse, BackendError> {
    // Transport failure remains authoritative, including cleanup after consumer
    // rejection. Never replace a nested initiating cause with a protocol error.
    let (candidate, completion) = match protocol {
        Ok(done) => (Some(done.candidate), Ok(done.usage)),
        Err(failure) => (failure.candidate, Err(failure.error)),
    };
    let raw = candidate.map(|c| c.payload);
    let outcome = transport.map_err(|error| transport_error(error, raw.clone()))?;
    if cancel.is_cancelled() {
        return Err(cancelled(raw, outcome.diagnostics));
    }
    let usage = completion.map_err(|error| BackendError::Generation {
        message: error.to_string(),
        raw_response: raw.clone().unwrap_or_default(),
        diagnostics: outcome.diagnostics.clone(),
    })?;
    let response = GenerationResponse::from_json(raw.unwrap_or_default(), usage)
        .map_err(|error| match error {
            BackendError::Generation {
                message,
                raw_response,
                ..
            } => BackendError::Generation {
                message,
                raw_response,
                diagnostics: outcome.diagnostics.clone(),
            },
            other => other,
        })?
        .with_diagnostics(outcome.diagnostics);
    // No observed model/cost is invented from invocation configuration.
    if cancel.is_cancelled() {
        return Err(cancelled(
            Some(response.raw_response().into()),
            response.diagnostics().clone(),
        ));
    }
    on_json(response.raw_response());
    if cancel.is_cancelled() {
        return Err(cancelled(
            Some(response.raw_response().into()),
            response.diagnostics().clone(),
        ));
    }
    Ok(response)
}

fn cancelled(raw_response: Option<String>, diagnostics: TransportDiagnostics) -> BackendError {
    BackendError::Cancelled {
        raw_response,
        diagnostics,
    }
}
fn unavailable(message: String) -> BackendError {
    BackendError::Unavailable {
        message,
        diagnostics: TransportDiagnostics::empty(),
    }
}

fn transport_error(error: SupervisorError, raw: Option<String>) -> BackendError {
    let diagnostics = error.diagnostics();
    match error {
        SupervisorError::Cancelled { .. } => cancelled(raw, diagnostics),
        SupervisorError::Timeout { .. } => BackendError::Timeout {
            raw_response: raw,
            diagnostics,
        },
        SupervisorError::Spawn(source) => BackendError::Unavailable {
            message: format!("Codex launch failed: {source}"),
            diagnostics,
        },
        SupervisorError::Unsupported => BackendError::Unavailable {
            message: "process supervision is unsupported on this platform".into(),
            diagnostics,
        },
        SupervisorError::NonzeroExit { exit_code, .. } => {
            let message = format!("Codex exited with status {exit_code}");
            if diagnostics.stdout().is_empty() {
                BackendError::Unavailable {
                    message,
                    diagnostics,
                }
            } else {
                BackendError::Generation {
                    message,
                    raw_response: raw.unwrap_or_default(),
                    diagnostics,
                }
            }
        }
        SupervisorError::ConsumerRejected { reason, .. } => BackendError::Generation {
            message: reason,
            raw_response: raw.unwrap_or_default(),
            diagnostics,
        },
        error @ (SupervisorError::Io { .. }
        | SupervisorError::Cleanup { .. }
        | SupervisorError::IncompleteInput { .. }
        | SupervisorError::OutputBoundExceeded { .. }) => BackendError::Transport {
            message: safe_transport_summary(&error),
            raw_response: raw.unwrap_or_default(),
            diagnostics: Box::new(diagnostics),
            cause: Box::new(error),
        },
    }
}

// SupervisorError's Debug/cleanup Display includes captured buffers. Keep those
// bytes in the typed cause and diagnostics, not in the user-facing message.
fn safe_transport_summary(error: &SupervisorError) -> String {
    match error {
        SupervisorError::Cleanup {
            initial, failures, ..
        } => format!(
            "cleanup failed [{}]; initiating failure: {}",
            failures
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
            initial
                .as_deref()
                .map(safe_transport_summary)
                .unwrap_or_else(|| "none".into())
        ),
        SupervisorError::Io { failure, .. } => format!("process I/O failed: {failure}"),
        SupervisorError::IncompleteInput {
            written, expected, ..
        } => format!("request delivery incomplete: wrote {written} of {expected} bytes"),
        SupervisorError::OutputBoundExceeded { stream, .. } => {
            format!("{stream:?} output exceeded its configured bound")
        }
        SupervisorError::Spawn(source) => format!("process launch failed: {source}"),
        SupervisorError::Unsupported => "process supervision unsupported".into(),
        SupervisorError::Cancelled { .. } => "generation cancelled".into(),
        SupervisorError::Timeout { .. } => "generation timed out".into(),
        SupervisorError::NonzeroExit { exit_code, .. } => {
            format!("process exited with status {exit_code}")
        }
        SupervisorError::ConsumerRejected { reason, .. } => format!("protocol rejected: {reason}"),
    }
}

#[cfg(test)]
mod tests;
