//! Business generation capabilities. Execution context and reports are supplied
//! by adapters; domain values never contain transport buffers or vendor envelopes.
use crate::{
    game::GameState,
    limits::Limits,
    text::{Brief, PlayerInput, RawResponse},
    turn::{GenerationProvenance, StoryTurn},
    world::{WorldCast, WorldOutline},
};

pub trait GenerationControl {
    fn is_cancelled(&self) -> bool;
}

/// The business-facing part of a generated result. An outer execution report may
/// retain additional diagnostics without putting them in the domain model.
pub trait GenerationReport<T> {
    fn value(&self) -> &T;
    fn into_parts(self) -> (T, RawResponse, GenerationProvenance);
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
    type Context: GenerationControl + ?Sized;
    type Failure: std::error::Error;
    type Report<T>: GenerationReport<T>;

    fn outline(
        &mut self,
        brief: &Brief,
        context: &Self::Context,
    ) -> Result<Self::Report<WorldOutline>, Self::Failure>;
    fn cast(
        &mut self,
        brief: &Brief,
        outline: &WorldOutline,
        limits: &Limits,
        context: &Self::Context,
    ) -> Result<Self::Report<WorldCast>, Self::Failure>;
    fn turn(
        &mut self,
        game: &GameState,
        direction: &TurnDirection,
        context: &Self::Context,
        on_narrative: &mut dyn FnMut(&str),
    ) -> Result<Self::Report<StoryTurn>, Self::Failure>;
}
