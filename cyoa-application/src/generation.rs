//! Typed generation ports and synchronous use cases. Workers may run these on an
//! owned game snapshot; presentation retains ownership of canonical state.
use crate::cancellation::CancellationToken;
use cyoa_core::{
    game::{GameState, InvalidRewind, TurnCount},
    limits::Limits,
    text::{Brief, PlayerInput, RawResponse},
    turn::{GenerationProvenance, StoryTurn},
    world::{World, WorldCast, WorldOutline},
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Cancelled,
    Unavailable,
    Transport,
    InvalidResponse,
    Configuration,
}
#[derive(Debug, Error)]
#[error("{message}")]
pub struct GenerationFailure {
    kind: FailureKind,
    message: String,
    raw_response: RawResponse,
}
impl GenerationFailure {
    pub fn new(kind: FailureKind, message: impl Into<String>, raw_response: RawResponse) -> Self {
        Self {
            kind,
            message: message.into(),
            raw_response,
        }
    }
    pub fn kind(&self) -> FailureKind {
        self.kind
    }
    pub fn raw_response(&self) -> &RawResponse {
        &self.raw_response
    }
}
#[derive(Debug)]
pub struct Generated<T> {
    value: T,
    raw_response: RawResponse,
    provenance: GenerationProvenance,
}
impl<T> Generated<T> {
    pub fn new(value: T, raw_response: RawResponse, provenance: GenerationProvenance) -> Self {
        Self {
            value,
            raw_response,
            provenance,
        }
    }
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn raw_response(&self) -> &RawResponse {
        &self.raw_response
    }
    pub fn into_parts(self) -> (T, RawResponse, GenerationProvenance) {
        (self.value, self.raw_response, self.provenance)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnDirection {
    Continue,
    Player(PlayerInput),
    InterestingEvent,
}
impl TurnDirection {
    pub fn input(&self) -> Option<&PlayerInput> {
        match self {
            Self::Player(input) => Some(input),
            _ => None,
        }
    }
}
pub trait StoryGenerator {
    fn outline(
        &mut self,
        brief: &Brief,
        cancellation: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure>;
    fn cast(
        &mut self,
        brief: &Brief,
        outline: &WorldOutline,
        limits: &Limits,
        cancellation: &CancellationToken,
    ) -> Result<Generated<WorldCast>, GenerationFailure>;
    fn turn(
        &mut self,
        state: &GameState,
        direction: &TurnDirection,
        cancellation: &CancellationToken,
        on_narrative: &mut dyn FnMut(&str),
    ) -> Result<Generated<StoryTurn>, GenerationFailure>;
}
pub struct StoryUseCases<G> {
    generator: G,
}
impl<G: StoryGenerator> StoryUseCases<G> {
    pub fn new(generator: G) -> Self {
        Self { generator }
    }
    pub fn into_generator(self) -> G {
        self.generator
    }
    pub fn rewind(&mut self, state: &mut GameState, count: TurnCount) -> Result<(), InvalidRewind> {
        state.rewind(count)
    }
    pub fn generate_outline(
        &mut self,
        brief: &Brief,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        check_cancelled(cancel, RawResponse::new(""))?;
        let generated = self.generator.outline(brief, cancel)?;
        check_cancelled(cancel, generated.raw_response().clone())?;
        Ok(generated)
    }
    pub fn generate_world(
        &mut self,
        brief: &Brief,
        outline: WorldOutline,
        limits: &Limits,
        cancel: &CancellationToken,
    ) -> Result<World, GenerationFailure> {
        check_cancelled(cancel, RawResponse::new(""))?;
        let generated = self.generator.cast(brief, &outline, limits, cancel)?;
        check_cancelled(cancel, generated.raw_response().clone())?;
        Ok(World::new(outline, generated.into_parts().0))
    }
    pub fn take_turn(
        &mut self,
        state: &mut GameState,
        direction: TurnDirection,
        cancel: &CancellationToken,
        on_narrative: &mut dyn FnMut(&str),
    ) -> Result<(), GenerationFailure> {
        check_cancelled(cancel, RawResponse::new(""))?;
        let generated = self
            .generator
            .turn(state, &direction, cancel, on_narrative)?;
        check_cancelled(cancel, generated.raw_response().clone())?;
        let (turn, raw_response, provenance) = generated.into_parts();
        state.commit_turn(
            turn,
            cyoa_core::turn::TurnGenerationRecord {
                input: direction.input().cloned(),
                raw_response,
                provenance,
                prompt_trace: None,
            },
        );
        Ok(())
    }
}
fn check_cancelled(
    cancel: &CancellationToken,
    raw_response: RawResponse,
) -> Result<(), GenerationFailure> {
    if cancel.is_cancelled() {
        Err(GenerationFailure::new(
            FailureKind::Cancelled,
            "generation cancelled",
            raw_response,
        ))
    } else {
        Ok(())
    }
}
