//! The frozen CLI 2.1.286 `stream-json` profile (`reference/01-claude-cli.md`,
//! "Supported profile"). Protocol completion is not process success: the parent
//! adapter must still reconcile the supervisor's outcome before accepting it.
//!
//! The authoritative payload is `result.structured_output`, taken as its exact
//! span. Previews come only from `input_json_delta` events of the first
//! `StructuredOutput` block, identified by (message ordinal, block index) because
//! indices restart in every assistant message (a live capture contained a second
//! message after the CLI's enforce prompt). Text, thinking, advisor and other
//! tool blocks never become preview or payload.

use crate::backend::{TokenUsage, normalize_input_tokens};
use cyoa_core::{
    text::{CurrencyCode, ModelName, ProviderName},
    turn::{CostAmount, GenerationProvenance, ListPriceEstimate},
};
use serde::Deserialize;
use serde_json::{Value, value::RawValue};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Claude record {record} at {location}: {kind:?}")]
pub(super) struct ProtocolError {
    pub record: usize,
    pub location: String,
    pub kind: ErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ErrorKind {
    InvalidUtf8,
    InvalidJson,
    InvalidField,
    /// A record or stream event type outside the frozen profile.
    Unsupported,
    Order,
    /// Two `StructuredOutput` blocks in one assistant message.
    AmbiguousPayload,
    DuplicateResult,
    /// Two equal keys in an object that carries control fields (the record, `event`,
    /// `content_block` or `delta`): which value counts would be arbitrary.
    DuplicateField,
    /// `is_error`, a non-success subtype or a stream `error` event.
    VendorError,
    MissingPayload,
    InvalidPayload,
    /// `init.apiKeySource` was not the confirmed subscription value. A backstop
    /// that fires after the request is under way; the real guards are the auth
    /// preflight and the environment allowlist.
    MeteredAuth,
    Incomplete,
}

/// The exact `structured_output` span as the CLI serialized it into `result`.
#[derive(Debug, Clone)]
pub(super) struct Candidate {
    pub payload: String,
}

#[derive(Debug)]
pub(super) struct Completion {
    pub candidate: Candidate,
    pub usage: TokenUsage,
    pub provenance: GenerationProvenance,
}

#[derive(Debug)]
pub(super) struct Failure {
    pub error: ProtocolError,
    pub candidate: Option<Candidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MessageOrdinal(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BlockIndex(u32);

/// A block index is only meaningful inside its message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlockKey {
    message: MessageOrdinal,
    index: BlockIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Payload,
    Other,
}

/// Lifecycle of the preview: it follows the first payload block only and stops
/// for good at that block's stop (or the end of its message).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Preview {
    Unstarted,
    Open(BlockKey),
    Closed,
}

#[derive(Debug)]
struct Stream {
    model: Option<ModelName>,
    message: MessageOrdinal,
    blocks: HashMap<BlockIndex, BlockKind>,
    preview: Preview,
}

#[derive(Debug, Default)]
enum State {
    #[default]
    AwaitingInit,
    Streaming(Stream),
    Finished(Completion),
    Failed(Failure),
}

impl State {
    fn into_candidate(self) -> Option<Candidate> {
        match self {
            Self::Finished(done) => Some(done.candidate),
            Self::Failed(failure) => failure.candidate,
            _ => None,
        }
    }
}

struct Rejection {
    error: ProtocolError,
    candidate: Option<Candidate>,
}

#[derive(Debug, Default)]
pub(super) struct Protocol {
    state: State,
    records: usize,
}

impl Protocol {
    /// Consumes one complete record. A rejection latches the first failure:
    /// further records cannot repair it, and callers should stop delivery and
    /// let the supervisor clean up. `Ok(Some(_))` is a preview fragment of the
    /// correlated payload block, tentative and possibly different from the
    /// final payload.
    pub(crate) fn record(&mut self, record: &[u8]) -> Result<Option<String>, ProtocolError> {
        if let State::Failed(failure) = &self.state {
            return Err(failure.error.clone());
        }
        self.records += 1;
        match self.process(record) {
            Ok(preview) => Ok(preview),
            Err(rejection) => {
                let previous = std::mem::take(&mut self.state);
                let candidate = rejection.candidate.or_else(|| previous.into_candidate());
                self.state = State::Failed(Failure {
                    error: rejection.error.clone(),
                    candidate,
                });
                Err(rejection.error)
            }
        }
    }

