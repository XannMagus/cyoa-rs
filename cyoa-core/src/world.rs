//! The generated game world: an outline plus a validated cast.
//! Ports calibre's `validated_world`/`validated_player_characters`/
//! `validated_npcs`/`validated_cast` (cyoa.py:1167-1222).

use std::collections::HashSet;

use crate::{
    limits::Limits,
    text::{
        Backstory, CharacterDescription, CharacterName, Relationships, WorldDescription,
        WorldTitle, matching_key,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Deduplicates playables and NPCs by casefolded name (first occurrence
    /// wins, matching calibre), drops NPCs whose name collides with a
    /// playable, and caps NPCs at `limits.max_generated_npcs`.
    pub fn new(
        playable: impl IntoIterator<Item = PlayerCharacter>,
        npcs: impl IntoIterator<Item = NonPlayerCharacter>,
        limits: &Limits,
    ) -> Result<Self, TooFewPlayableCharacters> {
        let mut seen = HashSet::new();
        let playable: Vec<_> = playable
            .into_iter()
            .filter(|c| seen.insert(matching_key(c.name.as_str())))
            .collect();
        let minimum = limits.min_playable_characters.get();
        if playable.len() < minimum {
            return Err(TooFewPlayableCharacters {
                minimum,
                found: playable.len(),
            });
        }
        let mut names: HashSet<String> = playable
            .iter()
            .map(|c| matching_key(c.name.as_str()))
            .collect();
        let mut selected_npcs = Vec::new();
        for npc in npcs {
            let key = matching_key(npc.name.as_str());
            if !names.insert(key) {
                continue;
            }
            selected_npcs.push(npc);
            if selected_npcs.len() >= limits.max_generated_npcs.get() {
                break;
            }
        }
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
    fn duplicate_playable_names_collapse_and_npc_colliding_with_playable_is_dropped() {
        let limits = Limits::default();
        let cast = WorldCast::new(
            [player("Alex"), player("Blair"), player("alex")],
            [npc("Alex"), npc("Casey")],
            &limits,
        )
        .unwrap();
        assert_eq!(cast.playable().len(), 2);
        assert_eq!(cast.npcs().len(), 1);
        assert_eq!(cast.npcs()[0].name().as_str(), "Casey");
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
    fn playable_index_is_only_constructible_in_range() {
        let limits = Limits::default();
        let cast = WorldCast::new([player("Alex"), player("Blair")], [], &limits).unwrap();
        assert!(cast.playable_index(1).is_some());
        assert!(cast.playable_index(2).is_none());
    }
}
