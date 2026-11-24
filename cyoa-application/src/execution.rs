//! Execution contracts. Adapters own threads and joins; application owns acceptance.
use crate::{
    persistence::*,
    session::{Completion, RequestKey, SessionRevision, WorkRequest},
};
use std::{io, num::NonZeroUsize};

#[derive(Debug, Clone, Copy)]
pub struct PreviewLimit(NonZeroUsize);
impl PreviewLimit {
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

pub enum WorkerEvent {
    Progress {
        key: RequestKey,
        text: String,
        incomplete: bool,
    },
    Finished(Box<Completion>),
    Fault {
        key: RequestKey,
        message: String,
    },
}

/// Poll never joins a live worker; terminal events imply joined execution.
pub trait GenerationExecution {
    fn start(&mut self, request: WorkRequest) -> io::Result<()>;
    fn poll(&mut self) -> Vec<WorkerEvent>;
}

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
    pub fn operation(&self) -> StorageOperation {
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
pub trait StorageExecution {
    fn start(&mut self, key: StorageKey, intent: StorageIntent) -> io::Result<()>;
    fn recover_fault(&mut self);
    fn cancel(&self);
    fn close(&mut self);
    fn poll(&mut self) -> Vec<StorageEvent>;
}

/// One scoped startup/query job, driven by the caller's cancellation signal.
/// This preserves the current synchronous CLI contract without terminal globals.
pub trait BlockingExecution {
    fn run<T: Send, E: std::error::Error + Send + 'static>(
        &self,
        operation: &str,
        job: impl FnOnce(&crate::cancellation::CancellationToken) -> Result<T, E> + Send,
        cancel_requested: impl FnMut() -> bool,
    ) -> Result<T, Box<dyn std::error::Error>>;
}
