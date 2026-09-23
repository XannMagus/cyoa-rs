//! The generated game world: an outline plus a validated cast.
//! Ports calibre's `validated_world`/`validated_player_characters`/
//! `validated_npcs`/`validated_cast` (cyoa.py:1167-1222).

use std::collections::HashSet;

use crate::{
    limits::Limits,
    text::{
        Backstory, CharacterDescription, CharacterName, Relationships, WorldDescription, WorldTitle,
    },
};
use thiserror::Error;

/// What the first generation call produces: no cast yet, so the player edits
/// the world before the cast is generated to match it (calibre's `WorldOutline`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldOutline {
    title: WorldTitle,
    description: WorldDescription,
}

impl WorldOutline {
    pub fn new(title: WorldTitle, description: WorldDescription) -> Self {
        Self { title, description }
    }

    pub fn title(&self) -> &WorldTitle {
        &self.title
    }

    pub fn description(&self) -> &WorldDescription {
        &self.description
    }
}

/// A character the player may choose to play. Every field is required: unlike
/// a story summary's cast there is no previous state to repair these from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerCharacter {
    name: CharacterName,
    description: CharacterDescription,
    backstory: Backstory,
}

impl PlayerCharacter {
    pub fn new(
        name: CharacterName,
        description: CharacterDescription,
        backstory: Backstory,
    ) -> Self {
        Self {
            name,
            description,
            backstory,
        }
    }

    pub fn name(&self) -> &CharacterName {
        &self.name
    }

    pub fn description(&self) -> &CharacterDescription {
        &self.description
    }

    pub fn backstory(&self) -> &Backstory {
        &self.backstory
    }
}

/// A character who lives in the world but cannot be played.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NonPlayerCharacter {
    name: CharacterName,
    description: CharacterDescription,
    backstory: Backstory,
    relationships: Option<Relationships>,
}

impl NonPlayerCharacter {
    pub fn new(
        name: CharacterName,
        description: CharacterDescription,
        backstory: Backstory,
        relationships: Option<Relationships>,
    ) -> Self {
        Self {
            name,
            description,
            backstory,
            relationships,
        }
    }

    pub fn name(&self) -> &CharacterName {
        &self.name
    }

    pub fn description(&self) -> &CharacterDescription {
        &self.description
    }

    pub fn backstory(&self) -> &Backstory {
        &self.backstory
    }

    pub fn relationships(&self) -> Option<&Relationships> {
        self.relationships.as_ref()
    }
}

/// A validated index into a `WorldCast`'s playable characters. The only way
/// to obtain one is `WorldCast::playable_index`, so it cannot go out of range
/// so long as playables are never removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayableIndex(usize);

impl PlayableIndex {
    fn get(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a world needs at least {minimum} usable playable characters, found {found}")]
pub struct TooFewPlayableCharacters {
    minimum: usize,
    found: usize,
}

/// The generated cast of a world: playable characters and NPCs, deduplicated
/// and capped. Constructing one requires no LLM call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldCast {
    playable: Vec<PlayerCharacter>,
    npcs: Vec<NonPlayerCharacter>,
}

impl WorldCast {
    /// Deduplicates complete records within each role, preserving order, then
    /// caps NPCs. Deliberately differs from calibre's name-based deduplication:
    /// a shared name does not establish identity, including across roles.
    pub fn new(
        playable: impl IntoIterator<Item = PlayerCharacter>,
        npcs: impl IntoIterator<Item = NonPlayerCharacter>,
        limits: &Limits,
    ) -> Result<Self, TooFewPlayableCharacters> {
        let mut seen = HashSet::new();
        let playable: Vec<_> = playable
            .into_iter()
            .filter(|c| seen.insert(c.clone()))
            .collect();
        let minimum = limits.min_playable_characters.get();
        if playable.len() < minimum {
            return Err(TooFewPlayableCharacters {
                minimum,
                found: playable.len(),
            });
        }
        let mut seen = HashSet::new();
        let selected_npcs = npcs
            .into_iter()
            .filter(|npc| seen.insert(npc.clone()))
            .take(limits.max_generated_npcs.get())
            .collect();
        Ok(Self {
            playable,
            npcs: selected_npcs,
        })
    }

    pub fn playable(&self) -> &[PlayerCharacter] {
        &self.playable
    }

    pub fn npcs(&self) -> &[NonPlayerCharacter] {
        &self.npcs
    }

    /// The checked index type accepted by `GameState::start`.
    pub fn playable_index(&self, index: usize) -> Option<PlayableIndex> {
        (index < self.playable.len()).then_some(PlayableIndex(index))
    }

