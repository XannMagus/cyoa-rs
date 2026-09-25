//! Adapter from typed application generation ports to JSON transport. One call
//! per request; decoding and domain validation happen before returning success.
use super::{
    templates::{GenerationTemplates, RenderError, RenderedGeneration},
    wire::*,
};
use crate::backend::{Backend, BackendError, GenerationRequest, GenerationResponse};
use cyoa_application::{cancellation::CancellationToken, generation::*};
use cyoa_core::{
    game::GameState,
    limits::Limits,
    text::{Brief, RawResponse, TransportDiagnostics},
    turn::StoryTurn,
    world::{WorldCast, WorldOutline},
};

pub struct GenerationEngine<B> {
    backend: B,
    templates: GenerationTemplates,
}
impl<B> GenerationEngine<B> {
    pub fn new(backend: B, templates: GenerationTemplates) -> Self {
        Self { backend, templates }
    }
    pub fn into_backend(self) -> B {
        self.backend
    }
}
impl<B: Backend> GenerationEngine<B> {
    fn generate(
        &mut self,
        request: RenderedGeneration,
        cancel: &CancellationToken,
        on_json: &mut dyn FnMut(&str),
    ) -> Result<GenerationResponse, GenerationFailure> {
        if cancel.is_cancelled() {
            return Err(cancelled("", TransportDiagnostics::empty()));
        }
        let mut fragments = String::new();
        let response = self.backend.generate(
            GenerationRequest {
                instructions: request.instructions().as_str(),
                prompt: request.prompt().as_str(),
                schema: request.schema(),
            },
            cancel,
            &mut |chunk| {
                fragments.push_str(chunk);
                if !cancel.is_cancelled() {
                    on_json(chunk);
                }
            },
        );
        let response = response.map_err(|error| match error {
            BackendError::Cancelled { diagnostics } => cancelled(fragments, diagnostics),
            BackendError::Unavailable {
                message,
                diagnostics,
            } => GenerationFailure::new(
                FailureKind::Unavailable,
                message,
                RawResponse::new(fragments),
                diagnostics,
            ),
            BackendError::Timeout { diagnostics } => GenerationFailure::new(
                FailureKind::Timeout,
                "generation timed out",
                RawResponse::new(fragments),
                diagnostics,
            ),
            BackendError::Generation {
                message,
                raw_response,
                diagnostics,
            } => GenerationFailure::new(
                FailureKind::Transport,
                message,
                RawResponse::new(raw_response),
                diagnostics,
            ),
        })?;
        if cancel.is_cancelled() {
            // The transport succeeded, so there is no separate diagnostics
            // capture attached to a `GenerationResponse` yet (a known gap;
            // see this commit's report). Empty is honest, not fabricated.
            return Err(cancelled(
                response.raw_response(),
                TransportDiagnostics::empty(),
            ));
        }
        Ok(response)
    }
}
fn cancelled(raw: impl Into<String>, diagnostics: TransportDiagnostics) -> GenerationFailure {
    GenerationFailure::new(
        FailureKind::Cancelled,
        "generation cancelled",
        RawResponse::new(raw),
        diagnostics,
    )
}
fn configuration(error: RenderError) -> GenerationFailure {
    GenerationFailure::new(
        FailureKind::Configuration,
        error.to_string(),
        RawResponse::new(""),
        TransportDiagnostics::empty(),
    )
}
fn invalid(error: impl std::fmt::Display, response: &GenerationResponse) -> GenerationFailure {
    GenerationFailure::new(
        FailureKind::InvalidResponse,
        error.to_string(),
        RawResponse::new(response.raw_response()),
        TransportDiagnostics::empty(),
    )
}
fn decode<T: serde::de::DeserializeOwned>(
    response: &GenerationResponse,
) -> Result<T, GenerationFailure> {
    serde_json::from_value(response.value().clone()).map_err(|error| invalid(error, response))
}
fn generated<T>(value: T, response: &GenerationResponse) -> Generated<T> {
    Generated::new(
        value,
        RawResponse::new(response.raw_response()),
        Default::default(),
    )
}
impl<B: Backend> StoryGenerator for GenerationEngine<B> {
    fn outline(
        &mut self,
        brief: &Brief,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        let request = self.templates.world_request(brief).map_err(configuration)?;
        let response = self.generate(request, cancel, &mut |_| {})?;
        let wire: WorldOutlineWire = decode(&response)?;
        let world = WorldOutline::try_from(wire).map_err(|error| invalid(error, &response))?;
        Ok(generated(world, &response))
    }
    fn cast(
        &mut self,
        brief: &Brief,
        outline: &WorldOutline,
        limits: &Limits,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldCast>, GenerationFailure> {
        let request = self
            .templates
            .cast_request(brief, outline, limits)
            .map_err(configuration)?;
        let response = self.generate(request, cancel, &mut |_| {})?;
        let wire: GeneratedCastWire = decode(&response)?;
        let (players, npcs) = playable_and_npcs_from_wire(wire);
        let cast =
            WorldCast::new(players, npcs, limits).map_err(|error| invalid(error, &response))?;
        Ok(generated(cast, &response))
    }
    fn turn(
        &mut self,
        state: &GameState,
        direction: &TurnDirection,
        cancel: &CancellationToken,
        on_narrative: &mut dyn FnMut(&str),
    ) -> Result<Generated<StoryTurn>, GenerationFailure> {
        let request = self
            .templates
            .turn_request(
                state,
                direction.input().map(|i| i.as_str()),
                matches!(direction, TurnDirection::InterestingEvent),
            )
            .map_err(configuration)?;
        let mut scanner = super::stream::StreamingStringField::new("narrative");
        let mut preview = String::new();
        let response = self.generate(request, cancel, &mut |chunk| {
            let text = scanner.feed(chunk);
            if !text.is_empty() {
                preview.push_str(&text);
                on_narrative(&text);
            }
        })?;
        let wire: StoryTurnWire = decode(&response)?;
        let turn = StoryTurn::try_from(wire).map_err(|error| invalid(error, &response))?;
        if let Some(remainder) = turn.narrative().as_str().strip_prefix(&preview)
            && !remainder.is_empty()
        {
            on_narrative(remainder);
        }
        if cancel.is_cancelled() {
            return Err(cancelled(
                response.raw_response(),
                TransportDiagnostics::empty(),
            ));
        }
        Ok(generated(turn, &response))
    }
}
