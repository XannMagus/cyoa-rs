use super::*;
use crate::backend::{Backend, BackendError, GenerationRequest, GenerationResponse};
use crate::backends::process::{self, ProcessOutcome, SupervisorError};
use crate::backends::transport::{self, cancelled};

const VENDOR: &str = "Codex";
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
        transport::preflight_subscription(
            VENDOR,
            |workspace| ProcessSpec {
                workspace,
                program: config.executable.as_path().into(),
                args: vec!["login".into(), "status".into()],
                env: explicit_environment(&config),
                stdin: vec![],
                bounds: config.bounds,
            },
            subscription_auth,
            "use a ChatGPT login in the selected home",
            cancel,
        )?;
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
        .map_err(|e| transport::unavailable(format!("Codex request preparation: {e}")))?;
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
    let outcome =
        transport.map_err(|error| transport::transport_error(VENDOR, error, raw.clone()))?;
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

#[cfg(test)]
mod tests;