    /// End-of-stream is mandatory. Returns protocol evidence only, never a
    /// `GenerationResponse`; process failure can still invalidate a completion.
    pub(crate) fn finish(self) -> Result<Completion, Failure> {
        match self.state {
            State::Finished(done) => Ok(done),
            State::Failed(failure) => Err(failure),
            _ => Err(Failure {
                error: error(self.records + 1, "$", ErrorKind::Incomplete),
                candidate: None,
            }),
        }
    }

    fn process(&mut self, bytes: &[u8]) -> Result<Option<String>, Rejection> {
        let n = self.records;
        let text =
            std::str::from_utf8(bytes).map_err(|_| reject(n, "$", ErrorKind::InvalidUtf8))?;
        let value: Value =
            serde_json::from_str(text).map_err(|_| reject(n, "$", ErrorKind::InvalidJson))?;
        if !value.is_object() {
            return Err(reject(n, "$", ErrorKind::InvalidField));
        }
        // `Value` keeps the last of two equal keys silently, so ambiguity must be
        // rejected before any control field is interpreted.
        if crate::json::has_duplicate_keys_in(text, is_control_object) {
            return Err(Rejection {
                error: error(n, "$", ErrorKind::DuplicateField),
                // Best effort: an unambiguous payload span is still audit evidence.
                candidate: candidate_of(text),
            });
        }
        let kind = str_field(&value, "type", n, "$.type")?;
        if let State::Finished(done) = &self.state {
            let kind = if kind == "result" {
                ErrorKind::DuplicateResult
            } else {
                ErrorKind::Order
            };
            return Err(Rejection {
                error: error(n, "$.type", kind),
                candidate: Some(done.candidate.clone()),
            });
        }
        match kind {
            "system" => self.system(&value, n).map(|()| None),
            "stream_event" => self.stream_event(&value, n),
            "assistant" | "user" => self.require_streaming(n).map(|_| None),
            // Identified harmless metadata, observed once or twice per stream.
            "rate_limit_event" => Ok(None),
            "result" => self.result(text, &value, n).map(|()| None),
            _ => Err(reject(n, "$.type", ErrorKind::Unsupported)),
        }
    }

    fn require_streaming(&mut self, n: usize) -> Result<&mut Stream, Rejection> {
        match &mut self.state {
            State::Streaming(stream) => Ok(stream),
            _ => Err(reject(n, "$.type", ErrorKind::Order)),
        }
    }

    fn system(&mut self, value: &Value, n: usize) -> Result<(), Rejection> {
        // Other subtypes (status, thinking_tokens, commands_changed, api_retry,
        // hook_*, ...) are metadata and may precede init; init itself is checked.
        if str_field(value, "subtype", n, "$.subtype")? != "init" {
            return Ok(());
        }
        if !matches!(self.state, State::AwaitingInit) {
            return Err(reject(n, "$.subtype", ErrorKind::Order));
        }
        if str_field(value, "apiKeySource", n, "$.apiKeySource")? != "none" {
            return Err(reject(n, "$.apiKeySource", ErrorKind::MeteredAuth));
        }
        self.state = State::Streaming(Stream {
            model: value
                .get("model")
                .and_then(Value::as_str)
                .and_then(|m| ModelName::new(m).ok()),
            message: MessageOrdinal(0),
            blocks: HashMap::new(),
            preview: Preview::Unstarted,
        });
        Ok(())
    }

