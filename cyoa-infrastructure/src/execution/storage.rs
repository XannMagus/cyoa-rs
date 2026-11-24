//! Owned storage worker. Polling never joins a live thread or performs JSON/I/O.
use cyoa_application::execution::{
    StorageEvent, StorageExecution, StorageIntent, StorageKey, StorageOutcome,
};
use cyoa_application::{cancellation::CancellationSource, persistence::*};
use std::{
    io,
    sync::Arc,
    thread::{self, JoinHandle},
};

enum State {
    Ready,
    Running {
        key: StorageKey,
        operation: StorageOperation,
        source: CancellationSource,
        evidence: PreparedWriteEvidence,
        published: Option<ContentStamp>,
        handle: JoinHandle<Result<StorageOutcome, StorageFailure>>,
    },
    Faulted,
    Closed,
}
/// The factory constructs the inward repository on the worker. Each write gets a
/// fresh preparation sink through the port; the runner keeps its own clone, so a
/// prepared write survives a worker that unwinds.
pub struct StorageRunner<R, F>
where
    R: GameRepository + Send + 'static,
    F: Fn() -> R + Send + Sync + 'static,
{
    factory: Arc<F>,
    state: State,
    closing: bool,
}
impl<R, F> StorageRunner<R, F>
where
    R: GameRepository + Send + 'static,
    F: Fn() -> R + Send + Sync + 'static,
{
    pub fn new(factory: F) -> Self {
        Self {
            factory: Arc::new(factory),
            state: State::Ready,
            closing: false,
        }
    }
    pub fn is_ready(&self) -> bool {
        !self.closing && matches!(self.state, State::Ready)
    }
    pub fn is_closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }
    /// A joined panic lost only this request's repository, not the factory.
    pub fn recover_fault(&mut self) {
        if !self.closing && matches!(self.state, State::Faulted) {
            self.state = State::Ready;
        }
    }
    pub fn start(&mut self, key: StorageKey, intent: StorageIntent) -> io::Result<()> {
        self.start_with(key, intent, |job| {
            thread::Builder::new()
                .name("cyoa-storage".into())
                .spawn(job)
        })
    }
    fn start_with(
        &mut self,
        key: StorageKey,
        intent: StorageIntent,
        spawn: impl FnOnce(
            Box<dyn FnOnce() -> Result<StorageOutcome, StorageFailure> + Send>,
        ) -> io::Result<JoinHandle<Result<StorageOutcome, StorageFailure>>>,
    ) -> io::Result<()> {
        if !self.is_ready() {
            return Err(io::Error::other(
                "storage runner is busy, closed or faulted",
            ));
        }
        let factory = self.factory.clone();
        let operation = intent.operation();
        let source = CancellationSource::default();
        let token = source.token();
        let evidence = PreparedWriteEvidence::default();
        let publisher = evidence.clone();
        let job = Box::new(move || {
            let mut cases = PersistenceUseCases::new(factory());
            match intent {
                StorageIntent::Save(command) => cases
                    .save_game(*command, &publisher, &token)
                    .map(StorageOutcome::Saved),
                StorageIntent::Load(command) => cases
                    .load_game(command, &token)
                    .map(|v| StorageOutcome::Loaded(Box::new(v))),
                StorageIntent::Inspect(query) => cases
                    .inspect_save(query, &token)
                    .map(|v| StorageOutcome::Inspected(Box::new(v))),
                StorageIntent::List(query) => {
                    cases.list_saves(query, &token).map(StorageOutcome::Listed)
                }
            }
        });
        let handle = spawn(job)?;
        self.state = State::Running {
            key,
            operation,
            source,
            evidence,
            published: None,
            handle,
        };
        Ok(())
    }
    pub fn cancel(&self) {
        if let State::Running { source, .. } = &self.state {
            source.cancel();
        }
    }
    pub fn close(&mut self) {
        self.closing = true;
        self.cancel();
        if !matches!(self.state, State::Running { .. }) {
            self.state = State::Closed;
        }
    }
    pub fn poll(&mut self) -> Vec<StorageEvent> {
        let mut events = vec![];
        let finished = if let State::Running {
            key,
            evidence,
            published,
            handle,
            ..
        } = &mut self.state
        {
            if let Some(pending) = evidence.pending()
                && *published != Some(pending.intended_stamp())
            {
                *published = Some(pending.intended_stamp());
                events.push(StorageEvent::Prepared { key: *key, pending });
            }
            handle.is_finished()
        } else {
            false
        };
        if finished {
            let State::Running {
                key,
                operation,
                evidence,
                published,
                handle,
                ..
            } = std::mem::replace(&mut self.state, State::Closed)
            else {
                unreachable!()
            };
            let result = handle.join();
            // Publication can race the first poll; deliver it before completion.
            if let Some(pending) = evidence.pending()
                && published != Some(pending.intended_stamp())
            {
                events.push(StorageEvent::Prepared { key, pending });
            }
            let panicked = result.is_err();
            let result = result.unwrap_or_else(|_| {
                let failure = StorageFailure::new(
                    operation,
                    StorageStage::Worker,
                    StorageFailureKind::WorkerFault,
                    "storage worker panicked; use retained preparation to reconcile with a fresh runner",
                );
                // A worker that unwound after preparing may have changed disk.
                Err(match evidence.pending() {
                    Some(pending) => failure.visibility_unknown().prepared(pending),
                    None => failure,
                })
            });
            self.state = if self.closing {
                State::Closed
            } else if panicked {
                State::Faulted
            } else {
                State::Ready
            };
            events.push(StorageEvent::Finished { key, result });
        }
        events
    }
}
impl<R, F> StorageExecution for StorageRunner<R, F>
where
    R: GameRepository + Send + 'static,
    F: Fn() -> R + Send + Sync + 'static,
{
    fn start(&mut self, key: StorageKey, intent: StorageIntent) -> io::Result<()> {
        self.start(key, intent)
    }
    fn recover_fault(&mut self) {
        self.recover_fault();
    }
    fn cancel(&self) {
        self.cancel();
    }
    fn close(&mut self) {
        self.close();
    }
    fn poll(&mut self) -> Vec<StorageEvent> {
        self.poll()
    }
}
impl<R, F> Drop for StorageRunner<R, F>
where
    R: GameRepository + Send + 'static,
    F: Fn() -> R + Send + Sync + 'static,
{
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
    struct Empty;
    impl cyoa_core::persistence::GameRepository for Empty {
        type Context = cyoa_application::cancellation::CancellationToken;
        type Failure = cyoa_application::persistence::StorageFailure;
        type Attempt = cyoa_application::persistence::PendingWrite;
        type Stored = cyoa_application::persistence::StoredGame;

        fn create(
            &mut self,
            _: SaveSnapshot,
            _: &dyn AttemptObserver<PendingWrite>,
            _: &cyoa_application::cancellation::CancellationToken,
        ) -> Result<SaveReceipt, StorageFailure> {
            unreachable!()
        }
        fn replace(
            &mut self,
            _: SaveTarget,
            _: SaveSnapshot,
            _: &dyn AttemptObserver<PendingWrite>,
            _: &cyoa_application::cancellation::CancellationToken,
        ) -> Result<SaveReceipt, StorageFailure> {
            unreachable!()
        }
        fn reconcile(
            &mut self,
            _: PendingWrite,
            _: &dyn AttemptObserver<PendingWrite>,
            _: &cyoa_application::cancellation::CancellationToken,
        ) -> Result<SaveReceipt, StorageFailure> {
            unreachable!()
        }
        fn load(
            &mut self,
            _: &SaveId,
            _: SaveCopy,
            _: &cyoa_application::cancellation::CancellationToken,
        ) -> Result<StoredGame, StorageFailure> {
            unreachable!()
        }
    }
    impl cyoa_application::persistence::SaveQueries for Empty {
        fn list(
            &mut self,
            _: SavePage,
            _: &cyoa_application::cancellation::CancellationToken,
        ) -> Result<SavePageResult, StorageFailure> {
            Ok(SavePageResult {
                entries: vec![],
                next: None,
            })
        }
    }
    #[test]
    fn failed_storage_thread_spawn_preserves_ready_state_and_never_runs_factory() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let mut runner = StorageRunner::new(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Empty
        });
        let key = StorageKey {
            id: StorageRequestId::new(1).unwrap(),
            revision: cyoa_application::session::SessionController::new(
                cyoa_core::limits::Limits::default(),
                cyoa_core::style::StoryStyle::default(),
            )
            .revision(),
        };
        let intent = || {
            StorageIntent::List(ListSaves {
                page: SavePage::new(None, PageSize::new(1).unwrap()),
            })
        };
        assert!(
            runner
                .start_with(key, intent(), |_| Err(io::Error::other("spawn failed")))
                .is_err()
        );
        assert!(runner.is_ready());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        runner.start(key, intent()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(StorageEvent::Finished {
                result: Ok(StorageOutcome::Listed(page)),
                ..
            }) = runner.poll().pop()
            {
                assert!(page.entries.is_empty());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(runner.is_ready());
    }
}
