//! Characters and the nonempty, uniquely identified cast of a story summary.
//! Merge behavior follows calibre's `updated_characters` (cyoa.py:1318).

use std::collections::{HashMap, HashSet};

use indexmap::{IndexMap, IndexSet};
use thiserror::Error;

use crate::{
    ids::CharacterId,
    text::{
        Backstory, CharacterDescription, CharacterName, CharacterSituation, Relationships,
        matching_key,
    },
};

/// Four `Option` fields of similar-looking value objects gives positional
/// construction no readable equivalent of a named argument, so callers build
/// via this field-labelled struct instead of four bare positional `Option`s.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct CharacterDetailsFields {
    pub description: Option<CharacterDescription>,
    pub backstory: Option<Backstory>,
    pub relationships: Option<Relationships>,
    pub current_state: Option<CharacterSituation>,
}

/// Valid stored details: at least one of description/backstory is present.
/// All present values are nonblank; missing relationships/state are legitimate.
///
/// ```compile_fail
/// use cyoa_core::character::CharacterDetails;
/// let details = CharacterDetails::default(); // No invalid empty default.
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CharacterDetails {
    description: Option<CharacterDescription>,
    backstory: Option<Backstory>,
    relationships: Option<Relationships>,
    current_state: Option<CharacterSituation>,
}

impl CharacterDetails {
    pub fn new(fields: CharacterDetailsFields) -> Result<Self, MissingCharacterDetails> {
        if fields.description.is_none() && fields.backstory.is_none() {
            return Err(MissingCharacterDetails);
        }
        Ok(Self {
            description: fields.description,
            backstory: fields.backstory,
            relationships: fields.relationships,
            current_state: fields.current_state,
        })
    }

    pub fn description(&self) -> Option<&CharacterDescription> {
        self.description.as_ref()
    }
    pub fn backstory(&self) -> Option<&Backstory> {
        self.backstory.as_ref()
    }
    pub fn relationships(&self) -> Option<&Relationships> {
        self.relationships.as_ref()
    }
    pub fn current_state(&self) -> Option<&CharacterSituation> {
        self.current_state.as_ref()
    }

    fn updated(&self, update: &CharacterDelta) -> Self {
        // A delta cannot clear a value, so at least one durable detail survives.
        Self {
            description: update
                .description
                .as_ref()
                .or(self.description.as_ref())
                .cloned(),
            backstory: update
                .backstory
                .as_ref()
                .or(self.backstory.as_ref())
                .cloned(),
            relationships: update
                .relationships
                .as_ref()
                .or(self.relationships.as_ref())
                .cloned(),
            current_state: update
                .current_state
                .as_ref()
                .or(self.current_state.as_ref())
                .cloned(),
        }
    }
}

/// A stored character has a name, identity, and at least one durable detail.
///
/// ```compile_fail
/// use cyoa_core::{character::Character, text::CharacterName};
/// fn rename(character: &mut Character, name: CharacterName) {
///     character.name = name;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Character {
    id: CharacterId,
    name: CharacterName,
    details: CharacterDetails,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a character needs a description or backstory")]
pub struct MissingCharacterDetails;

impl Character {
    pub fn new(id: CharacterId, name: CharacterName, details: CharacterDetails) -> Self {
        Self { id, name, details }
    }

    pub fn id(&self) -> &CharacterId {
        &self.id
    }
    pub fn name(&self) -> &CharacterName {
        &self.name
    }
    pub fn details(&self) -> &CharacterDetails {
        &self.details
    }
}

/// A proposed change, not a stored character. For every field, `None` leaves it
/// unchanged and `Some(nonblank value)` supplies a replacement (or lookup id).
/// Clearing is not an operation this type can express. Boundary adapters map all
/// blank wire strings to `None`; unmatched incomplete introductions are ignored.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CharacterDelta {
    pub id: Option<CharacterId>,
    pub name: Option<CharacterName>,
    pub description: Option<CharacterDescription>,
    pub backstory: Option<Backstory>,
    pub relationships: Option<Relationships>,
    pub current_state: Option<CharacterSituation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvalidCast {
    #[error("a story summary must contain at least one character")]
    Empty,
}

/// A summary cast. Exact records collapse before colliding ids receive suffixes.
/// Equality for deduplication includes id, name, and every detail; names may coincide.
#[derive(Debug, Clone)]
pub struct CharacterCast {
    characters: IndexMap<CharacterId, Character>,
}

impl PartialEq for CharacterCast {
    fn eq(&self, other: &Self) -> bool {
        // IndexMap equality ignores order; cast order is part of story state.
        self.characters.iter().eq(other.characters.iter())
    }
}

impl Eq for CharacterCast {}

impl CharacterCast {
    pub fn new(characters: impl IntoIterator<Item = Character>) -> Result<Self, InvalidCast> {
        let characters: IndexSet<_> = characters.into_iter().collect();
        let mut reserved_ids: HashSet<_> = characters.iter().map(|c| c.id.clone()).collect();
        let mut by_id = IndexMap::new();
        for mut character in characters {
            if by_id.contains_key(&character.id) {
                // Reserve even later input ids so a repair cannot steal one.
                character.id = character.id.unique(|id| reserved_ids.contains(id));
                reserved_ids.insert(character.id.clone());
            }
            by_id.insert(character.id.clone(), character);
        }
        if by_id.is_empty() {
            return Err(InvalidCast::Empty);
        }
        Ok(Self { characters: by_id })
    }

    pub fn get(&self, id: &CharacterId) -> Option<&Character> {
        self.characters.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Character> + DoubleEndedIterator {
        self.characters.values()
    }

    pub fn len(&self) -> usize {
        self.characters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }

    #[must_use = "merging returns a new cast; use the result to commit the update"]
    pub fn updated(&self, updates: &[CharacterDelta]) -> Self {
        let mut characters = self.characters.clone();
        // Collection deliberately makes the last duplicate name win, as in Python.
        let mut by_name: HashMap<_, _> = characters
            .values()
            .map(|c| (matching_key(c.name.as_str()), c.id.clone()))
            .collect();
        let mut updated = HashSet::new();
        for update in updates {
            let id = update
                .id
                .as_ref()
                .filter(|id| characters.contains_key(*id))
                .or_else(|| {
                    update
                        .name
                        .as_ref()
                        .and_then(|name| by_name.get(&matching_key(name.as_str())))
                })
                .cloned();
            if let Some(id) = id {
                if !updated.insert(id.clone()) {
                    continue;
                }
                let character = characters
                    .get_mut(&id)
                    .expect("resolved id exists in the cast");
                if let Some(name) = &update.name {
                    character.name = name.clone();
                    // Keep the old alias too until this turn ends, matching calibre.
                    by_name.insert(matching_key(name.as_str()), id);
                }
                character.details = character.details.updated(update);
                continue;
            }
            let Some(name) = &update.name else {
                continue;
            };
            let id = update
                .id
                .clone()
                .unwrap_or_else(|| CharacterId::for_name(name.as_str()))
                .unique(|id| characters.contains_key(id));
            let Ok(details) = CharacterDetails::new(CharacterDetailsFields {
                description: update.description.clone(),
                backstory: update.backstory.clone(),
                relationships: update.relationships.clone(),
                current_state: update.current_state.clone(),
            }) else {
                continue;
            };
            let character = Character::new(id.clone(), name.clone(), details);
            by_name.insert(matching_key(name.as_str()), id.clone());
            updated.insert(id.clone());
            characters.insert(id, character);
        }
        // The original nonempty cast survives; additions have unique ids and all
        // changes preserve names and durable details, so no revalidation is needed.
        Self { characters }
    }
}
