//! Typed generation ports and synchronous use cases. Workers may run these on an
//! owned game snapshot; the application session retains canonical state.
use crate::cancellation::CancellationToken;
use crate::diagnostics::TransportDiagnostics;
use cyoa_core::{
    game::GameState,
    limits::Limits,
    text::{Brief, RawResponse},
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
pub use cyoa_core::generation::{StoryGenerator as DomainStoryGenerator, TurnDirection};

impl cyoa_core::generation::GenerationControl for CancellationToken {
    fn is_cancelled(&self) -> bool {
        self.is_cancelled()
    }
}
impl<T> cyoa_core::generation::GenerationReport<T> for Generated<T> {
    fn value(&self) -> &T {
        self.value()
    }
    fn into_parts(self) -> (T, RawResponse, GenerationProvenance) {
        self.into_parts()
    }
}

/// Binds the inward business capability to this application's execution report.
pub trait StoryGenerator:
    DomainStoryGenerator<
        Context = CancellationToken,
        Failure = GenerationFailure,
        Report<WorldOutline> = Generated<WorldOutline>,
        Report<WorldCast> = Generated<WorldCast>,
        Report<StoryTurn> = Generated<StoryTurn>,
    >
{
}
impl<T> StoryGenerator for T where
    T: DomainStoryGenerator<
            Context = CancellationToken,
            Failure = GenerationFailure,
            Report<WorldOutline> = Generated<WorldOutline>,
            Report<WorldCast> = Generated<WorldCast>,
            Report<StoryTurn> = Generated<StoryTurn>,
        >
{
}
/// Execution evidence stays outside the saved story and retains exact bytes.
#[derive(Debug, Clone)]
pub struct GenerationEvidence {
    pub raw_response: Option<RawResponse>,
    pub provenance: Option<GenerationProvenance>,
    pub diagnostics: TransportDiagnostics,
}
impl GenerationEvidence {
    fn capture<T>(result: &Result<Generated<T>, GenerationFailure>) -> Self {
        match result {
            Ok(report) => Self {
                raw_response: Some(report.raw_response.clone()),
                provenance: Some(report.provenance.clone()),
                diagnostics: report.diagnostics.clone(),
            },
            Err(failure) => Self {
                raw_response: failure.raw_response.clone(),
                provenance: None,
                diagnostics: failure.diagnostics.clone(),
            },
        }
    }
}
pub struct StoryUseCases<G> {
    generator: G,
    evidence: Option<GenerationEvidence>,
}
impl<G: StoryGenerator> StoryUseCases<G> {
    pub fn new(generator: G) -> Self {
        Self {
            generator,
            evidence: None,
        }
    }
    pub fn take_evidence(&mut self) -> Option<GenerationEvidence> {
        self.evidence.take()
    }
    pub fn into_generator(self) -> G {
        self.generator
    }
    pub fn generate_outline(
        &mut self,
        brief: &Brief,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        let result = check_cancelled(cancel).and_then(|()| self.generator.outline(brief, cancel));
        self.evidence = Some(GenerationEvidence::capture(&result));
        let generated = result?;
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
        let result = check_cancelled(cancel)
            .and_then(|()| self.generator.cast(brief, &outline, limits, cancel));
        self.evidence = Some(GenerationEvidence::capture(&result));
        let generated = result?;
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
        let result = check_cancelled(cancel).and_then(|()| {
            self.generator
                .turn(&state, &direction, cancel, on_narrative)
        });
        self.evidence = Some(GenerationEvidence::capture(&result));
        let generated = match result
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
