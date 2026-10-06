//! Persistence commands, queries and inward-owned repository contract.
//! JSON, paths, clocks and filesystem effects belong to infrastructure.
use crate::cancellation::CancellationToken;
use cyoa_core::{
    game::{GameState, TurnCount},
    limits::RestoreLimits,
    text::WorldTitle,
};
use thiserror::Error;

pub const MAX_SAVE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SaveId(String);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("save ID must be a 1–40 character ASCII slug followed by 32 lowercase hex digits")]
pub struct InvalidSaveId;
impl SaveId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidSaveId> {
        let value = value.into();
        let (slug, suffix) = value.rsplit_once('-').ok_or(InvalidSaveId)?;
        if !(1..=40).contains(&slug.len())
            || suffix.len() != 32
            || !suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !slug.split('-').all(|word| {
                !word.is_empty()
                    && word
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            })
        {
            return Err(InvalidSaveId);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("persistence counter must be positive and must not overflow")]
pub struct InvalidCounter;
macro_rules! counter {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name(std::num::NonZeroU64);
        impl $name {
            pub fn new(value: u64) -> Result<Self, InvalidCounter> {
                std::num::NonZeroU64::new(value)
                    .map(Self)
                    .ok_or(InvalidCounter)
            }
            pub fn get(self) -> u64 {
                self.0.get()
            }
            pub fn next(self) -> Result<Self, InvalidCounter> {
                self.get()
                    .checked_add(1)
                    .ok_or(InvalidCounter)
                    .and_then(Self::new)
            }
        }
    };
}
counter!(SaveRevision);
counter!(StorageRequestId);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentStamp([u8; 32]);
impl ContentStamp {
    /// The repository supplies the SHA-256 of the exact encoded document.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedAt {
    seconds: i64,
    nanos: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("save time must be a UTC instant within years 0001–9999 with valid nanoseconds")]
