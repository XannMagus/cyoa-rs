//! Private save DTOs: normalization may not silently change stored history.
use super::codec::SaveCodecError;
use cyoa_application::persistence::*;
use cyoa_core::{
    character::{
        Character, CharacterCast, CharacterDelta, CharacterDetails, CharacterDetailsFields,
    },
    game::GameState,
    ids::CharacterId,
    limits::*,
    style::StoryStyle,
    summary::*,
    text::*,
    turn::*,
    world::*,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

type Result<T> = std::result::Result<T, SaveCodecError>;
fn text<'a, T, E: std::fmt::Display>(
    s: &'a str,
    path: &str,
    make: impl FnOnce(&'a str) -> std::result::Result<T, E>,
) -> Result<T> {
    if s.trim() != s {
        return Err(SaveCodecError::invalid(
            path,
            "stored business text is not normalized",
        ));
    }
    make(s).map_err(|e| SaveCodecError::invalid(path, e.to_string()))
}
fn optional<'a, T, E: std::fmt::Display>(
    s: &'a str,
    path: &str,
    make: impl FnOnce(&'a str) -> std::result::Result<T, E>,
) -> Result<Option<T>> {
    if s.is_empty() {
        Ok(None)
    } else {
        text(s, path, make).map(Some)
    }
}
fn events(items: Vec<String>, path: &str) -> Result<EventList> {
    let list = EventList::new(&items);
    if !list
        .iter()
        .map(|x| x.as_str())
        .eq(items.iter().map(String::as_str))
    {
        return Err(SaveCodecError::invalid(
            path,
            "stored events would be trimmed, dropped or deduplicated",
        ));
    }
    Ok(list)
}
fn strings(list: &EventList) -> Vec<String> {
    list.iter().map(|x| x.as_str().into()).collect()
}
fn sentinel<T: AsRef<str>>(value: Option<&T>) -> String {
    value.map_or("", AsRef::as_ref).into()
}
fn count(value: u64, path: &str) -> Result<usize> {
    usize::try_from(value)
        .map_err(|_| SaveCodecError::invalid(path, "integer exceeds this platform's range"))
}
fn required_nullable<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::deserialize(d)
}

