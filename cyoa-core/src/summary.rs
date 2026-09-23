//! Bounded story memory and its invariant-preserving delta merge.
//! Port of calibre's `clean_text_list` and `updated_summary` (cyoa.py:1306,1374).

use std::collections::HashSet;

use crate::{
    character::{CharacterCast, CharacterDelta},
    limits::MajorEventLimit,
    text::{CurrentSituation, EventText, WorldDescription, matching_key},
};

/// Ordered, trimmed, nonblank events, deduplicated without changing first spelling.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EventList {
    events: Vec<EventText>,
}

impl EventList {
    pub fn new(items: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let mut seen = HashSet::new();
        Self {
            events: items
                .into_iter()
                .filter_map(|item| EventText::new(item).ok())
                .filter(|event| seen.insert(matching_key(event.as_str())))
                .collect(),
        }
    }

    pub fn as_slice(&self) -> &[EventText] {
        &self.events
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, EventText> {
        self.events.iter()
    }

    fn retain_latest(&mut self, limit: MajorEventLimit) {
        let discard = self.events.len().saturating_sub(limit.get());
        self.events.drain(..discard);
    }
}

/// Carries its bound so subsequent merges cannot accidentally use a different cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MajorEvents {
    events: EventList,
    limit: MajorEventLimit,
}

impl MajorEvents {
    pub fn new(mut events: EventList, limit: MajorEventLimit) -> Self {
        events.retain_latest(limit);
        Self { events, limit }
    }

    pub fn events(&self) -> &EventList {
        &self.events
    }
    pub fn limit(&self) -> MajorEventLimit {
        self.limit
    }

    fn updated(&self, consolidated: &EventList, new: &EventList) -> Self {
        let base = if consolidated.is_empty() {
            &self.events
        } else {
            consolidated
        };
        Self::new(EventList::new(base.iter().chain(new.iter())), self.limit)
    }
}

/// Domain expression of wire `null`/omitted versus a full replacement list.
/// `Replace(EventList::default())` clears the last outstanding thread.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum UpcomingEventsUpdate {
    #[default]
    Keep,
    Replace(EventList),
}

/// Normalized proposed changes. Adapters map blank world/situation fields to
/// `None`; raw wire DTOs retain their original string and nullable-list shape.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SummaryUpdate {
    pub world: Option<WorldDescription>,
    pub current_situation: Option<CurrentSituation>,
    pub character_updates: Vec<CharacterDelta>,
    pub new_major_events: EventList,
    pub consolidated_major_events: EventList,
    pub upcoming_events: UpcomingEventsUpdate,
}

/// Valid story memory. No unchecked default, deserialization, or mutable fields.
///
/// ```compile_fail
/// use cyoa_core::{summary::{StorySummary, MajorEvents, EventList}, character::CharacterCast, text::{CharacterName, CurrentSituation}};
/// fn wrong_world(name: CharacterName, situation: CurrentSituation, cast: CharacterCast, events: MajorEvents) {
///     StorySummary::new(name, situation, cast, events, EventList::default());
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorySummary {
    world: WorldDescription,
    current_situation: CurrentSituation,
    characters: CharacterCast,
    major_events: MajorEvents,
    upcoming_events: EventList,
}

impl StorySummary {
    pub fn new(
        world: WorldDescription,
        current_situation: CurrentSituation,
        characters: CharacterCast,
        major_events: MajorEvents,
        upcoming_events: EventList,
    ) -> Self {
        Self {
            world,
            current_situation,
            characters,
            major_events,
            upcoming_events,
        }
    }

    pub fn world(&self) -> &WorldDescription {
        &self.world
    }
    pub fn current_situation(&self) -> &CurrentSituation {
        &self.current_situation
    }
    pub fn characters(&self) -> &CharacterCast {
        &self.characters
    }
    pub fn major_events(&self) -> &MajorEvents {
        &self.major_events
    }
    pub fn upcoming_events(&self) -> &EventList {
        &self.upcoming_events
    }

    /// Infallible after construction: no delta can erase required summary fields
    /// or remove the existing cast. The caller decides whether to commit the result.
    #[must_use = "merging returns a new summary; use the result to commit the update"]
    pub fn updated(&self, update: &SummaryUpdate) -> Self {
        let world = update.world.as_ref().unwrap_or(&self.world).clone();
        let current_situation = update
            .current_situation
            .as_ref()
            .unwrap_or(&self.current_situation)
            .clone();
        Self {
            world,
            current_situation,
            characters: self.characters.updated(&update.character_updates),
            major_events: self
                .major_events
                .updated(&update.consolidated_major_events, &update.new_major_events),
            upcoming_events: match &update.upcoming_events {
                UpcomingEventsUpdate::Keep => self.upcoming_events.clone(),
                UpcomingEventsUpdate::Replace(events) => events.clone(),
            },
        }
    }
}
