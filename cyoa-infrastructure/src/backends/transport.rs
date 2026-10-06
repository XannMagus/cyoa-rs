//! Vendor-neutral mapping from supervised-process outcomes to backend errors,
//! shared by the CLI adapters. Each adapter keeps its own protocol, auth command
//! and success predicate (ARCH-003); only the outcome plumbing lives here, and it
//! never inspects a vendor's event shapes. `vendor` is wording only.
use super::process::{self, ProcessSpec, RequestWorkspace, SupervisorError};
use crate::backend::BackendError;
use cyoa_application::{cancellation::CancellationToken, diagnostics::TransportDiagnostics};

pub(crate) fn cancelled(
    raw_response: Option<String>,
    diagnostics: TransportDiagnostics,
) -> BackendError {
    BackendError::Cancelled {
        raw_response,
        diagnostics,
    }
}

pub(crate) fn unavailable(message: String) -> BackendError {
    BackendError::Unavailable {
        message,
        diagnostics: TransportDiagnostics::empty(),
    }
}

/// Runs the adapter's own auth-status command with the generation executable and
/// environment, and admits the backend only when `confirmed` accepts its output.
/// Status output is evidence about the account, never a generation candidate.
pub(crate) fn preflight_subscription(
    vendor: &str,
    spec: impl FnOnce(RequestWorkspace) -> ProcessSpec,
    confirmed: fn(&TransportDiagnostics) -> bool,
    guidance: &str,
    cancel: &CancellationToken,
) -> Result<(), BackendError> {
    if cancel.is_cancelled() {
        return Err(cancelled(None, TransportDiagnostics::empty()));
    }
    let workspace =
        RequestWorkspace::new().map_err(|e| unavailable(format!("auth workspace: {e}")))?;
    let outcome =
        process::run(spec(workspace), cancel, &mut |_| Ok(())).map_err(|error| match error {
            SupervisorError::NonzeroExit {
                exit_code,
                diagnostics,
            } => BackendError::Unavailable {
                message: format!("{vendor} authentication check exited with status {exit_code}"),
                diagnostics,
            },
            other => transport_error(vendor, other, None),
        })?;
    if cancel.is_cancelled() {
        return Err(cancelled(None, outcome.diagnostics));
    }
    if !confirmed(&outcome.diagnostics) {
        return Err(BackendError::Unavailable {
            message: format!("{vendor} subscription authentication was not confirmed; {guidance}"),
            diagnostics: outcome.diagnostics,
        });
    }
    Ok(())
}

pub(crate) fn transport_error(
    vendor: &str,
    error: SupervisorError,
    raw: Option<String>,
) -> BackendError {
    let diagnostics = error.diagnostics();
    match error {
        SupervisorError::Cancelled { .. } => cancelled(raw, diagnostics),
        SupervisorError::Timeout { .. } => BackendError::Timeout {
            raw_response: raw,
            diagnostics,
        },
        SupervisorError::Spawn(source) => BackendError::Unavailable {
            message: format!("{vendor} launch failed: {source}"),
            diagnostics,
        },
        SupervisorError::Unsupported => BackendError::Unavailable {
            message: "process supervision is unsupported on this platform".into(),
            diagnostics,
        },
        SupervisorError::NonzeroExit { exit_code, .. } => {
            let message = format!("{vendor} exited with status {exit_code}");
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
