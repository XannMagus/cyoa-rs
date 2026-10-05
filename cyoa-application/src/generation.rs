//! Typed generation ports and synchronous use cases. Workers may run these on an
//! owned game snapshot; presentation retains ownership of canonical state.
use crate::cancellation::CancellationToken;
use crate::diagnostics::TransportDiagnostics;
use cyoa_core::{
    game::GameState,
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
    Timeout,
    Transport,
    InvalidResponse,
    Configuration,
}
#[derive(Debug, Error)]
#[error("{message}")]
pub struct GenerationFailure {
    kind: FailureKind,
    message: String,
    /// What the backend returned, exactly; `None` when no backend call was made
    /// (for example, cancelled before dispatch), never an empty-string stand-in.
    raw_response: Option<RawResponse>,
    diagnostics: TransportDiagnostics,
}
impl GenerationFailure {
    pub fn new(
        kind: FailureKind,
        message: impl Into<String>,
        raw_response: Option<RawResponse>,
        diagnostics: TransportDiagnostics,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            raw_response,
            diagnostics,
        }
    }
    pub fn kind(&self) -> FailureKind {
        self.kind
    }
    pub fn raw_response(&self) -> Option<&RawResponse> {
        self.raw_response.as_ref()
    }
    pub fn diagnostics(&self) -> &TransportDiagnostics {
        &self.diagnostics
    }
}
#[derive(Debug)]
pub struct Generated<T> {
    value: T,
    raw_response: RawResponse,
    provenance: GenerationProvenance,
    diagnostics: TransportDiagnostics,
}
impl<T> Generated<T> {
    pub fn new(value: T, raw_response: RawResponse, provenance: GenerationProvenance) -> Self {
        Self {
            value,
            raw_response,
            provenance,
            diagnostics: TransportDiagnostics::empty(),
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
    pub fn with_diagnostics(mut self, diagnostics: TransportDiagnostics) -> Self {
        self.diagnostics = diagnostics;
        self
    }
    pub fn diagnostics(&self) -> &TransportDiagnostics {
        &self.diagnostics
    }
    fn check_cancelled(&self, cancel: &CancellationToken) -> Result<(), GenerationFailure> {
        if cancel.is_cancelled() {
            Err(GenerationFailure::new(
                FailureKind::Cancelled,
                "generation cancelled",
                Some(self.raw_response.clone()),
                self.diagnostics.clone(),
            ))
        } else {
            Ok(())
        }
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
    pub fn generate_outline(
        &mut self,
        brief: &Brief,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        check_cancelled(cancel)?;
        let generated = self.generator.outline(brief, cancel)?;
        generated.check_cancelled(cancel)?;
        Ok(generated)
    }
    pub fn generate_world(
        &mut self,
        brief: &Brief,
        outline: WorldOutline,
        limits: &Limits,
        cancel: &CancellationToken,
    ) -> Result<World, GenerationFailure> {
        check_cancelled(cancel)?;
        let generated = self.generator.cast(brief, &outline, limits, cancel)?;
        generated.check_cancelled(cancel)?;
        Ok(World::new(outline, generated.into_parts().0))
    }
    /// Consumes the game and returns it with one more committed turn. On any
    /// failure the game comes back unchanged inside [`TurnFailure`].
    pub fn take_turn(
        &mut self,
        mut state: GameState,
        direction: TurnDirection,
        cancel: &CancellationToken,
        on_narrative: &mut dyn FnMut(&str),
    ) -> Result<GameState, Box<TurnFailure>> {
        let generated = match check_cancelled(cancel)
            .and_then(|()| {
                self.generator
                    .turn(&state, &direction, cancel, on_narrative)
            })
            .and_then(|generated| generated.check_cancelled(cancel).map(|()| generated))
        {
            Ok(generated) => generated,
            Err(failure) => return Err(Box::new(TurnFailure { state, failure })),
        };
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
        Ok(state)
    }
}
/// A failed turn: the unchanged game, returned to its owner, and why it failed.
#[derive(Debug, Error)]
#[error("{failure}")]
pub struct TurnFailure {
    pub state: GameState,
    #[source]
    pub failure: GenerationFailure,
}
/// Cancellation before dispatch: no backend call was made, so there is no response.
fn check_cancelled(cancel: &CancellationToken) -> Result<(), GenerationFailure> {
    if cancel.is_cancelled() {
        Err(GenerationFailure::new(
            FailureKind::Cancelled,
            "generation cancelled",
            None,
            TransportDiagnostics::empty(),
        ))
    } else {
        Ok(())
    }
}
