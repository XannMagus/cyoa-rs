use super::*;
use crate::backend::{Backend, BackendError, GenerationRequest, GenerationResponse};
use crate::backends::process::{self, ProcessOutcome, SupervisorError};
use cyoa_application::{cancellation::CancellationToken, diagnostics::TransportDiagnostics};

/// Fixed-profile Claude adapter. Construction verifies saved subscription
/// authentication using the same executable and environment as generation.
/// Authentication can expire or change afterwards; generation never changes the
/// login mode, passes no metered-auth variable and never uses `--bare`.
#[derive(Debug)]
pub struct ClaudeCliBackend {
    config: ClaudeInvocationConfig,
}

impl ClaudeCliBackend {
    pub fn connect(
        config: ClaudeInvocationConfig,
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
                args: ["auth", "status", "--json"].map(OsString::from).into(),
                env: explicit_environment(&config),
                stdin: vec![],
                bounds: config.bounds,
            },
            cancel,
            &mut |_| Ok(()),
        );
        let outcome = result.map_err(|error| match error {
            // Status output is evidence about the account, never a candidate.
            SupervisorError::NonzeroExit {
                exit_code,
                diagnostics,
            } => BackendError::Unavailable {
                message: format!("Claude authentication check exited with status {exit_code}"),
                diagnostics,
            },
            other => transport_error(other, None),
        })?;
        if cancel.is_cancelled() {
            return Err(cancelled(None, outcome.diagnostics));
        }
        if !subscription_auth(&outcome.diagnostics) {
            return Err(BackendError::Unavailable {
                message: "Claude subscription authentication was not confirmed; use a claude.ai login in the selected home".into(),
                diagnostics: outcome.diagnostics,
            });
        }
        Ok(Self { config })
    }
}

impl Backend for ClaudeCliBackend {
    fn generate(
        &mut self,
        request: GenerationRequest<'_>,
        cancel: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        if cancel.is_cancelled() {
            return Err(cancelled(None, TransportDiagnostics::empty()));
        }
        let prepared = PreparedClaudeRequest::prepare_parts(
            &self.config,
            request.instructions,
            request.prompt,
            request.schema,
        )
        .map_err(|e| unavailable(format!("Claude request preparation: {e}")))?;
        let mut protocol = protocol::Protocol::default();
        let mut previewed = false;
        let transport = process::run(prepared.into_process_spec(), cancel, &mut |record| {
            match protocol.record(record) {
                Ok(Some(fragment)) => {
                    // Tentative text from the correlated payload block, forwarded as
                    // it arrives; the final result stays authoritative.
                    previewed = true;
                    on_json(&fragment);
                    Ok(())
                }
                Ok(None) => Ok(()),
                Err(error) => Err(error.to_string()),
            }
        });
        reconcile(transport, protocol.finish(), previewed, cancel, on_json)
    }
}

/// Only the confirmed subscription shape passes: unknown, missing or conflicting
/// fields fail closed. Other fields (identity, directories) are ignored here.
fn subscription_auth(diagnostics: &TransportDiagnostics) -> bool {
    // Derived struct deserialization rejects repeated known fields (including
    // equal values), unlike Value's last-wins map. Unknown identity/configuration
    // fields remain tolerated, as in the confirmed auth-status profile.
    #[derive(serde::Deserialize)]
    struct AuthStatus {
        #[serde(rename = "loggedIn")]
        logged_in: bool,
        #[serde(rename = "authMethod")]
        auth_method: String,
        #[serde(rename = "apiProvider")]
        api_provider: String,
    }
    let Ok(status) = serde_json::from_slice::<AuthStatus>(diagnostics.stdout()) else {
        return false;
    };
    status.logged_in && status.auth_method == "claude.ai" && status.api_provider == "firstParty"
}

fn reconcile(
    transport: Result<ProcessOutcome, SupervisorError>,
    protocol: Result<protocol::Completion, protocol::Failure>,
    previewed: bool,
    cancel: &CancellationToken,
    on_json: &mut dyn FnMut(&str),
) -> Result<GenerationResponse, BackendError> {
    // Transport failure remains authoritative, including cleanup after a codec
    // rejection. Never replace a nested initiating cause with a protocol error.
    let (candidate, completion) = match protocol {
        Ok(done) => (
            Some(done.candidate.payload),
            Ok((done.usage, done.provenance)),
        ),
        Err(failure) => (failure.candidate.map(|c| c.payload), Err(failure.error)),
    };
    let outcome = transport.map_err(|error| transport_error(error, candidate.clone()))?;
    if cancel.is_cancelled() {
        return Err(cancelled(candidate, outcome.diagnostics));
    }
    let (usage, provenance) = completion.map_err(|error| BackendError::Generation {
        message: error.to_string(),
        raw_response: candidate.clone().unwrap_or_default(),
        diagnostics: outcome.diagnostics.clone(),
    })?;
    let response = GenerationResponse::from_json(candidate.unwrap_or_default(), usage)
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
        .with_diagnostics(outcome.diagnostics)
        .with_provenance(provenance);
    if cancel.is_cancelled() {
        return Err(cancelled(
            Some(response.raw_response().into()),
            response.diagnostics().clone(),
        ));
    }
    // A stream without previews still honors the complete-JSON contract once.
    // After previews, the engine reconciles the remainder from the final value.
    if !previewed {
        on_json(response.raw_response());
        if cancel.is_cancelled() {
            return Err(cancelled(
                Some(response.raw_response().into()),
                response.diagnostics().clone(),
            ));
        }
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

// Mirrors the vendor-blind mapping of the Codex adapter with Claude wording. It is
// duplicated rather than shared so this slice never edits the peer adapter's file
// (ARCH-003); a later behavior-preserving extraction can unify both.
fn transport_error(error: SupervisorError, raw: Option<String>) -> BackendError {
    let diagnostics = error.diagnostics();
    match error {
        SupervisorError::Cancelled { .. } => cancelled(raw, diagnostics),
        SupervisorError::Timeout { .. } => BackendError::Timeout {
            raw_response: raw,
            diagnostics,
        },
        SupervisorError::Spawn(source) => BackendError::Unavailable {
            message: format!("Claude launch failed: {source}"),
            diagnostics,
        },
        SupervisorError::Unsupported => BackendError::Unavailable {
            message: "process supervision is unsupported on this platform".into(),
            diagnostics,
        },
        SupervisorError::NonzeroExit { exit_code, .. } => {
            let message = format!("Claude exited with status {exit_code}");
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
