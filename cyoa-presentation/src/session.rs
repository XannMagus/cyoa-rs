//! Terminal-independent lifecycle. Only this controller owns canonical state.
use cyoa_application::{
    cancellation::{CancellationSource, CancellationToken},
    generation::{
        self, Generated, GenerationFailure, StoryGenerator, StoryUseCases, TurnDirection,
    },
};
use cyoa_core::{
    game::{GameState, TurnCount},
    limits::Limits,
    style::StoryStyle,
    text::{Brief, PlayerInput},
    world::{PlayablePosition, World, WorldOutline},
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestId(u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRevision(u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestKey {
    id: RequestId,
    revision: SessionRevision,
}
impl RequestKey {
    pub fn id(self) -> RequestId {
        self.id
    }
    pub fn revision(self) -> SessionRevision {
        self.revision
    }
}
impl RequestId {
    pub fn get(self) -> u64 {
        self.0
    }
}
impl SessionRevision {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Brief,
    OutlineReview { brief: Brief, outline: WorldOutline },
    CastSelection { brief: Brief, world: World },
    Playing { game: Box<GameState> },
}
#[derive(Debug, Clone)]
pub(crate) enum Work {
    Outline(Brief),
    Cast {
        brief: Brief,
        outline: WorldOutline,
        limits: Limits,
    },
    Turn {
        game: Box<GameState>,
        direction: TurnDirection,
    },
}
#[derive(Debug)]
pub struct WorkRequest {
    pub(crate) key: RequestKey,
    pub(crate) work: Work,
    pub(crate) token: CancellationToken,
    pub(crate) source: Arc<CancellationSource>,
}
impl WorkRequest {
    pub fn key(&self) -> RequestKey {
        self.key
    }
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
    pub(crate) fn execute<G: StoryGenerator>(
        self,
        cases: &mut StoryUseCases<G>,
        progress: &mut dyn FnMut(&str),
    ) -> Completion {
        let outcome = match self.work {
            Work::Outline(brief) => cases
                .generate_outline(&brief, &self.token)
                .map(WorkSuccess::Outline),
            Work::Cast {
                brief,
                outline,
                limits,
            } => cases
                .generate_world(&brief, outline, &limits, &self.token)
                .map(WorkSuccess::Cast),
            Work::Turn {
                mut game,
                direction,
            } => cases
                .take_turn(&mut game, direction, &self.token, progress)
                .map(|()| WorkSuccess::Turn(game)),
        };
        Completion {
            key: self.key,
            outcome,
        }
    }
}
#[derive(Debug)]
pub enum WorkSuccess {
    Outline(Generated<WorldOutline>),
    Cast(World),
    Turn(Box<GameState>),
}
#[derive(Debug)]
pub struct Completion {
    pub key: RequestKey,
    pub outcome: Result<WorkSuccess, GenerationFailure>,
}
#[derive(Debug)]
pub enum Failure {
    Generation(GenerationFailure),
    Cancelled {
        rejected: Result<WorkSuccess, GenerationFailure>,
    },
    Worker(String),
}
#[derive(Debug)]
struct Pending {
    key: RequestKey,
    source: Arc<CancellationSource>,
    work: Work,
}
#[derive(Debug)]
enum Operation {
    Ready,
    Running(Pending),
    Cancelling(Pending),
    Failed { work: Work, failure: Failure },
    Faulted(Failure),
    Closing(Option<Pending>),
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Running,
    Cancelling,
    Failed,
    Faulted,
    Closing,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptance {
    Ignored,
    Committed,
    Failed,
    Closed,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("an operation is already in progress or the session is closed")]
    Busy,
    #[error("intent is not valid in this stage")]
    WrongStage,
    #[error("selection is out of range")]
    Selection,
    #[error("request or revision counter exhausted")]
    Exhausted,
    #[error("cannot rewind that many turns")]
    Rewind,
}
#[derive(Debug)]
pub struct SessionController {
    stage: Stage,
    operation: Operation,
    limits: Limits,
    style: StoryStyle,
    last_id: u64,
    revision: SessionRevision,
    preview: String,
    incomplete: bool,
}
impl SessionController {
    pub fn new(limits: Limits, style: StoryStyle) -> Self {
        Self {
            stage: Stage::Brief,
            operation: Operation::Ready,
            limits,
            style,
            last_id: 0,
            revision: SessionRevision(0),
            preview: String::new(),
            incomplete: false,
        }
    }
    /// Seed with an already validated in-memory game, not a save/load boundary.
    pub fn from_game(game: GameState) -> Self {
        let mut session = Self::new(game.limits(), game.style().clone());
        session.stage = Stage::Playing {
            game: Box::new(game),
        };
        session
    }
    pub fn stage(&self) -> &Stage {
        &self.stage
    }
    pub fn revision(&self) -> SessionRevision {
        self.revision
    }
    pub fn game(&self) -> Option<&GameState> {
        if let Stage::Playing { game } = &self.stage {
            Some(game)
        } else {
            None
        }
    }
    pub fn phase(&self) -> Phase {
        match self.operation {
            Operation::Ready => Phase::Ready,
            Operation::Running(_) => Phase::Running,
            Operation::Cancelling(_) => Phase::Cancelling,
            Operation::Failed { .. } => Phase::Failed,
            Operation::Faulted(_) => Phase::Faulted,
            Operation::Closing(_) => Phase::Closing,
            Operation::Closed => Phase::Closed,
        }
    }
    pub fn failure(&self) -> Option<&Failure> {
        match &self.operation {
            Operation::Failed { failure, .. } | Operation::Faulted(failure) => Some(failure),
            _ => None,
        }
    }
    pub fn preview(&self) -> &str {
        &self.preview
    }
    pub fn preview_incomplete(&self) -> bool {
        self.incomplete
    }
    fn ready(&self) -> Result<(), SessionError> {
        if matches!(self.operation, Operation::Ready | Operation::Failed { .. }) {
            Ok(())
        } else {
            Err(SessionError::Busy)
        }
    }
    fn next_revision(&self) -> Result<SessionRevision, SessionError> {
        self.revision
            .0
            .checked_add(1)
            .map(SessionRevision)
            .ok_or(SessionError::Exhausted)
    }
    fn start(&mut self, work: Work) -> Result<WorkRequest, SessionError> {
        self.ready()?;
        self.next_revision()?;
        let id = self.last_id.checked_add(1).ok_or(SessionError::Exhausted)?;
        let key = RequestKey {
            id: RequestId(id),
            revision: self.revision,
        };
        let source = Arc::new(CancellationSource::default());
        let request = WorkRequest {
            key,
            work: work.clone(),
            token: source.token(),
            source: Arc::clone(&source),
        };
        self.last_id = id;
        self.preview.clear();
        self.incomplete = false;
        self.operation = Operation::Running(Pending { key, source, work });
        Ok(request)
    }
    pub fn submit_brief(&mut self, brief: Brief) -> Result<WorkRequest, SessionError> {
        self.ready()?;
        if !matches!(self.stage, Stage::Brief) {
            return Err(SessionError::WrongStage);
        }
        self.start(Work::Outline(brief))
    }
    pub fn replace_outline(&mut self, outline: WorldOutline) -> Result<(), SessionError> {
        self.ready()?;
        let revision = self.next_revision()?;
        let Stage::OutlineReview { brief, .. } = &self.stage else {
            return Err(SessionError::WrongStage);
        };
        self.stage = Stage::OutlineReview {
            brief: brief.clone(),
            outline,
        };
        self.revision = revision;
        self.operation = Operation::Ready;
        Ok(())
    }
    pub fn accept_outline(&mut self) -> Result<WorkRequest, SessionError> {
        self.ready()?;
        let Stage::OutlineReview { brief, outline } = &self.stage else {
            return Err(SessionError::WrongStage);
        };
        self.start(Work::Cast {
            brief: brief.clone(),
            outline: outline.clone(),
            limits: self.limits,
        })
    }
    pub fn select(&mut self, position: PlayablePosition) -> Result<(), SessionError> {
        self.ready()?;
        let revision = self.next_revision()?;
        let Stage::CastSelection { brief, world } = &self.stage else {
            return Err(SessionError::WrongStage);
        };
        let selected = world
            .clone()
            .select(position)
            .map_err(|_| SessionError::Selection)?;
        self.stage = Stage::Playing {
            game: Box::new(GameState::start(
                brief.clone(),
                selected,
                self.style.clone(),
                self.limits,
            )),
        };
        self.revision = revision;
        self.operation = Operation::Ready;
        Ok(())
    }
    pub fn take_turn(&mut self, direction: TurnDirection) -> Result<WorkRequest, SessionError> {
        self.ready()?;
        let game = self.game().ok_or(SessionError::WrongStage)?;
        if game.turns().is_empty() && direction != TurnDirection::Continue {
            return Err(SessionError::WrongStage);
        }
        self.start(Work::Turn {
            game: Box::new(game.clone()),
            direction,
        })
    }
    /// One-based action number from the last committed turn.
    pub fn action(&mut self, number: usize) -> Result<WorkRequest, SessionError> {
        self.ready()?;
        let action = self
            .game()
            .and_then(|g| g.turns().last())
            .and_then(|t| {
                number
                    .checked_sub(1)
                    .and_then(|i| t.turn().quick_actions().as_slice().get(i))
            })
            .ok_or(SessionError::Selection)?;
        let input =
            PlayerInput::new(action.text().as_str()).expect("checked action text is nonblank");
        self.take_turn(TurnDirection::Player(input))
    }
    pub fn retry(&mut self) -> Result<WorkRequest, SessionError> {
        let Operation::Failed { work, .. } = &self.operation else {
            return Err(SessionError::WrongStage);
        };
        self.start(work.clone())
    }
    pub fn rewind(&mut self, count: TurnCount) -> Result<(), SessionError> {
        self.ready()?;
        let revision = self.next_revision()?;
        let Stage::Playing { game } = &mut self.stage else {
            return Err(SessionError::WrongStage);
        };
        generation::rewind(game, count).map_err(|_| SessionError::Rewind)?;
        self.revision = revision;
        self.operation = Operation::Ready;
        self.preview.clear();
        Ok(())
    }
    pub fn cancel(&mut self) {
        if matches!(self.operation, Operation::Running(_)) {
            let Operation::Running(pending) =
                std::mem::replace(&mut self.operation, Operation::Ready)
            else {
                unreachable!()
            };
            pending.source.cancel();
            self.preview.clear();
            self.operation = Operation::Cancelling(pending);
        }
    }
    pub fn quit(&mut self) {
        let old = std::mem::replace(&mut self.operation, Operation::Closed);
        self.operation = match old {
            Operation::Running(p) | Operation::Cancelling(p) => {
                p.source.cancel();
                Operation::Closing(Some(p))
            }
            Operation::Closing(p) => Operation::Closing(p),
            _ => Operation::Closed,
        };
        self.preview.clear();
    }
    pub fn progress(&mut self, key: RequestKey, text: &str, incomplete: bool) {
        if matches!(&self.operation, Operation::Running(p) if p.key == key && key.revision == self.revision)
        {
            let remaining = 1_048_576usize.saturating_sub(self.preview.len());
            if text.len() <= remaining {
                self.preview.push_str(text);
            } else {
                self.incomplete = true;
            }
            self.incomplete |= incomplete;
        }
    }
    /// Called only after the runner has joined and recovered the use cases.
    pub fn complete(&mut self, completion: Completion) -> Acceptance {
        if self.active_key() != Some(completion.key) || completion.key.revision != self.revision {
            return Acceptance::Ignored;
        }
        let old = std::mem::replace(&mut self.operation, Operation::Ready);
        self.preview.clear();
        let pending = match old {
            Operation::Closing(_) => {
                self.operation = Operation::Closed;
                return Acceptance::Closed;
            }
            Operation::Cancelling(p) => {
                self.operation = Operation::Failed {
                    work: p.work,
                    failure: Failure::Cancelled {
                        rejected: completion.outcome,
                    },
                };
                return Acceptance::Failed;
            }
            Operation::Running(p) => p,
            _ => unreachable!("active key requires pending operation"),
        };
        if pending.source.token().is_cancelled() {
            self.operation = Operation::Failed {
                work: pending.work,
                failure: Failure::Cancelled {
                    rejected: completion.outcome,
                },
            };
            return Acceptance::Failed;
        }
        match completion.outcome {
            Err(failure) => {
                self.operation = Operation::Failed {
                    work: pending.work,
                    failure: Failure::Generation(failure),
                };
                Acceptance::Failed
            }
            Ok(success) => {
                let stage = match (pending.work, success) {
                    (Work::Outline(brief), WorkSuccess::Outline(generated)) => {
                        Stage::OutlineReview {
                            brief,
                            outline: generated.into_parts().0,
                        }
                    }
                    (Work::Cast { brief, .. }, WorkSuccess::Cast(world)) => {
                        Stage::CastSelection { brief, world }
                    }
                    (Work::Turn { .. }, WorkSuccess::Turn(game)) => Stage::Playing { game },
                    _ => {
                        self.operation = Operation::Faulted(Failure::Worker(
                            "worker returned the wrong result kind".into(),
                        ));
                        return Acceptance::Failed;
                    }
                };
                self.revision = self
                    .next_revision()
                    .expect("revision capacity reserved at admission");
                self.stage = stage;
                Acceptance::Committed
            }
        }
    }
    pub fn worker_failed(&mut self, key: RequestKey, message: String) -> Acceptance {
        if !self.active_key().is_some_and(|active| active == key) {
            return Acceptance::Ignored;
        }
        if matches!(self.operation, Operation::Closing(_)) {
            self.operation = Operation::Closed;
            Acceptance::Closed
        } else {
            self.operation = Operation::Faulted(Failure::Worker(message));
            self.preview.clear();
            Acceptance::Failed
        }
    }
    fn active_key(&self) -> Option<RequestKey> {
        match &self.operation {
            Operation::Running(p) | Operation::Cancelling(p) | Operation::Closing(Some(p)) => {
                Some(p.key)
            }
            _ => None,
        }
    }
}
impl Drop for SessionController {
    fn drop(&mut self) {
        match &self.operation {
            Operation::Running(p) | Operation::Cancelling(p) | Operation::Closing(Some(p)) => {
                p.source.cancel()
            }
            _ => (),
        }
    }
}