    pub fn playable_at(&self, index: PlayableIndex) -> &PlayerCharacter {
        &self.playable[index.get()]
    }
}

/// A world outline paired with its validated cast: what a game is played in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    outline: WorldOutline,
    cast: WorldCast,
}

impl World {
    pub fn new(outline: WorldOutline, cast: WorldCast) -> Self {
        Self { outline, cast }
    }

    pub fn outline(&self) -> &WorldOutline {
        &self.outline
    }

    pub fn cast(&self) -> &WorldCast {
        &self.cast
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(name: &str) -> PlayerCharacter {
        PlayerCharacter::new(
            CharacterName::new(name).unwrap(),
            CharacterDescription::new("description").unwrap(),
            Backstory::new("backstory").unwrap(),
        )
    }

    fn npc(name: &str) -> NonPlayerCharacter {
        NonPlayerCharacter::new(
            CharacterName::new(name).unwrap(),
            CharacterDescription::new("description").unwrap(),
            Backstory::new("backstory").unwrap(),
            None,
        )
    }

    #[test]
    fn too_few_playable_characters_is_rejected() {
        let limits = Limits::default();
        let err = WorldCast::new([player("Solo")], [], &limits).unwrap_err();
        assert_eq!(err.minimum, 2);
        assert_eq!(err.found, 1);
    }

    #[test]
    fn exact_records_collapse_within_each_role_but_shared_names_survive() {
        let limits = Limits::default();
        let cast = WorldCast::new(
            [player("Alex"), player("Blair"), player("Alex")],
            [npc("Alex"), npc("Casey"), npc("Alex")],
            &limits,
        )
        .unwrap();
        assert_eq!(cast.playable().len(), 2);
        assert_eq!(cast.npcs().len(), 2);
        assert_eq!(cast.npcs()[0].name().as_str(), "Alex");
        assert_eq!(cast.npcs()[1].name().as_str(), "Casey");
    }

    #[test]
    fn distinct_playable_namesakes_satisfy_the_minimum() {
        let first = player("Ajax");
        let second = PlayerCharacter::new(
            CharacterName::new("Ajax").unwrap(),
            CharacterDescription::new("the other Ajax").unwrap(),
            Backstory::new("a different history").unwrap(),
        );
        let cast = WorldCast::new([first.clone(), second.clone()], [], &Limits::default()).unwrap();
        assert_eq!(cast.playable(), [first, second]);
    }

    #[test]
    fn npcs_are_capped_at_the_configured_limit() {
        use crate::limits::MaxGeneratedNpcs;
        let limits = Limits {
            max_generated_npcs: MaxGeneratedNpcs::new(1),
            ..Limits::default()
        };
        let cast = WorldCast::new(
            [player("Alex"), player("Blair")],
            [npc("Casey"), npc("Devon")],
            &limits,
        )
        .unwrap();
        assert_eq!(cast.npcs().len(), 1);
        assert_eq!(cast.npcs()[0].name().as_str(), "Casey");
    }

    #[test]
    fn npc_limit_applies_to_surviving_entries_including_zero() {
        use crate::limits::MaxGeneratedNpcs;

        for maximum in [0, 1, 2, 3, 8, usize::MAX] {
            let limits = Limits {
                max_generated_npcs: MaxGeneratedNpcs::new(maximum),
                ..Limits::default()
            };
            let cast = WorldCast::new(
                [player("Alex"), player("Blair")],
                [npc("Casey"), npc("Casey"), npc("Devon")],
                &limits,
            )
            .unwrap();
            let names: Vec<_> = cast.npcs().iter().map(|npc| npc.name().as_str()).collect();
            assert_eq!(names, ["Casey", "Devon"][..maximum.min(2)]);
        }
    }

    #[test]
    fn zero_npc_limit_does_not_consume_candidates() {
        use crate::limits::MaxGeneratedNpcs;

        let limits = Limits {
            max_generated_npcs: MaxGeneratedNpcs::new(0),
            ..Limits::default()
        };
        let candidates = std::iter::from_fn(|| -> Option<NonPlayerCharacter> {
            panic!("a zero limit must not consume NPC candidates")
        });
        let cast = WorldCast::new([player("Alex"), player("Blair")], candidates, &limits).unwrap();
        assert!(cast.npcs().is_empty());
    }

    #[test]
    fn playable_index_is_only_constructible_in_range() {
        let limits = Limits::default();
        let cast = WorldCast::new([player("Alex"), player("Blair")], [], &limits).unwrap();
        assert!(cast.playable_index(1).is_some());
        assert!(cast.playable_index(2).is_none());
    }
}
