use super::*;
use crate::backend::{Backend, BackendError, GenerationRequest, GenerationResponse};
use crate::backends::process::{self, ProcessOutcome, SupervisorError};
use crate::backends::transport::{self, cancelled};

const VENDOR: &str = "Claude";
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
        transport::preflight_subscription(
            VENDOR,
            |workspace| ProcessSpec {
                workspace,
                program: config.executable.as_path().into(),
                args: ["auth", "status", "--json"].map(OsString::from).into(),
                env: explicit_environment(&config),
                stdin: vec![],
                bounds: config.bounds,
            },
            subscription_auth,
            "use a claude.ai login in the selected home",
            cancel,
        )?;
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
        .map_err(|e| transport::unavailable(format!("Claude request preparation: {e}")))?;
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
    let Ok(status) = serde_json::from_slice::<Value>(diagnostics.stdout()) else {
        return false;
    };
    status.get("loggedIn").and_then(Value::as_bool) == Some(true)
        && status.get("authMethod").and_then(Value::as_str) == Some("claude.ai")
        && status.get("apiProvider").and_then(Value::as_str) == Some("firstParty")
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
    let outcome =
        transport.map_err(|error| transport::transport_error(VENDOR, error, candidate.clone()))?;
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