#[derive(Serialize, Deserialize)]
pub(super) struct SaveEnvelopeV1 {
    pub version: u32,
    pub id: String,
    pub revision: u64,
    pub title: String,
    pub saved_at: String,
    pub turn_count: u64,
    pub source: SourceSave,
    pub game: GameSaveV1,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(super) enum SourceSave {
    Live,
    Demo { scenario: String },
}
impl From<StorySource> for SourceSave {
    fn from(value: StorySource) -> Self {
        match value {
            StorySource::Live => Self::Live,
            StorySource::Demo {
                scenario: DemoScenarioId::HarbourV1,
            } => Self::Demo {
                scenario: "harbour-v1".into(),
            },
        }
    }
}
impl SourceSave {
    // Serde's internally tagged enum buffers its fields before deserializing a
    // variant, so serde_ignored cannot see discarded source extensions.
    pub fn unrecognized_fields(value: &serde_json::Value) -> Vec<String> {
        let Some(source) = value.get("source").and_then(serde_json::Value::as_object) else {
            return vec![];
        };
        let demo = source.get("kind").and_then(serde_json::Value::as_str) == Some("demo");
        source
            .keys()
            .filter(|key| key.as_str() != "kind" && !(demo && key.as_str() == "scenario"))
            .map(|key| format!("source.{key}"))
            .collect()
    }
    pub fn into_domain(self) -> Result<StorySource> {
        match self {
            Self::Live => Ok(StorySource::Live),
            Self::Demo { scenario } if scenario == "harbour-v1" => Ok(StorySource::Demo {
                scenario: DemoScenarioId::HarbourV1,
            }),
            Self::Demo { .. } => Err(SaveCodecError::invalid(
                "source.scenario",
                "unsupported demo scenario",
            )),
        }
    }
}
#[derive(Serialize, Deserialize)]
pub(super) struct LimitsSave {
    max_major_events: u64,
    max_generated_npcs: u64,
    prose_bridge_turns: u64,
    min_playable_characters: u64,
}
impl From<Limits> for LimitsSave {
    fn from(v: Limits) -> Self {
        Self {
            max_major_events: v.max_major_events.get() as u64,
            max_generated_npcs: v.max_generated_npcs.get() as u64,
            prose_bridge_turns: v.prose_bridge_turns.get() as u64,
            min_playable_characters: v.min_playable_characters.get() as u64,
        }
    }
}
impl LimitsSave {
    fn into_domain(self, path: &str) -> Result<Limits> {
        let major = format!("{path}.max_major_events");
        let minimum = format!("{path}.min_playable_characters");
        Ok(Limits {
            max_major_events: MajorEventLimit::new(count(self.max_major_events, &major)?)
                .map_err(|e| SaveCodecError::invalid(&major, e.to_string()))?,
            max_generated_npcs: MaxGeneratedNpcs::new(count(
                self.max_generated_npcs,
                &format!("{path}.max_generated_npcs"),
            )?),
            prose_bridge_turns: ProseBridgeTurns::new(count(
                self.prose_bridge_turns,
                &format!("{path}.prose_bridge_turns"),
            )?),
            min_playable_characters: MinPlayableCharacters::new(count(
                self.min_playable_characters,
                &minimum,
            )?)
            .map_err(|e| SaveCodecError::invalid(&minimum, e.to_string()))?,
        })
    }
}
#[derive(Serialize, Deserialize)]
pub(super) struct GameSaveV1 {
    brief: String,
    world: WorldSave,
    character_index: u64,
    original_limits: LimitsSave,
    active_limits: LimitsSave,
    #[serde(default)]
    turns: Vec<TurnRecordSave>,
    #[serde(default)]
    art_style: String,
    #[serde(default)]
    pace: String,
    #[serde(default)]
    tone: String,
    #[serde(default)]
    narration: String,
}
impl From<&GameState> for GameSaveV1 {
    fn from(v: &GameState) -> Self {
        Self {
            brief: v.brief().as_str().into(),
            world: v.world().into(),
            character_index: v.selected_world().position().get() as u64,
            original_limits: v.original_limits().into(),
            active_limits: v.limits().into(),
            turns: v.turns().iter().map(Into::into).collect(),
            art_style: sentinel(v.style().art_style.as_ref()),
            pace: sentinel(v.style().pace.as_ref()),
            tone: sentinel(v.style().tone.as_ref()),
            narration: sentinel(v.style().narration.as_ref()),
        }
    }
}
impl GameSaveV1 {
    pub fn into_domain(self) -> Result<GameState> {
        let original = self.original_limits.into_domain("game.original_limits")?;
        let active = self.active_limits.into_domain("game.active_limits")?;
        let selected = self
            .world
            .into_domain()?
            .select(PlayablePosition::new(count(
                self.character_index,
                "game.character_index",
            )?))
            .map_err(|e| SaveCodecError::invalid("game.character_index", e.to_string()))?;
        let style = StoryStyle {
            art_style: optional(&self.art_style, "game.art_style", ArtStyleKey::new)?,
            pace: optional(&self.pace, "game.pace", PaceKey::new)?,
            tone: optional(&self.tone, "game.tone", ToneKey::new)?,
            narration: optional(&self.narration, "game.narration", NarrationKey::new)?,
        };
        let turns = self
            .turns
            .into_iter()
            .enumerate()
            .map(|(i, t)| t.into_domain(&format!("game.turns[{i}]"), active))
            .collect::<Result<Vec<_>>>()?;
        Ok(GameState::restore(
            text(&self.brief, "game.brief", Brief::new)?,
            selected,
            style,
            original,
            RestoreLimits::Current(active),
            turns,
        ))
    }
}
#[derive(Serialize, Deserialize)]
struct WorldSave {
    title: String,
    world_description: String,
    #[serde(default)]
    characters: Vec<PlayerSave>,
    #[serde(default)]
    npcs: Vec<NpcSave>,
}
#[derive(Serialize, Deserialize)]
struct PlayerSave {
    name: String,
    description: String,
    backstory: String,
}
#[derive(Serialize, Deserialize)]
struct NpcSave {
    name: String,
    description: String,
    backstory: String,
    relationships: String,
}
impl From<&World> for WorldSave {
    fn from(v: &World) -> Self {
        Self {
            title: v.outline().title().as_str().into(),
            world_description: v.outline().description().as_str().into(),
            characters: v
                .cast()
                .playable()
                .iter()
                .map(|c| PlayerSave {
                    name: c.name().as_str().into(),
                    description: c.description().as_str().into(),
                    backstory: c.backstory().as_str().into(),
                })
                .collect(),
            npcs: v
                .cast()
                .npcs()
                .iter()
                .map(|c| NpcSave {
                    name: c.name().as_str().into(),
                    description: c.description().as_str().into(),
                    backstory: c.backstory().as_str().into(),
                    relationships: sentinel(c.relationships()),
                })
                .collect(),
        }
    }
}
impl WorldSave {
    fn into_domain(self) -> Result<World> {
        let playable = self
            .characters
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let p = format!("game.world.characters[{i}]");
                Ok(PlayerCharacter::new(
                    text(&c.name, &format!("{p}.name"), CharacterName::new)?,
                    text(
                        &c.description,
                        &format!("{p}.description"),
                        CharacterDescription::new,
                    )?,
                    text(&c.backstory, &format!("{p}.backstory"), Backstory::new)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let npcs = self
            .npcs
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let p = format!("game.world.npcs[{i}]");
                Ok(NonPlayerCharacter::new(
                    text(&c.name, &format!("{p}.name"), CharacterName::new)?,
                    text(
                        &c.description,
                        &format!("{p}.description"),
                        CharacterDescription::new,
                    )?,
                    text(&c.backstory, &format!("{p}.backstory"), Backstory::new)?,
                    optional(
                        &c.relationships,
                        &format!("{p}.relationships"),
                        Relationships::new,
                    )?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let cast = WorldCast::restore(playable, npcs)
            .map_err(|e| SaveCodecError::invalid("game.world", e.to_string()))?;
        Ok(World::new(
            WorldOutline::new(
                text(&self.title, "game.world.title", WorldTitle::new)?,
                text(
                    &self.world_description,
                    "game.world.world_description",
                    WorldDescription::new,
                )?,
            ),
            cast,
        ))
    }
}
#[derive(Serialize, Deserialize)]
struct TurnRecordSave {
    player_input: String,
    raw_response: String,
    turn: TurnSave,
    summary: SummarySave,
    #[serde(default)]
    provenance: ProvenanceSave,
    #[serde(default)]
    prompt_trace: Option<PromptTraceSave>,
}
#[derive(Serialize, Deserialize)]
struct TurnSave {
    narrative: String,
    quick_actions: Vec<ActionSave>,
    scene_description: String,
    summary_update: UpdateSave,
    starts_new_chapter: bool,
    #[serde(deserialize_with = "required_nullable")]
    chapter_title: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct ActionSave {
    text: String,
    #[serde(default = "other")]
    kind: String,
}
fn other() -> String {
    "other".into()
}
fn kind(v: &str) -> QuickActionKind {
    match v {
        "cautious" => QuickActionKind::Cautious,
        "bold" => QuickActionKind::Bold,
        "social" => QuickActionKind::Social,
        "investigate" => QuickActionKind::Investigate,
        _ => QuickActionKind::Other,
    }
}
fn kind_string(v: QuickActionKind) -> String {
    match v {
        QuickActionKind::Cautious => "cautious",
        QuickActionKind::Bold => "bold",
        QuickActionKind::Social => "social",
        QuickActionKind::Investigate => "investigate",
        QuickActionKind::Other => "other",
    }
    .into()
}
#[derive(Serialize, Deserialize)]
struct SummarySave {
    world: String,
    major_events: Vec<String>,
    characters: Vec<CharacterSave>,
    current_situation: String,
    upcoming_events: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct CharacterSave {
    name: String,
    description: String,
    backstory: String,
    relationships: String,
    #[serde(default)]
    current_state: String,
    #[serde(default)]
    id: String,
}
#[derive(Serialize, Deserialize)]
struct UpdateSave {
    current_situation: String,
    character_updates: Vec<DeltaSave>,
    new_major_events: Vec<String>,
    #[serde(default)]
    upcoming_events: Option<Vec<String>>,
    #[serde(default)]
    world: String,
    #[serde(default)]
    consolidated_major_events: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct DeltaSave {
    id: String,
    current_state: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    backstory: String,
    #[serde(default)]
    relationships: String,
}
#[derive(Default, Serialize, Deserialize)]
struct ProvenanceSave {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    list_price_estimate: Option<CostSave>,
}
#[derive(Serialize, Deserialize)]
struct CostSave {
    amount: f64,
    currency: String,
}
#[derive(Serialize, Deserialize)]
struct PromptTraceSave {
    instructions: String,
    prompt: String,
}

impl From<&TurnRecord> for TurnRecordSave {
    fn from(v: &TurnRecord) -> Self {
        Self {
            player_input: sentinel(v.input()),
            raw_response: v.raw_response().as_str().into(),
            turn: v.turn().into(),
            summary: v.summary().into(),
            provenance: v.provenance().into(),
            prompt_trace: v.prompt_trace().map(|p| PromptTraceSave {
                instructions: p.instructions.as_str().into(),
                prompt: p.prompt.as_str().into(),
            }),
        }
    }
}
impl TurnRecordSave {
    fn into_domain(self, p: &str, limits: Limits) -> Result<TurnRecord> {
        let turn = self.turn.into_domain(&format!("{p}.turn"))?;
        let summary = self.summary.into_domain(&format!("{p}.summary"), limits)?;
        let record = TurnGenerationRecord {
            input: optional(
                &self.player_input,
                &format!("{p}.player_input"),
                PlayerInput::new,
            )?,
            raw_response: RawResponse::new(self.raw_response),
            provenance: self.provenance.into_domain(&format!("{p}.provenance"))?,
            prompt_trace: self.prompt_trace.map(|t| PromptTrace {
                instructions: Instructions::new(t.instructions),
                prompt: RenderedPrompt::new(t.prompt),
            }),
        };
        Ok(TurnRecord::new(turn, summary, record))
    }
}
impl From<&StoryTurn> for TurnSave {
    fn from(v: &StoryTurn) -> Self {
        let (starts, title) = match v.chapter() {
            ChapterMarker::Continue { title } => (false, title),
            ChapterMarker::NewChapter { title } => (true, title),
        };
        Self {
            narrative: v.narrative().as_str().into(),
            quick_actions: v
                .quick_actions()
                .as_slice()
                .iter()
                .map(|a| ActionSave {
                    text: a.text().as_str().into(),
                    kind: kind_string(a.kind()),
                })
                .collect(),
            scene_description: sentinel(v.scene_description()),
            summary_update: v.summary_update().into(),
            starts_new_chapter: starts,
            chapter_title: title.as_ref().map(|t| t.as_str().into()),
        }
    }
}
impl TurnSave {
    fn into_domain(self, p: &str) -> Result<StoryTurn> {
        let ap = format!("{p}.quick_actions");
        if !(1..=QUICK_ACTION_COUNT).contains(&self.quick_actions.len()) {
            return Err(SaveCodecError::invalid(
                &ap,
                "stored turn needs 1–3 quick actions",
            ));
        }
        let actions = self
            .quick_actions
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                Ok(QuickAction::new(
                    text(&a.text, &format!("{ap}[{i}].text"), QuickActionText::new)?,
                    kind(&a.kind),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let selected = QuickActions::select(actions.clone())
            .map_err(|e| SaveCodecError::invalid(&ap, e.to_string()))?;
        if selected.as_slice() != actions {
            return Err(SaveCodecError::invalid(
                &ap,
                "stored quick actions would be deduplicated or reordered",
            ));
        }
        let title = self
            .chapter_title
            .as_ref()
            .map(|s| text(s, &format!("{p}.chapter_title"), ChapterTitle::new))
            .transpose()?;
        let chapter = if self.starts_new_chapter {
            ChapterMarker::NewChapter { title }
        } else {
            ChapterMarker::Continue { title }
        };
        Ok(StoryTurn::new(
            text(&self.narrative, &format!("{p}.narrative"), Narrative::new)?,
            selected,
            optional(
                &self.scene_description,
                &format!("{p}.scene_description"),
                SceneDescription::new,
            )?,
            self.summary_update
                .into_domain(&format!("{p}.summary_update"))?,
            chapter,
        ))
    }
}
impl From<&StorySummary> for SummarySave {
    fn from(v: &StorySummary) -> Self {
        Self {
            world: v.world().as_str().into(),
            major_events: strings(v.major_events().events()),
            characters: v.characters().iter().map(Into::into).collect(),
            current_situation: v.current_situation().as_str().into(),
            upcoming_events: strings(v.upcoming_events()),
        }
    }
}
impl SummarySave {
    fn into_domain(self, p: &str, limits: Limits) -> Result<StorySummary> {
        if self.major_events.len() > limits.max_major_events.get() {
            return Err(SaveCodecError::invalid(
                &format!("{p}.major_events"),
                "stored events exceed saved active limit",
            ));
        }
        let major = events(self.major_events, &format!("{p}.major_events"))?;
        let characters = self
            .characters
            .into_iter()
            .enumerate()
            .map(|(i, c)| c.into_domain(&format!("{p}.characters[{i}]")))
            .collect::<Result<Vec<_>>>()?;
        let mut ids = HashSet::new();
        if characters.iter().any(|c| !ids.insert(c.id())) {
            return Err(SaveCodecError::invalid(
                &format!("{p}.characters"),
                "duplicate stored character ID",
            ));
        }
        let cast = CharacterCast::new(characters.clone())
            .map_err(|e| SaveCodecError::invalid(&format!("{p}.characters"), e.to_string()))?;
        if !cast.iter().eq(characters.iter()) {
            return Err(SaveCodecError::invalid(
                &format!("{p}.characters"),
                "stored cast would lose records or change identity",
            ));
        }
        Ok(StorySummary::new(
            text(&self.world, &format!("{p}.world"), WorldDescription::new)?,
            text(
                &self.current_situation,
                &format!("{p}.current_situation"),
                CurrentSituation::new,
            )?,
            cast,
            MajorEvents::new(major, limits.max_major_events),
            events(self.upcoming_events, &format!("{p}.upcoming_events"))?,
        ))
    }
}
impl From<&Character> for CharacterSave {
    fn from(c: &Character) -> Self {
        Self {
            id: c.id().as_str().into(),
            name: c.name().as_str().into(),
            description: sentinel(c.details().description()),
            backstory: sentinel(c.details().backstory()),
            relationships: sentinel(c.details().relationships()),
            current_state: sentinel(c.details().current_state()),
        }
    }
}
impl CharacterSave {
    fn into_domain(self, p: &str) -> Result<Character> {
        let details = CharacterDetails::new(CharacterDetailsFields {
            description: optional(
                &self.description,
                &format!("{p}.description"),
                CharacterDescription::new,
            )?,
            backstory: optional(&self.backstory, &format!("{p}.backstory"), Backstory::new)?,
            relationships: optional(
                &self.relationships,
                &format!("{p}.relationships"),
                Relationships::new,
            )?,
            current_state: optional(
                &self.current_state,
                &format!("{p}.current_state"),
                CharacterSituation::new,
            )?,
        })
        .map_err(|e| SaveCodecError::invalid(p, e.to_string()))?;
        Ok(Character::new(
            text(&self.id, &format!("{p}.id"), CharacterId::new)?,
            text(&self.name, &format!("{p}.name"), CharacterName::new)?,
            details,
        ))
    }
}
impl From<&SummaryUpdate> for UpdateSave {
    fn from(v: &SummaryUpdate) -> Self {
        Self {
            world: sentinel(v.world.as_ref()),
            current_situation: sentinel(v.current_situation.as_ref()),
            character_updates: v.character_updates.iter().map(Into::into).collect(),
            new_major_events: strings(&v.new_major_events),
            consolidated_major_events: strings(&v.consolidated_major_events),
            upcoming_events: match &v.upcoming_events {
                UpcomingEventsUpdate::Keep => None,
                UpcomingEventsUpdate::Replace(e) => Some(strings(e)),
            },
        }
    }
}
impl UpdateSave {
    fn into_domain(self, p: &str) -> Result<SummaryUpdate> {
        Ok(SummaryUpdate {
            world: optional(&self.world, &format!("{p}.world"), WorldDescription::new)?,
            current_situation: optional(
                &self.current_situation,
                &format!("{p}.current_situation"),
                CurrentSituation::new,
            )?,
            character_updates: self
                .character_updates
                .into_iter()
                .enumerate()
                .map(|(i, c)| c.into_domain(&format!("{p}.character_updates[{i}]")))
                .collect::<Result<Vec<_>>>()?,
            new_major_events: events(self.new_major_events, &format!("{p}.new_major_events"))?,
            consolidated_major_events: events(
                self.consolidated_major_events,
                &format!("{p}.consolidated_major_events"),
            )?,
            upcoming_events: match self.upcoming_events {
                None => UpcomingEventsUpdate::Keep,
                Some(v) => {
                    UpcomingEventsUpdate::Replace(events(v, &format!("{p}.upcoming_events"))?)
                }
            },
        })
    }
}
impl From<&CharacterDelta> for DeltaSave {
    fn from(v: &CharacterDelta) -> Self {
        Self {
            id: sentinel(v.id.as_ref()),
            name: sentinel(v.name.as_ref()),
            description: sentinel(v.description.as_ref()),
            backstory: sentinel(v.backstory.as_ref()),
            relationships: sentinel(v.relationships.as_ref()),
            current_state: sentinel(v.current_state.as_ref()),
        }
    }
}
impl DeltaSave {
    fn into_domain(self, p: &str) -> Result<CharacterDelta> {
        Ok(CharacterDelta {
            id: optional(&self.id, &format!("{p}.id"), CharacterId::new)?,
            name: optional(&self.name, &format!("{p}.name"), CharacterName::new)?,
            description: optional(
                &self.description,
                &format!("{p}.description"),
                CharacterDescription::new,
            )?,
            backstory: optional(&self.backstory, &format!("{p}.backstory"), Backstory::new)?,
            relationships: optional(
                &self.relationships,
                &format!("{p}.relationships"),
                Relationships::new,
            )?,
            current_state: optional(
                &self.current_state,
                &format!("{p}.current_state"),
                CharacterSituation::new,
            )?,
        })
    }
}
impl From<&GenerationProvenance> for ProvenanceSave {
    fn from(v: &GenerationProvenance) -> Self {
        Self {
            provider: v.provider.as_ref().map(|p| p.as_str().into()),
            model: v.model.as_ref().map(|m| m.as_str().into()),
            list_price_estimate: v.cost.as_ref().map(|c| CostSave {
                amount: c.amount().get(),
                currency: c.currency().as_str().into(),
            }),
        }
    }
}
impl ProvenanceSave {
    fn into_domain(self, p: &str) -> Result<GenerationProvenance> {
        Ok(GenerationProvenance {
            provider: self
                .provider
                .as_ref()
                .map(|s| text(s, &format!("{p}.provider"), ProviderName::new))
                .transpose()?,
            model: self
                .model
                .as_ref()
                .map(|s| text(s, &format!("{p}.model"), ModelName::new))
                .transpose()?,
            cost: self
                .list_price_estimate
                .map(|c| {
                    Ok(ListPriceEstimate::new(
                        CostAmount::new(c.amount).map_err(|e| {
                            SaveCodecError::invalid(
                                &format!("{p}.list_price_estimate.amount"),
                                e.to_string(),
                            )
                        })?,
                        text(
                            &c.currency,
                            &format!("{p}.list_price_estimate.currency"),
                            CurrencyCode::new,
                        )?,
                    ))
                })
                .transpose()?,
        })
    }
}
