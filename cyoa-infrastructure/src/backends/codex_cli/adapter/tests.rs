use super::*;
use crate::backends::process::{IoFailure, IoOperation, OutputStream};
use cyoa_application::{cancellation::CancellationSource, diagnostics::CapturedBytes};

fn protocol_done() -> Result<protocol::Completion, protocol::Failure> {
    let mut decoder = protocol::Protocol::default();
    for record in include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../reviews/2026-09-26-codex-profile/synthetic/success.jsonl"
    ))
    .split(|b| *b == b'\n')
    .filter(|b| !b.is_empty())
    {
        decoder.record(record).unwrap();
    }
    decoder.finish()
}
fn diagnostics() -> TransportDiagnostics {
    TransportDiagnostics::from_captures(
        CapturedBytes::prefix(b"secret diagnostic\xff".to_vec()),
        CapturedBytes::complete(b"stderr\r\n".to_vec()),
    )
}
fn io(operation: IoOperation) -> IoFailure {
    IoFailure {
        operation,
        source: std::io::Error::other("injected fault"),
    }
}

#[test]
fn nested_cleanup_retains_initiating_failure_payload_and_prefix_without_displaying_buffers() {
    let diagnostic = diagnostics();
    let failure = SupervisorError::Cleanup {
        initial: Some(Box::new(SupervisorError::Cleanup {
            initial: Some(Box::new(SupervisorError::Cancelled {
                diagnostics: diagnostic.clone(),
            })),
            failures: vec![io(IoOperation::KillGroup)],
            diagnostics: diagnostic.clone(),
        })),
        failures: vec![io(IoOperation::Reap), io(IoOperation::WorkspaceCleanup)],
        diagnostics: diagnostic.clone(),
    };
    let error = reconcile(
        Err(failure),
        protocol_done(),
        &CancellationSource::default().token(),
        &mut |_| panic!("no emission"),
    )
    .unwrap_err();
    assert!(!error.to_string().contains("secret"));
    for text in ["cancelled", "KillGroup", "Reap", "WorkspaceCleanup"] {
        assert!(error.to_string().contains(text));
    }
    let BackendError::Transport {
        raw_response,
        diagnostics,
        cause,
        ..
    } = error
    else {
        panic!("cleanup must remain visible")
    };
    assert!(raw_response.contains("Harbour"));
    assert_eq!(*diagnostics, diagnostic);
    let SupervisorError::Cleanup {
        initial: Some(initial),
        failures,
        ..
    } = cause.downcast_ref::<SupervisorError>().unwrap()
    else {
        panic!("typed cause")
    };
    assert_eq!(failures.len(), 2);
    let SupervisorError::Cleanup {
        initial: Some(initial),
        ..
    } = initial.as_ref()
    else {
        panic!("nested cleanup")
    };
    assert!(matches!(
        initial.as_ref(),
        SupervisorError::Cancelled { .. }
    ));
}

#[test]
fn every_transport_failure_category_remains_distinct_and_never_emits() {
    let d = diagnostics();
    let cases = [
        SupervisorError::Io {
            failure: io(IoOperation::Read),
            diagnostics: d.clone(),
        },
        SupervisorError::IncompleteInput {
            written: 2,
            expected: 100,
            diagnostics: d.clone(),
        },
        SupervisorError::OutputBoundExceeded {
            stream: OutputStream::Stderr,
            diagnostics: d.clone(),
        },
        SupervisorError::Cleanup {
            initial: None,
            failures: vec![io(IoOperation::Reap)],
            diagnostics: d.clone(),
        },
    ];
    for failure in cases {
        let expected = std::mem::discriminant(&failure);
        let error = reconcile(
            Err(failure),
            protocol_done(),
            &CancellationSource::default().token(),
            &mut |_| panic!("no emission"),
        )
        .unwrap_err();
        let BackendError::Transport {
            cause,
            raw_response,
            diagnostics,
            ..
        } = error
        else {
            panic!("typed transport error")
        };
        assert_eq!(
            std::mem::discriminant(cause.downcast_ref::<SupervisorError>().unwrap()),
            expected
        );
        assert_eq!(*diagnostics, d);
        assert!(raw_response.contains("Harbour"));
    }
    for timeout in [false, true] {
        let failure = if timeout {
            SupervisorError::Timeout {
                diagnostics: d.clone(),
            }
        } else {
            SupervisorError::Cancelled {
                diagnostics: d.clone(),
            }
        };
        let error = reconcile(
            Err(failure),
            protocol_done(),
            &CancellationSource::default().token(),
            &mut |_| panic!("no emission"),
        )
        .unwrap_err();
        let (raw, diagnostics) = match error {
            BackendError::Timeout {
                raw_response,
                diagnostics,
            } if timeout => (raw_response, diagnostics),
            BackendError::Cancelled {
                raw_response,
                diagnostics,
            } if !timeout => (raw_response, diagnostics),
            _ => panic!("wrong category"),
        };
        assert!(raw.unwrap().contains("Harbour"));
        assert_eq!(diagnostics, d);
    }
    assert!(matches!(
        transport_error(SupervisorError::Unsupported, None),
        BackendError::Unavailable { .. }
    ));
}

#[test]
fn cancellation_at_acceptance_overrides_protocol_completion_without_emitting() {
    let source = CancellationSource::default();
    source.cancel();
    let diagnostic = diagnostics();
    let error = reconcile(
        Ok(ProcessOutcome {
            exit_code: 0,
            diagnostics: diagnostic.clone(),
        }),
        protocol_done(),
        &source.token(),
        &mut |_| panic!("no emission"),
    )
    .unwrap_err();
    let BackendError::Cancelled {
        raw_response,
        diagnostics,
    } = error
    else {
        panic!("must cancel")
    };
    assert!(raw_response.unwrap().contains("Harbour"));
    assert_eq!(diagnostics, diagnostic);
}
