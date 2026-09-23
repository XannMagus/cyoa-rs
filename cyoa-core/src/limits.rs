//! Bounds that shape generation and validation. Each is its own type so a
//! limit cannot be swapped for another limit or for an unrelated count, and
//! each carries only the values that are actually legal for it.

use std::num::NonZeroUsize;

use thiserror::Error;

/// Cap on `major_events` (calibre `MAX_MAJOR_EVENTS`). Zero would erase all
/// story memory every turn, so this is `NonZeroUsize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MajorEventLimit(NonZeroUsize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the major event limit must be positive")]
pub struct InvalidMajorEventLimit;

impl MajorEventLimit {
    pub fn new(limit: usize) -> Result<Self, InvalidMajorEventLimit> {
        NonZeroUsize::new(limit)
            .map(Self)
            .ok_or(InvalidMajorEventLimit)
    }

    pub fn get(self) -> usize {
        self.0.get()
    }
}

impl Default for MajorEventLimit {
    fn default() -> Self {
        Self(NonZeroUsize::new(30).expect("30 is positive"))
    }
}

/// Cap on world-generated NPCs (calibre `MAX_GENERATED_NPCS`). Zero is
/// legitimate: a world may start with nobody else in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxGeneratedNpcs(usize);

impl MaxGeneratedNpcs {
    pub fn new(limit: usize) -> Self {
        Self(limit)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

impl Default for MaxGeneratedNpcs {
    fn default() -> Self {
        Self(8)
    }
}

/// Turns of the previous chapter carried forward as a prose bridge (calibre
/// `MIN_PROSE_CONTEXT_TURNS`). Zero means no bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProseBridgeTurns(usize);

impl ProseBridgeTurns {
    pub fn new(turns: usize) -> Self {
        Self(turns)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

impl Default for ProseBridgeTurns {
    fn default() -> Self {
        Self(3)
    }
}

/// Floor on usable playable characters (calibre `MIN_PLAYER_CHARACTERS`).
/// Zero would admit a cast with no protagonist, so this is `NonZeroUsize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinPlayableCharacters(NonZeroUsize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the minimum number of playable characters must be positive")]
pub struct InvalidMinPlayableCharacters;

impl MinPlayableCharacters {
    pub fn new(minimum: usize) -> Result<Self, InvalidMinPlayableCharacters> {
        NonZeroUsize::new(minimum)
            .map(Self)
            .ok_or(InvalidMinPlayableCharacters)
    }

    pub fn get(self) -> usize {
        self.0.get()
    }
}

impl Default for MinPlayableCharacters {
    fn default() -> Self {
        Self(NonZeroUsize::new(2).expect("2 is positive"))
    }
}

/// Every bound the engine and prompts must agree on, injected at startup from
/// config; never part of a saved game (see PLAN.md's `[limits]` note).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limits {
    pub max_major_events: MajorEventLimit,
    pub max_generated_npcs: MaxGeneratedNpcs,
    pub prose_bridge_turns: ProseBridgeTurns,
    pub min_playable_characters: MinPlayableCharacters,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_rejected_for_nonzero_limits() {
        assert!(MajorEventLimit::new(0).is_err());
        assert!(MinPlayableCharacters::new(0).is_err());
    }

    #[test]
    fn zero_is_accepted_for_count_limits() {
        assert_eq!(MaxGeneratedNpcs::new(0).get(), 0);
        assert_eq!(ProseBridgeTurns::new(0).get(), 0);
    }

    #[test]
    fn defaults_match_calibre_constants() {
        let limits = Limits::default();
        assert_eq!(limits.max_major_events.get(), 30);
        assert_eq!(limits.max_generated_npcs.get(), 8);
        assert_eq!(limits.prose_bridge_turns.get(), 3);
        assert_eq!(limits.min_playable_characters.get(), 2);
    }
}
