//! Calibre-shaped request/response DTOs and their mapping into `cyoa-core`
//! domain types, colocated per the `clocker` `TimeLogEntryDTO` pattern: each
//! wire struct's `From`/`TryFrom` conversion lives right next to it, rather
//! than in a separate parsing module. `cyoa-core` stays serde-free, so we
//! can't use `#[serde(into/from)]` on the domain type itself the way clocker
//! does; the mapping is a plain, explicit `impl` here instead.
//!
//! This is the Rust home of calibre's `validated_world`/
//! `validated_player_characters`/`validated_npcs`/`validated_turn`
//! (`ARCH-002`: "boundary DTOs map blank wire strings to `None`").
//! `engine.rs` (a later slice) only orchestrates: call the backend, feed the
//! streaming scanner, call these conversions, call `GameState::commit_turn`.

use std::borrow::Cow;

use cyoa_core::{
    character::CharacterDelta,
    ids::CharacterId,
    summary::{EventList, SummaryUpdate, UpcomingEventsUpdate},
    text::{
        Backstory, ChapterTitle, CharacterDescription, CharacterName, CharacterSituation,
        CurrentSituation, Narrative, QuickActionText, Relationships, SceneDescription,
        WorldDescription, WorldTitle,
    },
    turn::{ChapterMarker, NoQuickActions, QuickAction, QuickActionKind, QuickActions, StoryTurn},
    world::{NonPlayerCharacter, PlayerCharacter, WorldOutline},
};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer};
use thiserror::Error;

/// LLM call #1 request shape (calibre `WorldOutline`, cyoa.py:97-104).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(rename = "WorldOutline")]
pub struct WorldOutlineWire {
    pub title: String,
    pub world_description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidWorldOutline {
    #[error("the model returned a world with no title")]
    BlankTitle,
    #[error("the model returned a world with no description")]
    BlankDescription,
}

impl TryFrom<WorldOutlineWire> for WorldOutline {
    type Error = InvalidWorldOutline;

    fn try_from(wire: WorldOutlineWire) -> Result<Self, Self::Error> {
        let title = WorldTitle::new(&wire.title).map_err(|_| InvalidWorldOutline::BlankTitle)?;
        let description = WorldDescription::new(&wire.world_description)
            .map_err(|_| InvalidWorldOutline::BlankDescription)?;
        Ok(WorldOutline::new(title, description))
    }
}

/// LLM call #2 request shape (calibre `GeneratedCast`, cyoa.py:107-117).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(rename = "PlayerCharacter")]
pub struct PlayerCharacterWire {
    pub name: String,
    pub description: String,
    pub backstory: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(rename = "NonPlayerCharacter")]
pub struct NonPlayerCharacterWire {
    pub name: String,
    pub description: String,
    pub backstory: String,
    #[serde(default)]
    pub relationships: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(rename = "GeneratedCast")]
pub struct GeneratedCastWire {
    pub characters: Vec<PlayerCharacterWire>,
    pub npcs: Vec<NonPlayerCharacterWire>,
}

/// Drops any character missing a required field rather than failing the
/// whole cast (calibre `validated_player_characters`/`validated_npcs`,
/// cyoa.py:1167-1201). Deduplication/capping against `Limits` is
/// `WorldCast::new`'s job, already implemented; this only constructs valid
/// per-character domain values from raw wire text.
pub fn playable_and_npcs_from_wire(
    wire: GeneratedCastWire,
) -> (Vec<PlayerCharacter>, Vec<NonPlayerCharacter>) {
    let playable = wire
        .characters
        .into_iter()
        .filter_map(|c| {
            Some(PlayerCharacter::new(
                CharacterName::new(&c.name).ok()?,
                CharacterDescription::new(&c.description).ok()?,
                Backstory::new(&c.backstory).ok()?,
            ))
        })
        .collect();
    let npcs = wire
        .npcs
        .into_iter()
        .filter_map(|n| {
            Some(NonPlayerCharacter::new(
                CharacterName::new(&n.name).ok()?,
                CharacterDescription::new(&n.description).ok()?,
                Backstory::new(&n.backstory).ok()?,
                Relationships::new(&n.relationships).ok(),
            ))
        })
        .collect();
    (playable, npcs)
}

/// A change to one character of the story summary (calibre `CharacterDelta`,
/// cyoa.py:181-222). Every optional field defaults to `""`, meaning
/// unchanged; `id`/`current_state` are always sent by the schema but are not
/// specially validated beyond the general blank-to-`None` rule.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(rename = "CharacterDelta")]
pub struct CharacterDeltaWire {
    pub id: String,
    pub current_state: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub backstory: String,
    #[serde(default)]
    pub relationships: String,
}

impl From<CharacterDeltaWire> for CharacterDelta {
    fn from(wire: CharacterDeltaWire) -> Self {
        CharacterDelta {
            id: CharacterId::new(&wire.id).ok(),
            name: CharacterName::new(&wire.name).ok(),
            description: CharacterDescription::new(&wire.description).ok(),
            backstory: Backstory::new(&wire.backstory).ok(),
            relationships: Relationships::new(&wire.relationships).ok(),
            current_state: CharacterSituation::new(&wire.current_state).ok(),
        }
    }
}

/// The per-turn delta (calibre `SummaryUpdate`, cyoa.py:225-256).
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(rename = "SummaryUpdate")]
pub struct SummaryUpdateWire {
    pub current_situation: String,
    #[serde(default)]
    pub character_updates: Vec<CharacterDeltaWire>,
    #[serde(default)]
    pub new_major_events: Vec<String>,
    /// `None` = unchanged (the common case); `Some(vec![])` = the last open
    /// thread was just resolved. This is the one replace-not-merge field —
    /// see `UpcomingEventsUpdate` — and the mapping below must keep the
    /// null/empty-list distinction intact (`ARCH-002`).
    #[serde(default)]
    pub upcoming_events: Option<Vec<String>>,
    #[serde(default)]
    pub world: String,
    #[serde(default)]
    pub consolidated_major_events: Vec<String>,
}

