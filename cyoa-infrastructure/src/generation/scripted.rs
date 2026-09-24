//! Deterministic, credential-free transport for integration tests and future demos.
use crate::backend::*;
use cyoa_application::cancellation::CancellationToken;
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
            return Err(BackendError::Cancelled);
        }
        self.requests.push(CapturedRequest {
            instructions: r.instructions.into(),
            prompt: r.prompt.into(),
            schema: r.schema.clone(),
        });
        let raw = self.responses.pop_front().ok_or_else(|| {
            BackendError::Unavailable("script exhausted: unexpected generation call".into())
        })??;
        GenerationResponse::from_json(raw, TokenUsage::default())
    }
}
