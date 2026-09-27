//! The frozen 0.157.1 complete-only profile. Protocol completion is not process
//! success: the parent adapter must still reconcile the supervisor's outcome.

use crate::backend::{TokenUsage, normalize_input_tokens};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Codex record {record} at {location}: {kind:?}")]
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
    Unsupported,
    Order,
    MultipleCandidates,
    DuplicateTerminal,
    ConflictingTerminal,
    VendorFailure,
    ErrorNotice,
    Incomplete,
    InvalidPayload,
}

#[derive(Debug)]
pub(super) struct Candidate {
    pub item_id: String,
    pub payload: String,
    record: usize,
}

#[derive(Debug)]
pub(super) struct Completion {
    pub candidate: Candidate,
    pub usage: TokenUsage,
}

#[derive(Debug)]
pub(super) struct Failure {
    pub error: ProtocolError,
    pub candidate: Option<Candidate>,
}

#[derive(Debug, Default)]
enum State {
    #[default]
    AwaitingThread,
    AwaitingTurn,
    AwaitingCandidate,
    Candidate(Candidate),
    Completed(Completion),
    Failed(Failure),
}

#[derive(Debug, Default)]
pub(super) struct Protocol {
    state: State,
    records: usize,
}

impl Protocol {
    /// A rejected record latches the first failure. Further records cannot repair
    /// it; callers should stop delivery and ask the supervisor to clean up.
    pub fn record(&mut self, record: &[u8]) -> Result<(), ProtocolError> {
        if let State::Failed(failure) = &self.state {
            return Err(failure.error.clone());
        }
        self.records += 1;
        let result = parse_event(record, self.records).and_then(|event| self.advance(event));
        if let Err(error) = &result {
            let previous = std::mem::take(&mut self.state);
            self.state = State::Failed(Failure {
                error: error.clone(),
                candidate: previous.into_candidate(),
            });
        }
        result
    }

    fn advance(&mut self, event: Event) -> Result<(), ProtocolError> {
        let kind = match (&self.state, &event) {
            (State::Completed(_), Event::Completed(_)) => Some(ErrorKind::DuplicateTerminal),
            (State::Completed(_), Event::Failed) => Some(ErrorKind::ConflictingTerminal),
            (_, Event::Failed) => Some(ErrorKind::VendorFailure),
            (_, Event::ErrorNotice) => Some(ErrorKind::ErrorNotice),
            (State::Candidate(_), Event::Message(_)) => Some(ErrorKind::MultipleCandidates),
            (State::AwaitingThread, Event::Thread)
            | (State::AwaitingTurn, Event::Turn)
            | (State::AwaitingCandidate, Event::Message(_))
            | (State::Candidate(_), Event::Completed(_)) => None,
            _ => Some(ErrorKind::Order),
        };
        if let Some(kind) = kind {
            return Err(error(self.records, "$.type", kind));
        }
        self.state = match event {
            Event::Thread => State::AwaitingTurn,
            Event::Turn => State::AwaitingCandidate,
            Event::Message(candidate) => State::Candidate(candidate),
            Event::Completed(usage) => {
                let State::Candidate(candidate) = std::mem::take(&mut self.state) else {
                    unreachable!("transition checked above")
                };
                State::Completed(Completion { candidate, usage })
            }
            Event::Failed | Event::ErrorNotice => unreachable!("failure returned above"),
        };
        Ok(())
    }

    /// End-of-stream is mandatory. This returns protocol evidence only, never a
    /// GenerationResponse. Process failure can still invalidate a Completion.
    pub fn finish(self) -> Result<Completion, Failure> {
        match self.state {
            State::Completed(completion) => {
                // Delay syntax validation until the transcript is complete:
                // commentary followed by JSON is ambiguous, not a repair path.
                if serde_json::from_str::<serde_json::Value>(&completion.candidate.payload).is_err()
                {
                    return Err(Failure {
                        error: error(
                            completion.candidate.record,
                            "$.item.text",
                            ErrorKind::InvalidPayload,
                        ),
                        candidate: Some(completion.candidate),
                    });
                }
                Ok(completion)
            }
            State::Failed(failure) => Err(failure),
            state => Err(Failure {
                error: error(self.records + 1, "$", ErrorKind::Incomplete),
                candidate: state.into_candidate(),
            }),
        }
    }
}

impl State {
    fn into_candidate(self) -> Option<Candidate> {
        match self {
            Self::Candidate(candidate) | Self::Completed(Completion { candidate, .. }) => {
                Some(candidate)
            }
            Self::Failed(failure) => failure.candidate,
            _ => None,
        }
    }
}

#[derive(Debug)]
enum Event {
    Thread,
    Turn,
    Message(Candidate),
    Completed(TokenUsage),
    Failed,
    ErrorNotice,
}

fn error(record: usize, location: &str, kind: ErrorKind) -> ProtocolError {
    ProtocolError {
        record,
        location: location.into(),
        kind,
    }
}

fn string_field<'a>(
    value: &'a serde_json::Value,
    key: &str,
    record: usize,
    location: &str,
    nonblank: bool,
) -> Result<&'a str, ProtocolError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|text| !nonblank || !text.trim().is_empty())
        .ok_or_else(|| error(record, location, ErrorKind::InvalidField))
}

fn parse_event(bytes: &[u8], record: usize) -> Result<Event, ProtocolError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| error(record, "$", ErrorKind::InvalidUtf8))?;
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| error(record, "$", ErrorKind::InvalidJson))?;
    if !value.is_object() {
        return Err(error(record, "$", ErrorKind::InvalidField));
    }
    let kind = string_field(&value, "type", record, "$.type", true)?;
    match kind {
        "thread.started" => {
            string_field(&value, "thread_id", record, "$.thread_id", true)?;
            Ok(Event::Thread)
        }
        "turn.started" => Ok(Event::Turn),
        "item.completed" => {
            let item = value
                .get("item")
                .filter(|v| v.is_object())
                .ok_or_else(|| error(record, "$.item", ErrorKind::InvalidField))?;
            let id = string_field(item, "id", record, "$.item.id", true)?;
            let kind = string_field(item, "type", record, "$.item.type", true)?;
            if kind != "agent_message" {
                return Err(error(record, "$.item.type", ErrorKind::Unsupported));
            }
            let text = string_field(item, "text", record, "$.item.text", false)?;
            Ok(Event::Message(Candidate {
                item_id: id.into(),
                payload: text.into(),
                record,
            }))
        }
        "turn.completed" => Ok(Event::Completed(usage(&value["usage"]))),
        "turn.failed" => {
            let failure = value
                .get("error")
                .filter(|v| v.is_object())
                .ok_or_else(|| error(record, "$.error", ErrorKind::InvalidField))?;
            string_field(failure, "message", record, "$.error.message", false)?;
            Ok(Event::Failed)
        }
        "error" => {
            string_field(&value, "message", record, "$.message", false)?;
            Ok(Event::ErrorNotice)
        }
        _ => Err(error(record, "$.type", ErrorKind::Unsupported)),
    }
}

// Malformed/missing counts are unknown independently. Auxiliary counts are not
// added to totals; no model identity or cost is established by this profile.
fn usage(value: &serde_json::Value) -> TokenUsage {
    TokenUsage {
        input: value
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64)
            .map(|total| {
                normalize_input_tokens(
                    total,
                    value
                        .get("cached_input_tokens")
                        .and_then(serde_json::Value::as_u64),
                )
            }),
        output: value
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64),
    }
}

#[cfg(test)]
mod tests;