    fn stream_event(&mut self, value: &Value, n: usize) -> Result<Option<String>, Rejection> {
        let stream = self.require_streaming(n)?;
        let event = value
            .get("event")
            .filter(|e| e.is_object())
            .ok_or_else(|| reject(n, "$.event", ErrorKind::InvalidField))?;
        match str_field(event, "type", n, "$.event.type")? {
            "message_start" => {
                stream.message = MessageOrdinal(stream.message.0 + 1);
                stream.blocks.clear();
                if matches!(stream.preview, Preview::Open(key) if key.message != stream.message) {
                    stream.preview = Preview::Closed;
                }
                Ok(None)
            }
            "content_block_start" => {
                let index = index_field(event, n)?;
                let block = event
                    .get("content_block")
                    .filter(|b| b.is_object())
                    .ok_or_else(|| reject(n, "$.event.content_block", ErrorKind::InvalidField))?;
                let block_type = str_field(block, "type", n, "$.event.content_block.type")?;
                if stream.blocks.contains_key(&index) {
                    return Err(reject(n, "$.event.index", ErrorKind::Order));
                }
                let payload = block_type == "tool_use"
                    && block.get("name").and_then(Value::as_str) == Some("StructuredOutput");
                if payload {
                    if stream.blocks.values().any(|k| *k == BlockKind::Payload) {
                        return Err(reject(
                            n,
                            "$.event.content_block",
                            ErrorKind::AmbiguousPayload,
                        ));
                    }
                    if stream.preview == Preview::Unstarted {
                        stream.preview = Preview::Open(BlockKey {
                            message: stream.message,
                            index,
                        });
                    }
                }
                let kind = if payload {
                    BlockKind::Payload
                } else {
                    BlockKind::Other
                };
                stream.blocks.insert(index, kind);
                Ok(None)
            }
            "content_block_delta" => {
                let index = index_field(event, n)?;
                let delta = event
                    .get("delta")
                    .filter(|d| d.is_object())
                    .ok_or_else(|| reject(n, "$.event.delta", ErrorKind::InvalidField))?;
                let delta_type = str_field(delta, "type", n, "$.event.delta.type")?;
                let kind = *stream
                    .blocks
                    .get(&index)
                    .ok_or_else(|| reject(n, "$.event.index", ErrorKind::Order))?;
                if delta_type != "input_json_delta" {
                    return Ok(None);
                }
                let fragment = delta
                    .get("partial_json")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        reject(n, "$.event.delta.partial_json", ErrorKind::InvalidField)
                    })?;
                let key = BlockKey {
                    message: stream.message,
                    index,
                };
                let correlated = kind == BlockKind::Payload && stream.preview == Preview::Open(key);
                Ok((correlated && !fragment.is_empty()).then(|| fragment.to_owned()))
            }
            "content_block_stop" => {
                let index = index_field(event, n)?;
                if !stream.blocks.contains_key(&index) {
                    return Err(reject(n, "$.event.index", ErrorKind::Order));
                }
                let key = BlockKey {
                    message: stream.message,
                    index,
                };
                if stream.preview == Preview::Open(key) {
                    stream.preview = Preview::Closed;
                }
                Ok(None)
            }
            "message_delta" | "message_stop" => Ok(None),
            // UNVERIFIED: from Anthropic's published Messages streaming docs; not
            // seen in any live capture. A ping carries nothing; an error is fatal.
            "ping" => Ok(None),
            "error" => Err(reject(n, "$.event.type", ErrorKind::VendorError)),
            _ => Err(reject(n, "$.event.type", ErrorKind::Unsupported)),
        }
    }

    fn result(&mut self, text: &str, value: &Value, n: usize) -> Result<(), Rejection> {
        let raw: RawResult =
            serde_json::from_str(text).map_err(|_| reject(n, "$", ErrorKind::InvalidJson))?;
        let candidate = candidate_from(raw.structured_output);
        let fail = |location: &str, kind: ErrorKind| Rejection {
            error: error(n, location, kind),
            candidate: candidate.clone(),
        };
        // The candidate span is extracted before the terminal metadata is
        // validated, so a malformed result still hands its exact payload to the
        // application as audit evidence (it can never authorize a commit).
        let is_error = value
            .get("is_error")
            .and_then(Value::as_bool)
            .ok_or_else(|| fail("$.is_error", ErrorKind::InvalidField))?;
        let subtype = value
            .get("subtype")
            .and_then(Value::as_str)
            .filter(|subtype| !subtype.trim().is_empty())
            .ok_or_else(|| fail("$.subtype", ErrorKind::InvalidField))?;
        // Neither a payload nor subtype "success" overrides is_error (the live
        // bad-model result carried subtype "success" with is_error true).
        if is_error {
            return Err(fail("$.is_error", ErrorKind::VendorError));
        }
        if subtype != "success" {
            return Err(fail("$.subtype", ErrorKind::VendorError));
        }
        let State::Streaming(stream) = &self.state else {
            return Err(fail("$.type", ErrorKind::Order));
        };
        let Some(candidate) = candidate.clone() else {
            return Err(fail("$.structured_output", ErrorKind::MissingPayload));
        };
        if crate::json::has_duplicate_keys_in(&candidate.payload, |_| true) {
            return Err(fail("$.structured_output", ErrorKind::InvalidPayload));
        }
        let provenance = provenance(value, stream.model.as_ref());
        self.state = State::Finished(Completion {
            candidate,
            usage: token_usage(value.get("usage")),
            provenance,
        });
        Ok(())
    }
}