impl From<SummaryUpdateWire> for SummaryUpdate {
    fn from(wire: SummaryUpdateWire) -> Self {
        SummaryUpdate {
            world: WorldDescription::new(&wire.world).ok(),
            current_situation: CurrentSituation::new(&wire.current_situation).ok(),
            character_updates: wire.character_updates.into_iter().map(Into::into).collect(),
            new_major_events: EventList::new(wire.new_major_events),
            consolidated_major_events: EventList::new(wire.consolidated_major_events),
            upcoming_events: match wire.upcoming_events {
                None => UpcomingEventsUpdate::Keep,
                Some(events) => UpcomingEventsUpdate::Replace(EventList::new(events)),
            },
        }
    }
}

/// The kind of approach a quick action takes (calibre `QuickActionKind`,
/// cyoa.py:264-276). Manual `Deserialize`: `#[serde(other)]` does not work on
/// a plain externally-tagged unit-variant enum (PLAN.md), and a hallucinated
/// value must become the harmless `Other`, never a deserialize error that
/// discards an entire turn's prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickActionKindWire {
    Cautious,
    Bold,
    Social,
    Investigate,
    Other,
}

impl QuickActionKindWire {
    pub fn to_domain(self) -> QuickActionKind {
        match self {
            Self::Cautious => QuickActionKind::Cautious,
            Self::Bold => QuickActionKind::Bold,
            Self::Social => QuickActionKind::Social,
            Self::Investigate => QuickActionKind::Investigate,
            Self::Other => QuickActionKind::Other,
        }
    }
}

impl<'de> Deserialize<'de> for QuickActionKindWire {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "cautious" => Self::Cautious,
            "bold" => Self::Bold,
            "social" => Self::Social,
            "investigate" => Self::Investigate,
            _ => Self::Other,
        })
    }
}

impl JsonSchema for QuickActionKindWire {
    fn schema_name() -> Cow<'static, str> {
        "QuickActionKind".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "enum": ["cautious", "bold", "social", "investigate", "other"]
        })
    }
}

fn default_quick_action_kind() -> QuickActionKindWire {
    QuickActionKindWire::Other
}

/// One suggested action (calibre `QuickAction`, cyoa.py:312-317).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(rename = "QuickAction")]
pub struct QuickActionWire {
    pub text: String,
    #[serde(default = "default_quick_action_kind")]
    pub kind: QuickActionKindWire,
}

impl QuickActionWire {
    /// Blank text is filtered out here, not an error — `selected_quick_actions`
    /// strips blanks before deduplicating (cyoa.py:1289-1292); only "zero
    /// survived" is a hard failure, and that lives in `QuickActions::select`.
    fn into_domain(self) -> Option<QuickAction> {
        let text = QuickActionText::new(&self.text).ok()?;
        Some(QuickAction::new(text, self.kind.to_domain()))
    }
}

/// LLM call #3 request shape, the per-turn streamed schema (calibre
/// `StoryTurn`, cyoa.py:320-345). Keeps calibre's separate
/// `starts_new_chapter`/`chapter_title` fields — the domain's single
/// `ChapterMarker` is a response-mapping concern, handled below.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(rename = "StoryTurn")]
pub struct StoryTurnWire {
    pub narrative: String,
    pub quick_actions: Vec<QuickActionWire>,
    #[serde(default)]
    pub scene_description: String,
    pub summary_update: SummaryUpdateWire,
    pub starts_new_chapter: bool,
    pub chapter_title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidStoryTurn {
    #[error("the model returned an empty passage of prose")]
    BlankNarrative,
    #[error("the model returned no usable quick actions")]
    NoQuickActions,
}

impl From<NoQuickActions> for InvalidStoryTurn {
    fn from(_: NoQuickActions) -> Self {
        Self::NoQuickActions
    }
}

impl TryFrom<StoryTurnWire> for StoryTurn {
    type Error = InvalidStoryTurn;

    fn try_from(wire: StoryTurnWire) -> Result<Self, Self::Error> {
        let narrative =
            Narrative::new(&wire.narrative).map_err(|_| InvalidStoryTurn::BlankNarrative)?;
        let quick_actions = QuickActions::select(
            wire.quick_actions
                .into_iter()
                .filter_map(QuickActionWire::into_domain),
        )?;
        let scene_description = SceneDescription::new(&wire.scene_description).ok();
        let summary_update = SummaryUpdate::from(wire.summary_update);
        // A blank/absent title is treated as "no title supplied" whether or
        // not this turn opens a new chapter — deliberately extending calibre
        // (see CHAPTER-001): a `Continue` turn can retitle its chapter too.
        let title = wire
            .chapter_title
            .as_deref()
            .and_then(|t| ChapterTitle::new(t).ok());
        let chapter = if wire.starts_new_chapter {
            ChapterMarker::NewChapter { title }
        } else {
            ChapterMarker::Continue { title }
        };
        Ok(StoryTurn::new(
            narrative,
            quick_actions,
            scene_description,
            summary_update,
            chapter,
        ))
    }
}
