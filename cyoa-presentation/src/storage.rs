//! Owned storage worker. Polling never joins a live thread or performs JSON/I/O.
use crate::session::SessionRevision;
use cyoa_application::{cancellation::CancellationSource, persistence::*};
use std::{
    io,
    sync::Arc,
    thread::{self, JoinHandle},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageKey {
    pub id: StorageRequestId,
    pub revision: SessionRevision,
}
pub enum StorageIntent {
    Save(Box<SaveGame>),
    Load(LoadGame),
    Inspect(InspectSave),
    List(ListSaves),
}
impl StorageIntent {
    pub(crate) fn operation(&self) -> StorageOperation {
        match self {
            Self::Save(command) => match command.as_ref() {
                SaveGame::Create(_) => StorageOperation::Create,
                SaveGame::Replace { .. } => StorageOperation::Replace,
                SaveGame::Reconcile(_) => StorageOperation::Reconcile,
            },
            Self::Load(_) => StorageOperation::Load,
            Self::Inspect(_) => StorageOperation::Inspect,
            Self::List(_) => StorageOperation::List,
        }
    }
}
pub enum StorageOutcome {
    Saved(SaveReceipt),
    Loaded(Box<LoadedGame>),
    Inspected(Box<StoredGame>),
    Listed(SavePageResult),
}
pub enum StorageEvent {
    Prepared {
        key: StorageKey,
        pending: PendingWrite,
    },
    Finished {
        key: StorageKey,
        result: Result<StorageOutcome, StorageFailure>,
    },
}
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
/// The factory constructs the inward repository on the worker. It receives an
/// evidence channel the repository publishes before dispatching any write helper.
pub struct StorageRunner<R, F>
where
    R: GameRepository + Send + 'static,
    F: Fn(PreparedWriteEvidence) -> R + Send + Sync + 'static,
{
    factory: Arc<F>,
    state: State,
    closing: bool,
}
impl<R, F> StorageRunner<R, F>
where
    R: GameRepository + Send + 'static,
    F: Fn(PreparedWriteEvidence) -> R + Send + Sync + 'static,
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
            let mut cases = PersistenceUseCases::new(factory(publisher));
            match intent {
                StorageIntent::Save(command) => {
                    cases.save_game(*command, &token).map(StorageOutcome::Saved)
                }
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
            let result=result.unwrap_or_else(|_|{let pending=evidence.pending();Err(StorageFailure{operation,stage:StorageStage::Worker,kind:StorageFailureKind::WorkerFault,message:"storage worker panicked; use retained preparation to reconcile with a fresh runner".into(),visibility:if pending.is_some(){WriteVisibility::Unknown}else{WriteVisibility::Unchanged},pending:pending.map(Box::new),cleanup_errors:Box::default()})});
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
impl<R, F> Drop for StorageRunner<R, F>
where
    R: GameRepository + Send + 'static,
    F: Fn(PreparedWriteEvidence) -> R + Send + Sync + 'static,
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
    impl GameRepository for Empty {
        fn create(
            &mut self,
            _: SaveSnapshot,
            _: &cyoa_application::cancellation::CancellationToken,
        ) -> Result<SaveReceipt, StorageFailure> {
            unreachable!()
        }
        fn replace(
            &mut self,
            _: SaveTarget,
            _: SaveSnapshot,
            _: &cyoa_application::cancellation::CancellationToken,
        ) -> Result<SaveReceipt, StorageFailure> {
            unreachable!()
        }
        fn reconcile(
            &mut self,
            _: PendingWrite,
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
        let mut runner = StorageRunner::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Empty
        });
        let key = StorageKey {
            id: StorageRequestId::new(1).unwrap(),
            revision: crate::session::SessionController::new(
                cyoa_core::limits::Limits::default(),
                cyoa_core::style::StoryStyle::default(),
            )
            .revision(),
        };
        let intent = || {
            StorageIntent::List(ListSaves {
                page: SavePage::new(None, 1).unwrap(),
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
