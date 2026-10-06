//! Sequential owned inference threads. Poll never joins a live thread.
use crate::session::{Completion, RequestKey, WorkRequest};
use cyoa_application::{
    cancellation::CancellationSource,
    generation::{StoryGenerator, StoryUseCases},
};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

#[derive(Debug, Clone, Copy)]
pub struct PreviewLimit(NonZeroUsize);
impl PreviewLimit {
    /// The largest preview any layer retains.
    pub const MAX_BYTES: usize = 1_048_576;
    pub fn new(bytes: usize) -> Option<Self> {
        (bytes <= Self::MAX_BYTES)
            .then(|| NonZeroUsize::new(bytes).map(Self))
            .flatten()
    }
    pub fn bytes(self) -> usize {
        self.0.get()
    }
}
impl Default for PreviewLimit {
    fn default() -> Self {
        Self::new(Self::MAX_BYTES).unwrap()
    }
}
#[derive(Default)]
struct Mailbox {
    text: String,
    total: usize,
    incomplete: bool,
}
impl Mailbox {
    fn publish(&mut self, text: &str, limit: PreviewLimit) {
        if self.incomplete {
            return;
        }
        let mut bytes = text.len().min(limit.bytes().saturating_sub(self.total));
        while !text.is_char_boundary(bytes) {
            bytes -= 1;
        }
        self.text.push_str(&text[..bytes]);
        self.total += bytes;
        self.incomplete = bytes < text.len();
    }
    fn drain(&mut self, key: RequestKey) -> Option<WorkerEvent> {
        if self.text.is_empty() && !self.incomplete {
            return None;
        }
        Some(WorkerEvent::Progress {
            key,
            text: std::mem::take(&mut self.text),
            incomplete: self.incomplete,
        })
    }
}
pub enum WorkerEvent {
    Progress {
        key: RequestKey,
        text: String,
        incomplete: bool,
    },
    Finished(Completion),
    Fault {
        key: RequestKey,
        message: String,
    },
}
type Returned<G> = (StoryUseCases<G>, Completion);
type Job<G> = Box<dyn FnOnce() -> Returned<G> + Send>;
enum State<G: StoryGenerator + Send + 'static> {
    Ready(StoryUseCases<G>),
    Running {
        key: RequestKey,
        source: Arc<CancellationSource>,
        mailbox: Arc<Mutex<Mailbox>>,
        handle: JoinHandle<Returned<G>>,
    },
    Faulted,
    Closed,
}
pub struct WorkerRunner<G: StoryGenerator + Send + 'static> {
    state: State<G>,
    limit: PreviewLimit,
}
impl<G: StoryGenerator + Send + 'static> WorkerRunner<G> {
    pub fn new(cases: StoryUseCases<G>, limit: PreviewLimit) -> Self {
        Self {
            state: State::Ready(cases),
            limit,
        }
    }
    pub fn is_ready(&self) -> bool {
        matches!(self.state, State::Ready(_))
    }
    pub fn start(&mut self, request: WorkRequest) -> std::io::Result<()> {
        self.start_with(request, |job| {
            thread::Builder::new()
                .name("cyoa-generation".into())
                .spawn(job)
        })
    }
    fn start_with(
        &mut self,
        request: WorkRequest,
        spawn: impl FnOnce(Job<G>) -> std::io::Result<JoinHandle<Returned<G>>>,
    ) -> std::io::Result<()> {
        if !self.is_ready() {
            return Err(std::io::Error::other("worker is busy or faulted"));
        }
        let State::Ready(cases) = std::mem::replace(&mut self.state, State::Closed) else {
            unreachable!()
        };
        let cell = Arc::new(Mutex::new(Some(cases)));
        let worker_cell = Arc::clone(&cell);
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        let writer = Arc::clone(&mailbox);
        let limit = self.limit;
        let key = request.key();
        let source = Arc::clone(&request.source);
        let job = Box::new(move || {
            let mut cases = worker_cell
                .lock()
                .unwrap()
                .take()
                .expect("single worker owns use cases");
            let completion = request.execute(&mut cases, &mut |text| {
                writer
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .publish(text, limit)
            });
            (cases, completion)
        });
        match spawn(job) {
            Ok(handle) => {
                self.state = State::Running {
                    key,
                    source,
                    mailbox,
                    handle,
                };
                Ok(())
            }
            Err(error) => {
                self.state = State::Ready(
                    cell.lock()
                        .unwrap()
                        .take()
                        .expect("failed spawn did not execute job"),
                );
                Err(error)
            }
        }
    }
    /// At most two coalesced progress batches and one terminal event per poll.
    pub fn poll(&mut self) -> Vec<WorkerEvent> {
        let mut events = Vec::new();
        let finished = if let State::Running {
            key,
            mailbox,
            handle,
            ..
        } = &self.state
        {
            if let Some(event) = mailbox
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .drain(*key)
            {
                events.push(event);
            }
            handle.is_finished()
        } else {
            false
        };
        if finished {
            let State::Running {
                key,
                mailbox,
                handle,
                ..
            } = std::mem::replace(&mut self.state, State::Closed)
            else {
                unreachable!()
            };
            // The writer is now finished. Drain its final publication before the terminal.
            if let Some(event) = mailbox
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .drain(key)
            {
                events.push(event);
            }
            match handle.join() {
                Ok((cases, completion)) => {
                    self.state = State::Ready(cases);
                    events.push(WorkerEvent::Finished(completion));
                }
                Err(_) => {
                    self.state = State::Faulted;
                    events.push(WorkerEvent::Fault {
                        key,
                        message: "generation worker panicked; reconnect before retrying".into(),
                    });
                }
            }
        }
        events
    }
}
impl<G: StoryGenerator + Send + 'static> Drop for WorkerRunner<G> {
    fn drop(&mut self) {
        if let State::Running { source, handle, .. } =
            std::mem::replace(&mut self.state, State::Closed)
        {
            source.cancel();
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyoa_application::{
        cancellation::CancellationToken,
        generation::{Generated, GenerationFailure, TurnDirection},
    };
    use cyoa_core::{
        game::GameState,
        limits::Limits,
        style::StoryStyle,
        text::{Brief, RawResponse, WorldDescription, WorldTitle},
        turn::{GenerationProvenance, StoryTurn},
        world::{WorldCast, WorldOutline},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Counter(Arc<AtomicUsize>);
    impl StoryGenerator for Counter {
        fn outline(
            &mut self,
            _: &Brief,
            _: &CancellationToken,
        ) -> Result<Generated<WorldOutline>, GenerationFailure> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Generated::new(
                WorldOutline::new(
                    WorldTitle::new("Title").unwrap(),
                    WorldDescription::new("World").unwrap(),
                ),
                RawResponse::new("exact"),
                GenerationProvenance::default(),
            ))
        }
        fn cast(
            &mut self,
            _: &Brief,
            _: &WorldOutline,
            _: &Limits,
            _: &CancellationToken,
        ) -> Result<Generated<WorldCast>, GenerationFailure> {
            unreachable!()
        }
        fn turn(
            &mut self,
            _: &GameState,
            _: &TurnDirection,
            _: &CancellationToken,
            _: &mut dyn FnMut(&str),
        ) -> Result<Generated<StoryTurn>, GenerationFailure> {
            unreachable!()
        }
    }
    #[test]
    fn failed_spawn_keeps_use_cases_available_for_explicit_retry() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut runner = WorkerRunner::new(
            StoryUseCases::new(Counter(Arc::clone(&calls))),
            PreviewLimit::default(),
        );
        let mut controller =
            crate::session::SessionController::new(Limits::default(), StoryStyle::default());
        let request = controller
            .submit_brief(Brief::new("brief").unwrap())
            .unwrap();
        let key = request.key();
        let result = runner.start_with(request, |_job| {
            Err(std::io::Error::other("injected spawn failure"))
        });
        assert!(result.is_err());
        assert!(runner.is_ready());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        controller.dispatch_failed(key, "spawn failure".into());
        runner.start(controller.retry().unwrap()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !runner.is_ready() {
            for event in runner.poll() {
                if let WorkerEvent::Finished(done) = event {
                    controller.complete(done);
                }
            }
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            controller.stage(),
            crate::session::Stage::OutlineReview { .. }
        ));
    }
    #[test]
    fn mailbox_budget_is_cumulative_and_does_not_split_unicode() {
        let mut m = Mailbox::default();
        let limit = PreviewLimit::new(5).unwrap();
        m.publish("é雪", limit);
        assert_eq!(m.text, "é雪");
        m.text.clear();
        m.publish("extra", limit);
        assert!(m.text.is_empty());
        assert!(m.incomplete);
        assert_eq!(m.total, 5);
        assert!(PreviewLimit::new(0).is_none());
    }
}
