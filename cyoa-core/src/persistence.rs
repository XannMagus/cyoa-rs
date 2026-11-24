//! Business save identity, snapshots and repository capabilities.
use crate::game::{GameState, TurnCount};
use thiserror::Error;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReceipt {
    pub metadata: SaveMetadata,
    pub stamp: ContentStamp,
}

pub trait SaveAttempt: Send + Sync {
    fn target(&self) -> &SaveId;
    fn previous_stamp(&self) -> Option<ContentStamp>;
    fn intended_stamp(&self) -> ContentStamp;
}
/// The executor retains a separate observer capture across worker failure.
pub trait AttemptObserver<A> {
    fn publish(&self, attempt: A);
}
pub trait StoredReport {
    fn snapshot(&self) -> &SaveSnapshot;
    fn metadata(&self) -> &SaveMetadata;
    fn stamp(&self) -> ContentStamp;
    fn copy(&self) -> SaveCopy;
}
pub trait GameRepository {
    type Context: crate::generation::GenerationControl + ?Sized;
    type Failure: std::error::Error;
    type Attempt: SaveAttempt;
    type Stored: StoredReport;
    fn create(
        &mut self,
        snapshot: SaveSnapshot,
        prepared: &dyn AttemptObserver<Self::Attempt>,
        context: &Self::Context,
    ) -> Result<SaveReceipt, Self::Failure>;
    fn replace(
        &mut self,
        target: SaveTarget,
        snapshot: SaveSnapshot,
        prepared: &dyn AttemptObserver<Self::Attempt>,
        context: &Self::Context,
    ) -> Result<SaveReceipt, Self::Failure>;
    fn reconcile(
        &mut self,
        attempt: Self::Attempt,
        prepared: &dyn AttemptObserver<Self::Attempt>,
        context: &Self::Context,
    ) -> Result<SaveReceipt, Self::Failure>;
    fn load(
        &mut self,
        id: &SaveId,
        copy: SaveCopy,
        context: &Self::Context,
    ) -> Result<Self::Stored, Self::Failure>;
}
