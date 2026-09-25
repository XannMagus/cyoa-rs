//! Low-level JSON transport contract shared by inference adapters.
//!
//! This is not an application port. Future story-generation adapters translate
//! domain-facing requests and results through this internal transport boundary.

use cyoa_application::cancellation::CancellationToken;
use cyoa_core::text::TransportDiagnostics;

use serde_json::Value;
use thiserror::Error;

/// Borrowed inputs, valid for the duration of one generation call.
#[derive(Debug)]
pub struct GenerationRequest<'a> {
    pub instructions: &'a str,
    pub prompt: &'a str,
    pub schema: &'a Value,
}

/// A syntactically valid structured response, still requiring game validation.
///
/// The parsed value and original JSON cannot be supplied or mutated independently.
/// CLI event envelopes belong in adapter diagnostics; `raw_response` is the actual
/// structured payload extracted from those envelopes.
///
/// ```compile_fail
/// use cyoa_infrastructure::backend::{GenerationResponse, TokenUsage};
/// let mut response = GenerationResponse::from_json("{}".into(), TokenUsage::default()).unwrap();
/// response.value = serde_json::json!({"different": true});
/// ```
#[derive(Debug)]
pub struct GenerationResponse {
    value: Value,
    raw_response: String,
    usage: TokenUsage,
}

impl GenerationResponse {
    pub fn from_json(raw_response: String, usage: TokenUsage) -> Result<Self, BackendError> {
        let value =
            serde_json::from_str(&raw_response).map_err(|error| BackendError::Generation {
                message: format!("invalid structured response: {error}"),
                raw_response: raw_response.clone(),
                diagnostics: TransportDiagnostics::empty(),
            })?;
        Ok(Self {
            value,
            raw_response,
            usage,
        })
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn raw_response(&self) -> &str {
        &self.raw_response
    }

    pub fn usage(&self) -> TokenUsage {
        self.usage
    }
}

/// Input counts normalized so cached tokens are a subset of total input tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputTokens {
    total: u64,
    cached: Option<u64>,
}

impl InputTokens {
    pub fn new(total: u64, cached: Option<u64>) -> Result<Self, InvalidTokenUsage> {
        if let Some(cached) = cached
            && cached > total
        {
            return Err(InvalidTokenUsage { total, cached });
        }
        Ok(Self { total, cached })
    }

    pub fn total(self) -> u64 {
        self.total
    }

    pub fn cached(self) -> Option<u64> {
        self.cached
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("cached input tokens ({cached}) exceed total input tokens ({total})")]
pub struct InvalidTokenUsage {
    total: u64,
    cached: u64,
}

/// Normalizes a vendor-reported cached-token count that contradicts the total.
///
/// A backend's own miscounted telemetry is not evidence the generation itself
/// failed: invalid cache accounting is treated as unknown cache information
/// (`cached: None`), not as a rejected response. `InputTokens::new` keeps
/// rejecting the invalid pair for direct, already-validated construction;
/// this normalizer is the boundary policy for raw vendor-reported counts.
pub fn normalize_input_tokens(total: u64, cached: Option<u64>) -> InputTokens {
    InputTokens::new(total, cached)
        .or_else(|_| InputTokens::new(total, None))
        .expect("total alone is always valid")
}

/// Missing counts differ from reported zero. There is one representation of
/// unavailable usage: `TokenUsage::default()`, without an outer `Option`.
/// Adapters must normalize provider-specific accounting before constructing it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: Option<InputTokens>,
    pub output: Option<u64>,
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("generation cancelled")]
    Cancelled { diagnostics: TransportDiagnostics },
    #[error("backend unavailable: {message}")]
    Unavailable {
        message: String,
        diagnostics: TransportDiagnostics,
    },
    #[error("generation timed out")]
    Timeout { diagnostics: TransportDiagnostics },
    #[error("generation failed: {message}")]
    Generation {
        message: String,
        /// Preserve diagnostics for the frontend's error details.
        raw_response: String,
        diagnostics: TransportDiagnostics,
    },
}

/// Implementations are movable to a worker thread, with exclusive access per call.
///
/// `on_json` receives raw JSON fragments, not decoded narrative text. Backends
/// without incremental output may emit the complete JSON once. The returned value
/// is authoritative; partial output must never be committed to game state.
/// Implementations return errors rather than retrying automatically.
pub trait Backend: Send {
    fn generate(
        &mut self,
        request: GenerationRequest<'_>,
        cancellation: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, BackendError>;
}