pub struct InvalidSavedAt;
impl SavedAt {
    /// Save metadata uses millisecond precision; finer clock precision is rounded
    /// down once at this boundary, never used for write ordering or freshness.
    pub fn new(seconds: i64, nanos: u32) -> Result<Self, InvalidSavedAt> {
        if !(-62_135_596_800..=253_402_300_799).contains(&seconds) || nanos >= 1_000_000_000 {
            return Err(InvalidSavedAt);
        }
        Ok(Self {
            seconds,
            nanos: nanos / 1_000_000 * 1_000_000,
        })
    }
    pub fn unix_seconds(self) -> i64 {
        self.seconds
    }
    pub fn nanoseconds(self) -> u32 {
        self.nanos
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoScenarioId {
    HarbourV1,
}
impl DemoScenarioId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HarbourV1 => "harbour-v1",
        }
    }
    /// The scripted scenario's fixed length; a demo game never holds more turns.
    pub fn passages(self) -> TurnCount {
        match self {
            Self::HarbourV1 => TurnCount::new(5).expect("nonzero literal"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorySource {
    Live,
    Demo { scenario: DemoScenarioId },
}
impl StorySource {
    /// Whether a game with this many turns can come from this source.
    pub fn admits(self, turns: usize) -> bool {
        match self {
            Self::Live => true,
            Self::Demo { scenario } => turns <= scenario.passages().get(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{scenario} has only {passages} passages, not {turns}", scenario = .scenario.as_str(), passages = .scenario.passages().get())]
pub struct DemoTooLong {
    pub scenario: DemoScenarioId,
    pub turns: usize,
}
/// A game paired with the source it was played from. Construction checks that
/// the source could have produced the game: a demo snapshot never exceeds its
/// scenario's passages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSnapshot {
    game: GameState,
    source: StorySource,
}
impl SaveSnapshot {
    pub fn new(game: GameState, source: StorySource) -> Result<Self, DemoTooLong> {
        match source {
            StorySource::Demo { scenario } if !source.admits(game.turns().len()) => {
                Err(DemoTooLong {
                    scenario,
                    turns: game.turns().len(),
                })
            }
            _ => Ok(Self { game, source }),
        }
    }
    pub fn game(&self) -> &GameState {
        &self.game
    }
    pub fn source(&self) -> StorySource {
        self.source
    }
    pub fn into_game(self) -> GameState {
        self.game
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveTarget {
    pub id: SaveId,
    pub expected_stamp: ContentStamp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveMetadata {
    pub id: SaveId,
    pub revision: SaveRevision,
    pub saved_at: SavedAt,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveCopy {
    Primary,
    Backup,
}
/// Where a save held an optional field this version does not understand. It is
/// reported to the player and omitted on re-save; the location is display text
/// only, never parsed by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnrecognizedField(Box<str>);
impl UnrecognizedField {
    pub fn new(location: impl Into<Box<str>>) -> Self {
        Self(location.into())
    }
    pub fn location(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredGame {
    pub snapshot: SaveSnapshot,
    pub metadata: SaveMetadata,
    pub stamp: ContentStamp,
    pub copy: SaveCopy,
    pub unrecognized_fields: Vec<UnrecognizedField>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedGame {
    pub stored: StoredGame,
    pub changed_by_restore: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReceipt {
    pub metadata: SaveMetadata,
    pub stamp: ContentStamp,
}
/// Opaque prepared-write evidence; application never parses or edits these bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingWrite {
    target: SaveId,
    previous_stamp: Option<ContentStamp>,
    intended_stamp: ContentStamp,
    // Shared, not copied: a prepared write is retained by the caller's evidence
    // sink, a retry and failures at once, and can be up to MAX_SAVE_BYTES.
    bytes: std::sync::Arc<[u8]>,
}
impl PendingWrite {
    pub fn new(
        target: SaveId,
        previous_stamp: Option<ContentStamp>,
        intended_stamp: ContentStamp,
        bytes: Vec<u8>,
    ) -> Result<Self, InvalidPendingWrite> {
        if bytes.is_empty() || bytes.len() > MAX_SAVE_BYTES {
            return Err(InvalidPendingWrite);
        }
        Ok(Self {
            target,
            previous_stamp,
            intended_stamp,
            bytes: bytes.into(),
        })
    }
    pub fn target(&self) -> &SaveId {
        &self.target
    }
    pub fn previous_stamp(&self) -> Option<ContentStamp> {
        self.previous_stamp
    }
    pub fn intended_stamp(&self) -> ContentStamp {
        self.intended_stamp
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("prepared save bytes must be nonempty and within the save size bound")]
pub struct InvalidPendingWrite;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteVisibility {
    Unchanged,
    Replaced { stamp: ContentStamp },
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageOperation {
    Create,
    Replace,
    Reconcile,
    Load,
    List,
    Inspect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageStage {
    Admission,
    Read,
    Decode,
    Prepare,
    Lock,
    Write,
    Backup,
    Replace,
    Sync,
    Cleanup,
    Worker,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageFailureKind {
    NotFound,
    Busy,
    Conflict,
    Corrupt {
        location: String,
    },
    FutureVersion {
        found: SaveFormatVersion,
        supported: SaveFormatVersion,
    },
    Unsupported,
    TooLarge,
    Io,
    Cancelled,
    Timeout,
    WorkerFault,
}
/// A failed storage operation. Fields are private so a failure is built through
/// constructors: `Replaced` visibility only comes from [`StorageFailure::replaced`],
/// which takes the stamp from the pending write itself, so the two cannot disagree.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct StorageFailure {
    operation: StorageOperation,
    stage: StorageStage,
    kind: StorageFailureKind,
    message: Box<str>,
    visibility: WriteVisibility,
    pending: Option<Box<PendingWrite>>,
    cleanup_errors: Box<[String]>,
}
impl StorageFailure {
    /// A failure that changed nothing and has no prepared write attached.
    pub fn new(
        operation: StorageOperation,
        stage: StorageStage,
        kind: StorageFailureKind,
        message: impl Into<Box<str>>,
    ) -> Self {
        Self {
            operation,
            stage,
            kind,
            message: message.into(),
            visibility: WriteVisibility::Unchanged,
            pending: None,
            cleanup_errors: Box::default(),
        }
    }
    pub fn cancelled(operation: StorageOperation) -> Self {
        Self::new(
            operation,
            StorageStage::Admission,
            StorageFailureKind::Cancelled,
            "storage operation cancelled",
        )
    }
    pub fn with_operation(mut self, operation: StorageOperation) -> Self {
        self.operation = operation;
        self
    }
    pub fn with_stage(mut self, stage: StorageStage) -> Self {
        self.stage = stage;
        self
    }
    pub fn with_kind(mut self, kind: StorageFailureKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn with_cleanup_errors(mut self, errors: impl IntoIterator<Item = String>) -> Self {
        self.cleanup_errors = self
            .cleanup_errors
            .into_vec()
            .into_iter()
            .chain(errors)
            .collect();
        self
    }
    /// Attaches the prepared write without claiming anything became visible. If a
    /// different write was already recorded as replaced, this attempt's outcome is
    /// unknown rather than silently inheriting that claim.
    pub fn prepared(mut self, pending: PendingWrite) -> Self {
        if matches!(self.visibility, WriteVisibility::Replaced { stamp } if stamp != pending.intended_stamp())
        {
            self.visibility = WriteVisibility::Unknown;
        }
        self.pending = Some(Box::new(pending));
        self
    }
    /// A durable write confirmed by its receipt, with no prepared write in hand
    /// (for example, a receipt that arrived for a request no longer current).
    pub fn observed_replacement(mut self, receipt: &SaveReceipt) -> Self {
        self.visibility = WriteVisibility::Replaced {
            stamp: receipt.stamp,
        };
        self.pending = None;
        self
    }
    /// The prepared write is now the primary on disk.
    pub fn replaced(mut self, pending: PendingWrite) -> Self {
        self.visibility = WriteVisibility::Replaced {
            stamp: pending.intended_stamp(),
        };
        self.pending = Some(Box::new(pending));
        self
    }
    /// Whether anything became visible cannot be established.
    pub fn visibility_unknown(mut self) -> Self {
        self.visibility = WriteVisibility::Unknown;
        self
    }
    pub fn operation(&self) -> StorageOperation {
        self.operation
    }
    pub fn stage(&self) -> StorageStage {
        self.stage
    }
    pub fn kind(&self) -> StorageFailureKind {
        self.kind.clone()
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn visibility(&self) -> WriteVisibility {
        self.visibility
    }
    pub fn pending(&self) -> Option<&PendingWrite> {
        self.pending.as_deref()
    }
    pub fn into_pending(self) -> Option<PendingWrite> {
        self.pending.map(|pending| *pending)
    }
    pub fn cleanup_errors(&self) -> &[String] {
        &self.cleanup_errors
    }
}
/// A save file format version; always positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SaveFormatVersion(std::num::NonZeroU32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("save format versions start at 1")]
pub struct InvalidSaveFormatVersion;
impl SaveFormatVersion {
    pub const fn from_nonzero(version: std::num::NonZeroU32) -> Self {
        Self(version)
    }
    pub fn new(version: u32) -> Result<Self, InvalidSaveFormatVersion> {
        std::num::NonZeroU32::new(version)
            .map(Self)
            .ok_or(InvalidSaveFormatVersion)
    }
    pub fn get(self) -> u32 {
        self.0.get()
    }
}
/// Entries per listing page, 1 to [`PageSize::MAX`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageSize(u8);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("save listing page size must be between 1 and 100")]
pub struct InvalidSavePage;
impl PageSize {
    pub const MAX: Self = Self(100);
    pub fn new(size: u8) -> Result<Self, InvalidSavePage> {
        if (1..=Self::MAX.0).contains(&size) {
            Ok(Self(size))
        } else {
            Err(InvalidSavePage)
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePage {
    after: Option<SaveId>,
    size: PageSize,
}
impl SavePage {
    pub fn new(after: Option<SaveId>, size: PageSize) -> Self {
        Self { after, size }
    }
    pub fn after(&self) -> Option<&SaveId> {
        self.after.as_ref()
    }
    pub fn size(&self) -> PageSize {
        self.size
    }
}
/// The number of turns recorded in a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SavedTurnCount(usize);
impl SavedTurnCount {
    pub fn new(turns: usize) -> Self {
        Self(turns)
    }
    pub fn get(self) -> usize {
        self.0
    }
}
/// What a listing shows for a readable primary save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSummary {
    pub title: WorldTitle,
    pub saved_at: SavedAt,
    pub turns: SavedTurnCount,
    pub source: StorySource,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupCopy {
    Present,
    Absent,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimaryCopy {
    Valid(SaveSummary),
    /// No primary file; with a present backup this is a backup-only slot.
    Missing,
    Corrupt,
    FutureVersion {
        version: SaveFormatVersion,
    },
    Unreadable,
}
/// A listed slot. Both copies are only reported when the slot was inspected
/// under its lock; a busy or unlockable slot makes no claim about either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveListingStatus {
    Inspected {
        primary: PrimaryCopy,
        backup: BackupCopy,
    },
    Busy,
    Unreadable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveListing {
    pub id: SaveId,
    pub status: SaveListingStatus,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePageResult {
    pub entries: Vec<SaveListing>,
    pub next: Option<SaveId>,
}

/// Storage port. Every write receives the caller's [`PreparedWriteEvidence`]:
/// an implementation must publish the exact [`PendingWrite`] to it before its
/// first disk mutation, so a caller whose worker unwinds or is lost can still
/// reconcile what may have become visible. Publishing again for the same
/// attempt is allowed; a write that never prepares (for example, rejected at
/// admission) publishes nothing.
pub trait GameRepository {
    fn create(
        &mut self,
        snapshot: SaveSnapshot,
        prepared: &PreparedWriteEvidence,
        cancel: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure>;
    fn replace(
        &mut self,
        target: SaveTarget,
        snapshot: SaveSnapshot,
        prepared: &PreparedWriteEvidence,
        cancel: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure>;
    fn reconcile(
        &mut self,
        attempt: PendingWrite,
        prepared: &PreparedWriteEvidence,
        cancel: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure>;
    fn load(
        &mut self,
        id: &SaveId,
        copy: SaveCopy,
        cancel: &CancellationToken,
    ) -> Result<StoredGame, StorageFailure>;
    fn list(
        &mut self,
        page: SavePage,
        cancel: &CancellationToken,
    ) -> Result<SavePageResult, StorageFailure>;
}
#[derive(Debug)]
pub enum SaveGame {
    Create(SaveSnapshot),
    Replace {
        target: SaveTarget,
        snapshot: SaveSnapshot,
    },
    Reconcile(PendingWrite),
}
#[derive(Debug)]
pub struct LoadGame {
    pub id: SaveId,
    pub copy: SaveCopy,
    pub limits: RestoreLimits,
}
#[derive(Debug)]
pub struct InspectSave {
    pub id: SaveId,
    pub copy: SaveCopy,
}
#[derive(Debug)]
pub struct ListSaves {
    pub page: SavePage,
}
pub struct PersistenceUseCases<R> {
    repository: R,
}
impl<R: GameRepository> PersistenceUseCases<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
    pub fn into_repository(self) -> R {
        self.repository
    }
    pub fn save_game(
        &mut self,
        command: SaveGame,
        prepared: &PreparedWriteEvidence,
        cancel: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        let operation = match &command {
            SaveGame::Create(_) => StorageOperation::Create,
            SaveGame::Replace { .. } => StorageOperation::Replace,
            SaveGame::Reconcile(_) => StorageOperation::Reconcile,
        };
        check_cancelled(cancel, operation)?;
        // A durable write is an observed effect: do not erase a receipt because
        // cancellation arrived after the repository completed it.
        match command {
            SaveGame::Create(snapshot) => self.repository.create(snapshot, prepared, cancel),
            SaveGame::Replace { target, snapshot } => {
                self.repository.replace(target, snapshot, prepared, cancel)
            }
            SaveGame::Reconcile(attempt) => self.repository.reconcile(attempt, prepared, cancel),
        }
    }
    pub fn load_game(
        &mut self,
        command: LoadGame,
        cancel: &CancellationToken,
    ) -> Result<LoadedGame, StorageFailure> {
        check_cancelled(cancel, StorageOperation::Load)?;
        let StoredGame {
            snapshot,
            metadata,
            stamp,
            copy,
            unrecognized_fields,
        } = self.repository.load(&command.id, command.copy, cancel)?;
        check_cancelled(cancel, StorageOperation::Load)?;
        let source = snapshot.source();
        let stored_limits = snapshot.game().limits();
        // Consumed, not cloned: a 64 MiB story is rebound in place or returned as is.
        let restored = snapshot.into_game().with_restore_limits(command.limits);
        let changed_by_restore = restored.limits() != stored_limits;
        check_cancelled(cancel, StorageOperation::Load)?;
        // Restoring never changes the turn log, so the source still admits it.
        let snapshot = SaveSnapshot::new(restored, source).map_err(|e| {
            StorageFailure::new(
                StorageOperation::Load,
                StorageStage::Decode,
                StorageFailureKind::Corrupt {
                    location: "source".into(),
                },
                e.to_string(),
            )
        })?;
        Ok(LoadedGame {
            stored: StoredGame {
                snapshot,
                metadata,
                stamp,
                copy,
                unrecognized_fields,
            },
            changed_by_restore,
        })
    }
    pub fn inspect_save(
        &mut self,
        query: InspectSave,
        cancel: &CancellationToken,
    ) -> Result<StoredGame, StorageFailure> {
        check_cancelled(cancel, StorageOperation::Inspect)?;
        let stored = self.repository.load(&query.id, query.copy, cancel)?;
        check_cancelled(cancel, StorageOperation::Inspect)?;
        Ok(stored)
    }
    pub fn list_saves(
        &mut self,
        query: ListSaves,
        cancel: &CancellationToken,
    ) -> Result<SavePageResult, StorageFailure> {
        check_cancelled(cancel, StorageOperation::List)?;
        let result = self.repository.list(query.page, cancel)?;
        check_cancelled(cancel, StorageOperation::List)?;
        Ok(result)
    }
}
fn check_cancelled(
    cancel: &CancellationToken,
    operation: StorageOperation,
) -> Result<(), StorageFailure> {
    if cancel.is_cancelled() {
        Err(StorageFailure::cancelled(operation))
    } else {
        Ok(())
    }
}

/// Request-scoped preparation evidence: the caller keeps one clone and passes the
/// other to a [`GameRepository`] write, which publishes before mutating disk.
/// The caller's clone survives even when the worker running the write unwinds.
#[derive(Clone, Default)]
pub struct PreparedWriteEvidence(std::sync::Arc<std::sync::Mutex<Option<PendingWrite>>>);
impl PreparedWriteEvidence {
    pub fn publish(&self, pending: PendingWrite) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(pending);
    }
    pub fn pending(&self) -> Option<PendingWrite> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
