//! Deterministic, credential-free transport for integration tests and future demos.
use crate::backend::*;
use cyoa_application::cancellation::CancellationToken;
use cyoa_core::text::TransportDiagnostics;
use std::collections::VecDeque;
#[derive(Debug)]
pub struct CapturedRequest {
    instructions: String,
    prompt: String,
    schema: serde_json::Value,
}
impl CapturedRequest {
    pub fn instructions(&self) -> &str {
        &self.instructions
    }
    pub fn prompt(&self) -> &str {
        &self.prompt
    }
    pub fn schema(&self) -> &serde_json::Value {
        &self.schema
    }
}
pub struct ScriptedBackend {
    responses: VecDeque<Result<String, BackendError>>,
    requests: Vec<CapturedRequest>,
}
impl ScriptedBackend {
    pub fn new(responses: impl IntoIterator<Item = Result<String, BackendError>>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
            requests: vec![],
        }
    }
    pub fn requests(&self) -> &[CapturedRequest] {
        &self.requests
    }
}
impl Backend for ScriptedBackend {
    fn generate(
        &mut self,
        r: GenerationRequest<'_>,
        cancel: &CancellationToken,
        _: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        if cancel.is_cancelled() {
            return Err(BackendError::Cancelled {
                diagnostics: TransportDiagnostics::empty(),
            });
        }
        self.requests.push(CapturedRequest {
            instructions: r.instructions.into(),
            prompt: r.prompt.into(),
            schema: r.schema.clone(),
        });
        let raw = self
            .responses
            .pop_front()
            .ok_or_else(|| BackendError::Unavailable {
                message: "script exhausted: unexpected generation call".into(),
                diagnostics: TransportDiagnostics::empty(),
            })??;
        GenerationResponse::from_json(raw, TokenUsage::default())
    }
}

/// Deterministic scalar-sized replay of a scripted response.
pub struct ChunkedBackend {
    inner: ScriptedBackend,
    seed: u64,
}
impl ChunkedBackend {
    pub fn new(
        responses: impl IntoIterator<Item = Result<String, BackendError>>,
        seed: u64,
    ) -> Self {
        Self {
            inner: ScriptedBackend::new(responses),
            seed,
        }
    }
    pub fn requests(&self) -> &[CapturedRequest] {
        self.inner.requests()
    }
}
impl Backend for ChunkedBackend {
    fn generate(
        &mut self,
        r: GenerationRequest<'_>,
        cancel: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError> {
        let result = self.inner.generate(r, cancel, &mut |_| {});
        let raw = match &result {
            Ok(response) => response.raw_response(),
            Err(BackendError::Generation { raw_response, .. }) => raw_response.as_str(),
            _ => return result,
        };
        let boundaries = raw
            .char_indices()
            .map(|(index, _)| index)
            .chain([raw.len()])
            .collect::<Vec<_>>();
        let mut start = 0;
        while start + 1 < boundaries.len() {
            if cancel.is_cancelled() {
                return Err(BackendError::Cancelled {
                    diagnostics: TransportDiagnostics::empty(),
                });
            }
            self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let end = (start + 1 + (self.seed as usize % 17)).min(boundaries.len() - 1);
            on_json(&raw[boundaries[start]..boundaries[end]]);
            start = end;
        }
        if cancel.is_cancelled() {
            return Err(BackendError::Cancelled {
                diagnostics: TransportDiagnostics::empty(),
            });
        }
        result
    }
}