#[derive(Deserialize)]
struct RawResult<'a> {
    #[serde(borrow)]
    structured_output: Option<&'a RawValue>,
}

/// Only an object can be a candidate; a null or absent value is "missing".
fn candidate_from(span: Option<&RawValue>) -> Option<Candidate> {
    span.map(RawValue::get)
        .filter(|span| span.starts_with('{'))
        .map(|span| Candidate {
            payload: span.to_owned(),
        })
}

/// The exact span of an unambiguous `structured_output`, if the record has one.
fn candidate_of(text: &str) -> Option<Candidate> {
    candidate_from(
        serde_json::from_str::<RawResult>(text)
            .ok()?
            .structured_output,
    )
}

fn error(record: usize, location: &str, kind: ErrorKind) -> ProtocolError {
    ProtocolError {
        record,
        location: location.into(),
        kind,
    }
}

fn reject(record: usize, location: &str, kind: ErrorKind) -> Rejection {
    Rejection {
        error: error(record, location, kind),
        candidate: None,
    }
}

fn str_field<'a>(
    value: &'a Value,
    key: &str,
    record: usize,
    location: &str,
) -> Result<&'a str, Rejection> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| reject(record, location, ErrorKind::InvalidField))
}

fn index_field(event: &Value, record: usize) -> Result<BlockIndex, Rejection> {
    event
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|i| u32::try_from(i).ok())
        .map(BlockIndex)
        .ok_or_else(|| reject(record, "$.event.index", ErrorKind::InvalidField))
}

/// Input total counts everything sent (`input_tokens` is only the uncached,
/// non-cache-writing remainder); cached is the cache-read part. Known only when
/// all three counts are present. Output is independent. Missing or malformed
/// counts are unknown, never zero.
fn token_usage(usage: Option<&Value>) -> TokenUsage {
    let count = |key: &str| usage.and_then(|u| u.get(key)).and_then(Value::as_u64);
    let input = match (
        count("input_tokens"),
        count("cache_creation_input_tokens"),
        count("cache_read_input_tokens"),
    ) {
        (Some(input), Some(created), Some(read)) => input
            .checked_add(created)
            .and_then(|sum| sum.checked_add(read))
            .map(|total| normalize_input_tokens(total, Some(read))),
        _ => None,
    };
    TokenUsage {
        input,
        output: count("output_tokens"),
    }
}

/// Observed, never configured: the model is `init.model` (the starting model;
/// `modelUsage` keys are the record of every model used), the provider is what
/// `modelUsage` reports for that model, and a cost exists only when every
/// `modelUsage` entry says `costBasis: "list"`. `total_cost_usd` is a client
/// list-price estimate, not subscription spending.
fn provenance(result: &Value, model: Option<&ModelName>) -> GenerationProvenance {
    let usage = result.get("modelUsage").and_then(Value::as_object);
    let provider = model
        .and_then(|m| usage?.get(m.as_str()))
        .and_then(|entry| entry.get("provider"))
        .and_then(Value::as_str)
        .and_then(|p| ProviderName::new(p).ok());
    let all_list = usage.is_some_and(|entries| {
        !entries.is_empty()
            && entries
                .values()
                .all(|e| e.get("costBasis").and_then(Value::as_str) == Some("list"))
    });
    let cost = all_list
        .then(|| result.get("total_cost_usd").and_then(Value::as_f64))
        .flatten()
        .and_then(|amount| CostAmount::new(amount).ok())
        .map(|amount| {
            ListPriceEstimate::new(amount, CurrencyCode::new("USD").expect("USD is nonblank"))
        });
    GenerationProvenance {
        provider,
        model: model.cloned(),
        cost,
    }
}

/// Objects that carry control fields (`type`, `subtype`, `is_error`, `apiKeySource`,
/// `event.type`/`index`, `content_block.type`/`name`, `delta.type`/`partial_json`).
/// Model-authored content (`message`, tool inputs, `structured_output`) is not
/// control data and is deliberately outside this scope.
fn is_control_object(path: &[String]) -> bool {
    matches!(
        path.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        [] | ["event"] | ["event", "content_block"] | ["event", "delta"]
    )
}

#[cfg(test)]
mod tests;
