//! Drives effects without exposing mutable canonical state or concrete backends.
use crate::{session::*, worker::*};
use cyoa_application::generation::{StoryGenerator, StoryUseCases, TurnDirection};
use cyoa_core::{
    game::TurnCount,
    text::Brief,
    world::{PlayablePosition, WorldOutline},
};
pub enum Intent {
    SubmitBrief(Brief),
    ReplaceOutline(WorldOutline),
    AcceptOutline,
    Select(PlayablePosition),
    Turn(TurnDirection),
    Action(usize),
    Retry,
    Rewind(TurnCount),
    Cancel,
    Quit,
}
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("could not start worker: {0}")]
    Spawn(#[from] std::io::Error),
}
pub struct SessionRuntime<G: StoryGenerator + Send + 'static> {
    controller: SessionController,
    worker: WorkerRunner<G>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalChange {
    Outline,
    Cast,
    Turn,
}
pub struct RuntimeEvent {
    pub acceptance: Acceptance,
    pub change: Option<CanonicalChange>,
    pub worker_fault: bool,
}
impl<G: StoryGenerator + Send + 'static> SessionRuntime<G> {
    pub fn new(
        controller: SessionController,
        cases: StoryUseCases<G>,
        limit: PreviewLimit,
    ) -> Self {
        Self {
            controller,
            worker: WorkerRunner::new(cases, limit),
        }
    }
    pub fn controller(&self) -> &SessionController {
        &self.controller
    }
    pub fn replace_game(&mut self, game: cyoa_core::game::GameState) -> Result<(), SessionError> {
        self.controller.replace_game(game)
    }
    pub fn dispatch(&mut self, intent: Intent) -> Result<(), RuntimeError> {
        let request = match intent {
            Intent::SubmitBrief(b) => Some(self.controller.submit_brief(b)?),
            Intent::ReplaceOutline(o) => {
                self.controller.replace_outline(o)?;
                None
            }
            Intent::AcceptOutline => Some(self.controller.accept_outline()?),
            Intent::Select(p) => {
                self.controller.select(p)?;
                None
            }
            Intent::Turn(d) => Some(self.controller.take_turn(d)?),
            Intent::Action(n) => Some(self.controller.action(n)?),
            Intent::Retry => Some(self.controller.retry()?),
            Intent::Rewind(n) => {
                self.controller.rewind(n)?;
                None
            }
            Intent::Cancel => {
                self.controller.cancel();
                None
            }
            Intent::Quit => {
                self.controller.quit();
                None
            }
        };
        if let Some(request) = request {
            let key = request.key();
            if let Err(error) = self.worker.start(request) {
                self.controller.dispatch_failed(key, error.to_string());
                return Err(error.into());
            }
        }
        Ok(())
    }
    /// Nonblocking. Terminal events are delivered only after their thread joined.
    pub fn poll(&mut self) -> Vec<Acceptance> {
        self.poll_events()
            .into_iter()
            .map(|e| e.acceptance)
            .collect()
    }
    pub fn poll_events(&mut self) -> Vec<RuntimeEvent> {
        let mut accepted = Vec::new();
        for event in self.worker.poll() {
            match event {
                WorkerEvent::Progress {
                    key,
                    text,
                    incomplete,
                } => self.controller.progress(key, &text, incomplete),
                WorkerEvent::Finished(done) => {
                    let change = match &done.outcome {
                        Ok(WorkSuccess::Outline(_)) => Some(CanonicalChange::Outline),
                        Ok(WorkSuccess::Cast(_)) => Some(CanonicalChange::Cast),
                        Ok(WorkSuccess::Turn(_)) => Some(CanonicalChange::Turn),
                        Err(_) => None,
                    };
                    let acceptance = self.controller.complete(done);
                    accepted.push(RuntimeEvent {
                        acceptance,
                        worker_fault: false,
                        change: if acceptance == Acceptance::Committed {
                            change
                        } else {
                            None
                        },
                    });
                }
                WorkerEvent::Fault { key, message } => accepted.push(RuntimeEvent {
                    acceptance: self.controller.worker_failed(key, message),
                    change: None,
                    worker_fault: true,
                }),
            }
        }
        accepted
    }
}
