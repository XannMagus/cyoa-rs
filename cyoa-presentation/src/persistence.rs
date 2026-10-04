//! Canonical session coordination. Storage owns snapshots, never the live game.
use crate::{
    runtime::{CanonicalChange, Intent, RuntimeError, SessionRuntime},
    session::{Acceptance, Phase, SessionController, SessionError},
    storage::{StorageEvent, StorageIntent, StorageKey, StorageOutcome, StorageRunner},
};
use cyoa_application::{
    generation::{StoryGenerator, TurnDirection},
    persistence::*,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveBinding {
    Unbound,
    Bound {
        id: SaveId,
        stamp: ContentStamp,
        disk_revision: SaveRevision,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    Clean,
    Dirty,
    Uncertain,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shutdown {
    Open,
    StoppingGeneration,
    SavingFinal,
    DrainingOutput,
    Closed,
}
#[derive(Debug, thiserror::Error)]
pub enum CoordinationError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("storage operation is in progress or session is closing")]
    Busy,
    #[error("Save the current revision with /save before continuing.")]
    Unsaved,
    #[error("Select a character before saving or rewinding.")]
    NoGame,
    #[error("storage request counter exhausted")]
    Exhausted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    Save,
    Load,
    List,
}
struct Running {
    key: StorageKey,
    effect: Effect,
    pending: Option<PendingWrite>,
    prior_durability: Durability,
}
struct RetryWrite {
    revision: crate::session::SessionRevision,
    pending: PendingWrite,
}
pub enum PersistenceEvent {
    Generation {
        acceptance: Acceptance,
        change: Option<CanonicalChange>,
    },
    Saved(SaveReceipt),
    Loaded {
        old_id: SaveId,
        backup: bool,
        unrecognized_fields: Vec<String>,
    },
    Listed(SavePageResult),
    Failed(StorageFailure),
    Ignored,
}
pub struct PersistedSession<G, R, F>
where
    G: StoryGenerator + Send + 'static,
    R: GameRepository + Send + 'static,
    F: Fn(PreparedWriteEvidence) -> R + Send + Sync + 'static,
{
    runtime: SessionRuntime<G>,
    storage: StorageRunner<R, F>,
    source: StorySource,
    binding: SaveBinding,
    durability: Durability,
    running: Option<Running>,
    last_storage_id: u64,
    retry_write: Option<RetryWrite>,
    storage_failure: Option<StorageFailure>,
    deferred_opening: bool,
    shutdown: Shutdown,
    final_attempted: bool,
    closing_write_satisfied: bool,
    worker_fault: bool,
    notifications: Vec<PersistenceEvent>,
}
impl<G, R, F> PersistedSession<G, R, F>
where
    G: StoryGenerator + Send + 'static,
    R: GameRepository + Send + 'static,
    F: Fn(PreparedWriteEvidence) -> R + Send + Sync + 'static,
{
    pub fn new(
        runtime: SessionRuntime<G>,
        storage: StorageRunner<R, F>,
        source: StorySource,
    ) -> Self {
        let durability = if runtime.controller().game().is_some() {
            Durability::Dirty
        } else {
            Durability::Clean
        };
        Self {
            runtime,
            storage,
            source,
            binding: SaveBinding::Unbound,
            durability,
            running: None,
            last_storage_id: 0,
            retry_write: None,
            storage_failure: None,
            deferred_opening: false,
            shutdown: Shutdown::Open,
            final_attempted: false,
            closing_write_satisfied: false,
            worker_fault: false,
            notifications: Vec::new(),
        }
    }
    pub fn controller(&self) -> &SessionController {
        self.runtime.controller()
    }
    pub fn binding(&self) -> &SaveBinding {
        &self.binding
    }
    pub fn durability(&self) -> Durability {
        self.durability
    }
    pub fn shutdown(&self) -> Shutdown {
        self.shutdown
    }
    pub fn storage_busy(&self) -> bool {
        self.running.is_some()
    }
    pub fn storage_failure(&self) -> Option<&StorageFailure> {
        self.storage_failure.as_ref()
    }
    pub fn exit_failed(&self) -> bool {
        self.worker_fault
            || self.controller().game().is_some() && self.durability != Durability::Clean
    }
    fn idle(&self, allow_fault: bool) -> Result<(), CoordinationError> {
        if self.shutdown != Shutdown::Open || self.running.is_some() {
            return Err(CoordinationError::Busy);
        }
        match self.controller().phase() {
            Phase::Ready | Phase::Failed => Ok(()),
            Phase::Faulted if allow_fault => Ok(()),
            _ => Err(CoordinationError::Busy),
        }
    }
    fn capacity(&self) -> Result<(), CoordinationError> {
        self.capacity_for(1)
    }
    fn capacity_for(&self, count: u64) -> Result<(), CoordinationError> {
        // Keep one distinct ID available for the obligatory final attempt.
        if self.last_storage_id > u64::MAX - 1 - count {
            Err(CoordinationError::Exhausted)
        } else {
            Ok(())
        }
    }
    fn start(
        &mut self,
        effect: Effect,
        intent: StorageIntent,
        final_save: bool,
    ) -> Result<(), CoordinationError> {
        if !final_save {
            self.capacity()?;
        }
        let id = self
            .last_storage_id
            .checked_add(1)
            .ok_or(CoordinationError::Exhausted)?;
        let key = StorageKey {
            id: StorageRequestId::new(id).expect("positive checked counter"),
            revision: self.controller().revision(),
        };
        self.storage.recover_fault();
        self.last_storage_id = id;
        self.running = Some(Running {
            key,
            effect,
            pending: None,
            prior_durability: self.durability,
        });
        let operation = intent.operation();
        if let Err(error) = self.storage.start(key, intent) {
            let failure = StorageFailure {
                operation,
                stage: StorageStage::Worker,
                kind: StorageFailureKind::WorkerFault,
                message: error.to_string().into(),
                visibility: WriteVisibility::Unchanged,
                pending: None,
                cleanup_errors: Box::default(),
            };
            let events = self.accept_storage(StorageEvent::Finished {
                key,
                result: Err(failure),
            });
            self.notifications.extend(events);
        }
        Ok(())
    }
    pub fn dispatch(&mut self, intent: Intent) -> Result<(), CoordinationError> {
        if matches!(intent, Intent::Quit) {
            self.quit();
            return Ok(());
        }
        if matches!(intent, Intent::Cancel) {
            if self.shutdown == Shutdown::Open {
                self.runtime.dispatch(intent)?;
            }
            return Ok(());
        }
        self.idle(false)?;
        if self.durability != Durability::Clean {
            return Err(CoordinationError::Unsaved);
        }
        let canonical = matches!(intent, Intent::Select(_) | Intent::Rewind(_));
        let selection = matches!(intent, Intent::Select(_));
        if selection {
            self.controller().reserve_selection_opening()?;
        }
        // Every generation can produce a turn; reserve autosave capacity before admission.
        self.capacity_for(if selection { 2 } else { 1 })?;
        if matches!(intent, Intent::Rewind(_)) && self.controller().game().is_none() {
            return Err(CoordinationError::NoGame);
        }
        if let Err(error) = self.runtime.dispatch(intent) {
            if matches!(error, RuntimeError::Spawn(_)) {
                self.worker_fault = true;
            }
            return Err(error.into());
        }
        if canonical {
            self.durability = Durability::Dirty;
            self.deferred_opening = selection;
            self.write(false, false)?;
        }
        Ok(())
    }
    fn write(&mut self, copy: bool, final_save: bool) -> Result<(), CoordinationError> {
        let game = self.controller().game().ok_or(CoordinationError::NoGame)?;
        let command = if !copy
            && self
                .retry_write
                .as_ref()
                .is_some_and(|r| r.revision == self.controller().revision())
        {
            SaveGame::Reconcile(self.retry_write.as_ref().unwrap().pending.clone())
        } else {
            let snapshot = SaveSnapshot {
                game: game.clone(),
                source: self.source,
            };
            match (&self.binding, copy) {
                (SaveBinding::Bound { id, stamp, .. }, false) => SaveGame::Replace {
                    target: SaveTarget {
                        id: id.clone(),
                        expected_stamp: *stamp,
                    },
                    snapshot,
                },
                _ => SaveGame::Create(snapshot),
            }
        };
        self.start(
            Effect::Save,
            StorageIntent::Save(Box::new(command)),
            final_save,
        )
    }
    pub fn save(&mut self, copy: bool) -> Result<(), CoordinationError> {
        self.idle(true)?;
        self.capacity_for(if self.deferred_opening { 2 } else { 1 })?;
        self.write(copy, false)
    }
    pub fn list(&mut self, page: SavePage) -> Result<(), CoordinationError> {
        self.idle(true)?;
        self.start(Effect::List, StorageIntent::List(ListSaves { page }), false)
    }
    pub fn load(&mut self, command: LoadGame) -> Result<(), CoordinationError> {
        self.idle(false)?;
        if self.durability != Durability::Clean {
            return Err(CoordinationError::Unsaved);
        }
        self.controller().can_replace_game()?;
        self.capacity_for(2)?;
        self.start(Effect::Load, StorageIntent::Load(command), false)
    }
    /// Admit a startup restore only after source validation and backend construction.
    pub fn admit_loaded(&mut self, loaded: LoadedGame) -> Result<(), CoordinationError> {
        self.idle(false)?;
        self.capacity()?;
        self.validate_source(&loaded)
            .map_err(|_| CoordinationError::Session(SessionError::WrongStage))?;
        let old_id = loaded.stored.metadata.id.clone();
        let backup = loaded.stored.copy == SaveCopy::Backup;
        let unrecognized_fields = loaded.stored.unrecognized_fields.clone();
        self.install_loaded(loaded)?;
        self.notifications.push(PersistenceEvent::Loaded {
            old_id,
            backup,
            unrecognized_fields,
        });
        Ok(())
    }
    fn validate_source(&self, loaded: &LoadedGame) -> Result<(), StorageFailure> {
        validate_loaded_source(loaded, self.source)
    }
}
pub fn validate_loaded_source(
    loaded: &LoadedGame,
    source: StorySource,
) -> Result<(), StorageFailure> {
    if loaded.stored.snapshot.source == source
        && (source == StorySource::Live || loaded.stored.snapshot.game.turns().len() <= 5)
    {
        return Ok(());
    }
    Err(StorageFailure {
        operation: StorageOperation::Load,
        stage: StorageStage::Admission,
        kind: StorageFailureKind::Unsupported,
        message: "save source does not match this session, or demo exceeds five turns".into(),
        visibility: WriteVisibility::Unchanged,
        pending: None,
        cleanup_errors: Box::default(),
    })
}
impl<G, R, F> PersistedSession<G, R, F>
where
    G: StoryGenerator + Send + 'static,
    R: GameRepository + Send + 'static,
    F: Fn(PreparedWriteEvidence) -> R + Send + Sync + 'static,
{
    fn install_loaded(&mut self, loaded: LoadedGame) -> Result<(), CoordinationError> {
        let backup = loaded.stored.copy == SaveCopy::Backup;
        self.runtime.replace_game(loaded.stored.snapshot.game)?;
        self.binding = if backup {
            SaveBinding::Unbound
        } else {
            SaveBinding::Bound {
                id: loaded.stored.metadata.id,
                stamp: loaded.stored.stamp,
                disk_revision: loaded.stored.metadata.revision,
            }
        };
        self.retry_write = None;
        self.storage_failure = None;
        self.deferred_opening = false;
        self.durability = if backup || loaded.changed_by_restore {
            Durability::Dirty
        } else {
            Durability::Clean
        };
        if self.durability == Durability::Dirty {
            self.write(false, false)?;
        }
        Ok(())
    }
    pub fn quit(&mut self) {
        if self.shutdown != Shutdown::Open {
            return;
        }
        self.deferred_opening = false;
        self.shutdown = Shutdown::StoppingGeneration;
        let _ = self.runtime.dispatch(Intent::Quit);
        if self
            .running
            .as_ref()
            .is_some_and(|r| r.effect != Effect::Save)
        {
            self.storage.cancel();
        }
    }
    pub fn output_drained(&mut self) {
        if self.shutdown == Shutdown::DrainingOutput {
            self.shutdown = Shutdown::Closed;
        }
    }
    pub fn poll(&mut self) -> Vec<PersistenceEvent> {
        let mut events = std::mem::take(&mut self.notifications);
        for event in self.runtime.poll_events() {
            if event.worker_fault || self.controller().phase() == Phase::Faulted {
                self.worker_fault = true;
            }
            let save = event.change == Some(CanonicalChange::Turn);
            events.push(PersistenceEvent::Generation {
                acceptance: event.acceptance,
                change: event.change,
            });
            if save {
                self.durability = Durability::Dirty;
                // Capacity was checked when admitting generation.
                self.write(false, false)
                    .expect("autosave capacity reserved at admission");
            }
        }
        for event in self.storage.poll() {
            events.extend(self.accept_storage(event));
        }
        self.advance_shutdown();
        events
    }
    /// Key and base revision checks apply even to synthetic/late completions.
    fn accept_storage(&mut self, event: StorageEvent) -> Vec<PersistenceEvent> {
        let key = match &event {
            StorageEvent::Prepared { key, .. } | StorageEvent::Finished { key, .. } => *key,
        };
        if !self.running.as_ref().is_some_and(|r| r.key == key)
            || key.revision != self.controller().revision()
        {
            if let StorageEvent::Finished { result, .. } = &event {
                match result {
                    Err(failure) => self.storage_failure = Some(failure.clone()),
                    Ok(StorageOutcome::Saved(receipt)) => {
                        self.storage_failure = Some(StorageFailure {
                            operation: StorageOperation::Replace,
                            stage: StorageStage::Worker,
                            kind: StorageFailureKind::Conflict,
                            message: "stale storage receipt ignored; its physical write may exist"
                                .into(),
                            visibility: WriteVisibility::Replaced {
                                stamp: receipt.stamp,
                            },
                            pending: None,
                            cleanup_errors: Box::default(),
                        });
                    }
                    _ => (),
                }
            }
            return vec![PersistenceEvent::Ignored];
        }
        match event {
            StorageEvent::Prepared { pending, .. } => {
                self.running.as_mut().unwrap().pending = Some(pending);
                vec![]
            }
            StorageEvent::Finished { result, .. } => {
                let running = self.running.take().unwrap();
                if self.shutdown != Shutdown::Open && running.effect != Effect::Save {
                    return vec![PersistenceEvent::Ignored];
                }
                let result = result.and_then(|outcome| {
                    if let StorageOutcome::Loaded(loaded) = &outcome {
                        self.validate_source(loaded)?;
                    }
                    Ok(outcome)
                });
                match result {
                    Ok(StorageOutcome::Saved(receipt)) if running.effect == Effect::Save => {
                        self.binding = SaveBinding::Bound {
                            id: receipt.metadata.id.clone(),
                            stamp: receipt.stamp,
                            disk_revision: receipt.metadata.revision,
                        };
                        self.durability = Durability::Clean;
                        self.retry_write = None;
                        self.storage_failure = None;
                        if self.shutdown != Shutdown::Open {
                            self.closing_write_satisfied = true;
                        }
                        if self.deferred_opening && self.shutdown == Shutdown::Open {
                            self.deferred_opening = false;
                            if self
                                .runtime
                                .dispatch(Intent::Turn(TurnDirection::Continue))
                                .is_err()
                            {
                                self.worker_fault = true;
                            }
                        }
                        vec![PersistenceEvent::Saved(receipt)]
                    }
                    Ok(StorageOutcome::Loaded(loaded)) if running.effect == Effect::Load => {
                        let old_id = loaded.stored.metadata.id.clone();
                        let backup = loaded.stored.copy == SaveCopy::Backup;
                        let unrecognized_fields = loaded.stored.unrecognized_fields.clone();
                        self.install_loaded(*loaded)
                            .expect("load transition capacity reserved at admission");
                        vec![PersistenceEvent::Loaded {
                            old_id,
                            backup,
                            unrecognized_fields,
                        }]
                    }
                    Ok(StorageOutcome::Listed(page)) if running.effect == Effect::List => {
                        vec![PersistenceEvent::Listed(page)]
                    }
                    Ok(_) => vec![PersistenceEvent::Ignored],
                    Err(mut failure) => {
                        if running.effect == Effect::Save {
                            self.durability = if failure.visibility == WriteVisibility::Unchanged {
                                if running.prior_durability == Durability::Uncertain {
                                    Durability::Uncertain
                                } else {
                                    Durability::Dirty
                                }
                            } else {
                                Durability::Uncertain
                            };
                            if failure.pending.is_none() {
                                failure.pending = running.pending.map(Box::new);
                            }
                            if failure.visibility != WriteVisibility::Unchanged {
                                self.retry_write =
                                    failure
                                        .pending
                                        .as_deref()
                                        .cloned()
                                        .map(|pending| RetryWrite {
                                            revision: key.revision,
                                            pending,
                                        });
                            }
                        }
                        self.storage_failure = Some(failure.clone());
                        vec![PersistenceEvent::Failed(failure)]
                    }
                }
            }
        }
    }
    fn advance_shutdown(&mut self) {
        if self.shutdown == Shutdown::StoppingGeneration
            && self.controller().phase() == Phase::Closed
            && self.running.is_none()
        {
            if self.controller().game().is_none() || self.closing_write_satisfied {
                self.finish_storage();
            } else {
                self.shutdown = Shutdown::SavingFinal;
                self.final_attempted = true;
                if self.write(false, true).is_err() {
                    self.finish_storage();
                }
            }
        }
        if self.shutdown == Shutdown::SavingFinal && self.final_attempted && self.running.is_none()
        {
            self.finish_storage();
        }
    }
    fn finish_storage(&mut self) {
        self.storage.close();
        self.shutdown = Shutdown::DrainingOutput;
    }
}

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod tests;

/// Operations needed by the headless driver, independent of generator/repository
/// composition. Canonical queries remain borrowed; effects own their inputs.
pub trait HeadlessSession {
    fn controller(&self) -> &SessionController;
    fn dispatch(&mut self, intent: Intent) -> Result<(), CoordinationError>;
    fn save(&mut self, copy: bool) -> Result<(), CoordinationError>;
    fn list(&mut self, page: SavePage) -> Result<(), CoordinationError>;
    fn load(&mut self, command: LoadGame) -> Result<(), CoordinationError>;
    fn poll(&mut self) -> Vec<PersistenceEvent>;
    fn binding(&self) -> &SaveBinding;
    fn durability(&self) -> Durability;
    fn storage_busy(&self) -> bool;
    fn storage_failure(&self) -> Option<&StorageFailure>;
    fn shutdown(&self) -> Shutdown;
    fn output_drained(&mut self);
    fn exit_failed(&self) -> bool;
}
impl<G, R, F> HeadlessSession for PersistedSession<G, R, F>
where
    G: StoryGenerator + Send + 'static,
    R: GameRepository + Send + 'static,
    F: Fn(PreparedWriteEvidence) -> R + Send + Sync + 'static,
{
    fn controller(&self) -> &SessionController {
        self.controller()
    }
    fn dispatch(&mut self, intent: Intent) -> Result<(), CoordinationError> {
        self.dispatch(intent)
    }
    fn save(&mut self, copy: bool) -> Result<(), CoordinationError> {
        self.save(copy)
    }
    fn list(&mut self, page: SavePage) -> Result<(), CoordinationError> {
        self.list(page)
    }
    fn load(&mut self, command: LoadGame) -> Result<(), CoordinationError> {
        self.load(command)
    }
    fn poll(&mut self) -> Vec<PersistenceEvent> {
        self.poll()
    }
    fn binding(&self) -> &SaveBinding {
        self.binding()
    }
    fn durability(&self) -> Durability {
        self.durability()
    }
    fn storage_busy(&self) -> bool {
        self.storage_busy()
    }
    fn storage_failure(&self) -> Option<&StorageFailure> {
        self.storage_failure()
    }
    fn shutdown(&self) -> Shutdown {
        self.shutdown()
    }
    fn output_drained(&mut self) {
        self.output_drained()
    }
    fn exit_failed(&self) -> bool {
        self.exit_failed()
    }
}
