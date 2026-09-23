//! The complete state of a game and its derived views.
//! Ports calibre's `GameState` (cyoa.py:454-540): everything but the turn log
//! is derived, which is why rewinding is trivial and why there is no stored
//! chapter index here — chapter membership is derived from each turn's
//! `ChapterMarker` instead (see `turn.rs`), extending calibre's rule that a
//! chapter title comes from its first turn: here the effective title is the
//! *last* title any of the chapter's turns supplied, so any turn can retitle
//! its chapter, not only the one that opened it.

use std::{borrow::Cow, collections::HashSet, num::NonZeroUsize};

use thiserror::Error;

use crate::{
    character::{Character, CharacterCast, CharacterDetails, CharacterDetailsFields},
    ids::CharacterId,
    limits::Limits,
    style::StoryStyle,
    summary::{EventList, MajorEvents, StorySummary},
    text::{Brief, ChapterTitle, CharacterSituation, CurrentSituation},
    turn::{ChapterMarker, StoryTurn, TurnGenerationRecord, TurnRecord},
    world::{PlayableIndex, World},
};

const NPC_NOT_YET_MET: &str = "Has not yet appeared in the story.";
const ADVENTURE_NOT_YET_BEGUN: &str = "The adventure has not yet begun.";

/// A positive number of turns to rewind. Zero is unrepresentable, so the only
/// failure `GameState::rewind` can report is "more than exist".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnCount(NonZeroUsize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a rewind must undo at least one turn")]
pub struct InvalidTurnCount;

impl TurnCount {
    pub fn new(turns: usize) -> Result<Self, InvalidTurnCount> {
        NonZeroUsize::new(turns).map(Self).ok_or(InvalidTurnCount)
    }

    pub fn get(self) -> usize {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("cannot rewind {requested} turn(s) in a game with {available} turn(s)")]
pub struct InvalidRewind {
    requested: usize,
    available: usize,
}

/// A zero-based chapter position. `ordinal()` gives the one-based number used
/// by the "Chapter N" fallback title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChapterNumber(usize);

impl ChapterNumber {
    pub fn get(self) -> usize {
        self.0
    }

    pub fn ordinal(self) -> usize {
        self.0 + 1
    }
}

/// A contiguous run of turns sharing a chapter, with the chapter's effective
/// title (the last title any of its turns supplied, or `None` for the
/// "Chapter N" fallback, which presentation/export formats — this is
/// deliberately not localized here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chapter<'a> {
    number: ChapterNumber,
    title: Option<&'a ChapterTitle>,
    turns: &'a [TurnRecord],
}

impl<'a> Chapter<'a> {
    pub fn number(&self) -> ChapterNumber {
        self.number
    }

    pub fn title(&self) -> Option<&'a ChapterTitle> {
        self.title
    }

    pub fn turns(&self) -> &'a [TurnRecord] {
        self.turns
    }
}

fn chapter_title(turns: &[TurnRecord]) -> Option<&ChapterTitle> {
    turns
        .iter()
        .filter_map(|t| match t.turn().chapter() {
            ChapterMarker::Continue { title } | ChapterMarker::NewChapter { title } => {
                title.as_ref()
            }
        })
        .next_back()
}

/// The complete state of a game.
///
/// ```compile_fail
/// use cyoa_core::game::GameState;
/// fn push_turn_directly(state: &mut GameState, turn: cyoa_core::turn::TurnRecord) {
///     state.turns.push(turn); // `turns` is private: only `commit_turn` can extend the log.
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    brief: Brief,
    world: World,
    protagonist: PlayableIndex,
    style: StoryStyle,
    turns: Vec<TurnRecord>,
    limits: Limits,
}

impl GameState {
    /// Starts a new game. `protagonist` must have been obtained from this
    /// same `world`'s cast (`WorldCast::playable_index`), which is the only
    /// way to construct one, so this cannot fail on an out-of-range index.
    pub fn start(
        brief: Brief,
        world: World,
        protagonist: PlayableIndex,
        style: StoryStyle,
        limits: Limits,
    ) -> Self {
        Self {
            brief,
            world,
            protagonist,
            style,
            turns: Vec::new(),
            limits,
        }
    }

    /// Restores a game from its persisted turn log. Persistence is a
    /// boundary concern: the caller maps a save's stored chapter indices
    /// into `ChapterMarker`s before calling this.
    pub fn restore(
        brief: Brief,
        world: World,
        protagonist: PlayableIndex,
        style: StoryStyle,
        limits: Limits,
        turns: Vec<TurnRecord>,
    ) -> Self {
        Self {
            brief,
            world,
            protagonist,
            style,
            turns,
            limits,
        }
    }

    pub fn brief(&self) -> &Brief {
        &self.brief
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn protagonist(&self) -> &crate::world::PlayerCharacter {
        self.world.cast().playable_at(self.protagonist)
    }

    pub fn style(&self) -> &StoryStyle {
        &self.style
    }

    /// Changing the style mid-game affects only turns played from now on;
    /// past turns keep whatever style produced their prose.
    pub fn set_style(&mut self, style: StoryStyle) {
        self.style = style;
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    pub fn turns(&self) -> &[TurnRecord] {
        &self.turns
    }

    /// The story memory sent to the model: the last turn's summary, or a
    /// synthesized opening summary before any turn has been played (calibre
    /// `initial_summary`, cyoa.py:404).
    pub fn current_summary(&self) -> Cow<'_, StorySummary> {
        match self.turns.last() {
            Some(turn) => Cow::Borrowed(turn.summary()),
            None => Cow::Owned(self.initial_summary()),
        }
    }

    fn initial_summary(&self) -> StorySummary {
        let protagonist = self.protagonist();
        // calibre's initial_summary (cyoa.py:408) starts the protagonist with
        // blank relationships and no current_state; both stay unset until the
        // first turn's SummaryUpdate fills them in.
        let protagonist_details = CharacterDetails::new(CharacterDetailsFields {
            description: Some(protagonist.description().clone()),
            backstory: Some(protagonist.backstory().clone()),
            relationships: None,
            current_state: None,
        })
        .expect("a player character always has a description and backstory");
        let protagonist_character = Character::new(
            CharacterId::protagonist(),
            protagonist.name().clone(),
            protagonist_details,
        );

        let not_yet_met = CharacterSituation::new(NPC_NOT_YET_MET).expect("literal is non-blank");
        let mut taken: HashSet<CharacterId> = HashSet::new();
        taken.insert(protagonist_character.id().clone());
        let mut characters = vec![protagonist_character];
        for npc in self.world.cast().npcs() {
            let id = CharacterId::for_name(npc.name().as_str()).unique(|id| taken.contains(id));
            taken.insert(id.clone());
            let details = CharacterDetails::new(CharacterDetailsFields {
                description: Some(npc.description().clone()),
                backstory: Some(npc.backstory().clone()),
                relationships: npc.relationships().cloned(),
                current_state: Some(not_yet_met.clone()),
            })
            .expect("a world-generated npc always has a description and backstory");
            characters.push(Character::new(id, npc.name().clone(), details));
        }
        let cast =
            CharacterCast::new(characters).expect("the protagonist alone makes the cast non-empty");

        StorySummary::new(
            self.world.outline().description().clone(),
            CurrentSituation::new(ADVENTURE_NOT_YET_BEGUN).expect("literal is non-blank"),
            cast,
            MajorEvents::new(EventList::default(), self.limits.max_major_events),
            EventList::default(),
        )
    }

    /// The turns of the current chapter (`current`), plus a bridge of turns
    /// from the previous chapter (`bridge`) so the prose sent to the model
    /// does not collapse to a single passage right after a chapter break.
    /// Both halves are contiguous slices of the turn log.
    pub fn prose_context(&self) -> (&[TurnRecord], &[TurnRecord]) {
        let n = self.turns.len();
        if n == 0 {
            return (&[], &[]);
        }
        let mut current_start = n - 1;
        while current_start > 0
            && !matches!(
                self.turns[current_start].turn().chapter(),
                ChapterMarker::NewChapter { .. }
            )
        {
            current_start -= 1;
        }
        let bridge_start = n.saturating_sub(self.limits.prose_bridge_turns.get());
        (
            &self.turns[bridge_start.min(current_start)..current_start],
            &self.turns[current_start..],
        )
    }

    /// Every chapter played so far, in order, with its effective title.
    pub fn chapters(&self) -> Vec<Chapter<'_>> {
        let mut chapters = Vec::new();
        let mut start = 0usize;
        for (i, turn) in self.turns.iter().enumerate() {
            // The first turn can never open a new chapter (calibre: the
            // chapter counter increments only when turns is already
            // non-empty), so a NewChapter marker at index 0 titles chapter 0
            // instead of starting a phantom chapter before it.
            let opens_new =
                i > 0 && matches!(turn.turn().chapter(), ChapterMarker::NewChapter { .. });
            if opens_new {
                chapters.push(Chapter {
                    number: ChapterNumber(chapters.len()),
                    title: chapter_title(&self.turns[start..i]),
                    turns: &self.turns[start..i],
                });
                start = i;
            }
        }
        if !self.turns.is_empty() {
            chapters.push(Chapter {
                number: ChapterNumber(chapters.len()),
                title: chapter_title(&self.turns[start..]),
                turns: &self.turns[start..],
            });
        }
        chapters
    }

    pub fn current_chapter(&self) -> Option<Chapter<'_>> {
        self.chapters().into_iter().next_back()
    }

    /// Undoes the last `count` turns. Everything else (summary, chapter
    /// position) is derived from `turns`, so nothing else needs updating.
    pub fn rewind(&mut self, count: TurnCount) -> Result<(), InvalidRewind> {
        let count = count.get();
        if count > self.turns.len() {
            return Err(InvalidRewind {
                requested: count,
                available: self.turns.len(),
            });
        }
        self.turns.truncate(self.turns.len() - count);
        Ok(())
    }

    /// Commits a validated turn. Infallible: every check `validated_turn`
    /// performed in calibre is now carried by `StoryTurn`'s own constructor,
    /// so nothing here can reject the turn — a failed generation is never
    /// passed to this method in the first place, keeping "state untouched on
    /// error" structural rather than enforced by a runtime check.
    pub fn commit_turn(&mut self, turn: StoryTurn, record: TurnGenerationRecord) {
        let summary = self.current_summary().updated(turn.summary_update());
        self.turns.push(TurnRecord::new(turn, summary, record));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        summary::SummaryUpdate,
        text::RawResponse,
        text::{
            Backstory, CharacterDescription, CharacterName, Narrative, QuickActionText,
            WorldDescription, WorldTitle,
        },
        turn::{
            ChapterMarker, GenerationProvenance, QuickAction, QuickActionKind, QuickActions,
            StoryTurn, TurnGenerationRecord,
        },
        world::{NonPlayerCharacter, PlayerCharacter, WorldCast, WorldOutline},
    };

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

    fn game_with_npcs(npcs: Vec<NonPlayerCharacter>) -> GameState {
        let limits = Limits::default();
        let outline = WorldOutline::new(
            WorldTitle::new("Title").unwrap(),
            WorldDescription::new("A world").unwrap(),
        );
        let cast = WorldCast::new([player("Alex"), player("Blair")], npcs, &limits).unwrap();
        let protagonist = cast.playable_index(0).unwrap();
        let world = World::new(outline, cast);
        GameState::start(
            Brief::new("brief").unwrap(),
            world,
            protagonist,
            StoryStyle::default(),
            limits,
        )
    }

    fn game() -> GameState {
        game_with_npcs(vec![npc("Nico")])
    }

    fn turn(chapter: ChapterMarker) -> StoryTurn {
        StoryTurn::new(
            Narrative::new("Some prose.").unwrap(),
            QuickActions::select([QuickAction::new(
                QuickActionText::new("Go").unwrap(),
                QuickActionKind::Bold,
            )])
            .unwrap(),
            None,
            SummaryUpdate::default(),
            chapter,
        )
    }

    fn commit(state: &mut GameState, chapter: ChapterMarker) {
        state.commit_turn(
            turn(chapter),
            TurnGenerationRecord {
                input: None,
                raw_response: RawResponse::new("{}").unwrap(),
                provenance: GenerationProvenance::default(),
                prompt_trace: None,
            },
        );
    }

    #[test]
    fn initial_summary_gives_protagonist_and_unmet_npcs() {
        let state = game();
        let summary = state.current_summary();
        assert_eq!(summary.characters().len(), 2);
        let protagonist = summary
            .characters()
            .get(&CharacterId::protagonist())
            .unwrap();
        assert_eq!(protagonist.name().as_str(), "Alex");
        let nico_id = CharacterId::for_name("Nico");
        let nico = summary.characters().get(&nico_id).unwrap();
        assert_eq!(
            nico.details().current_state().unwrap().as_str(),
            NPC_NOT_YET_MET
        );
    }

    #[test]
    fn first_turn_cannot_open_a_chapter() {
        let mut state = game();
        commit(
            &mut state,
            ChapterMarker::NewChapter {
                title: Some(ChapterTitle::new("Prologue").unwrap()),
            },
        );
        let chapters = state.chapters();
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].number().get(), 0);
        assert_eq!(chapters[0].title().unwrap().as_str(), "Prologue");
    }

    #[test]
    fn later_turn_retitles_its_chapter_and_rewind_restores_the_earlier_title() {
        let mut state = game();
        commit(&mut state, ChapterMarker::Continue { title: None });
        commit(
            &mut state,
            ChapterMarker::Continue {
                title: Some(ChapterTitle::new("Renamed").unwrap()),
            },
        );
        assert_eq!(
            state.current_chapter().unwrap().title().unwrap().as_str(),
            "Renamed"
        );
        state.rewind(TurnCount::new(1).unwrap()).unwrap();
        assert!(state.current_chapter().unwrap().title().is_none());
    }

    #[test]
    fn new_chapter_marker_starts_a_second_chapter() {
        let mut state = game();
        commit(&mut state, ChapterMarker::Continue { title: None });
        commit(
            &mut state,
            ChapterMarker::NewChapter {
                title: Some(ChapterTitle::new("Chapter Two").unwrap()),
            },
        );
        let chapters = state.chapters();
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].turns().len(), 1);
        assert_eq!(chapters[1].turns().len(), 1);
        assert_eq!(chapters[1].title().unwrap().as_str(), "Chapter Two");
    }

    #[test]
    fn rewind_rejects_undoing_more_turns_than_exist() {
        let mut state = game();
        commit(&mut state, ChapterMarker::Continue { title: None });
        assert!(state.rewind(TurnCount::new(2).unwrap()).is_err());
        assert!(state.rewind(TurnCount::new(1).unwrap()).is_ok());
        assert!(state.turns().is_empty());
    }

    #[test]
    fn turn_count_rejects_zero() {
        assert!(TurnCount::new(0).is_err());
    }

    #[test]
    fn prose_context_matches_a_naive_transcription_of_the_python_filter() {
        fn naive_prose_context(
            turns: &[TurnRecord],
            bridge_window: usize,
        ) -> (Vec<usize>, Vec<usize>) {
            // Direct transcription of GameState.prose_context (cyoa.py:501-510),
            // operating on indices with a locally derived chapter number per turn.
            let mut chapter_of = Vec::with_capacity(turns.len());
            let mut chapter = 0usize;
            for (i, t) in turns.iter().enumerate() {
                if i > 0 && matches!(t.turn().chapter(), ChapterMarker::NewChapter { .. }) {
                    chapter += 1;
                }
                chapter_of.push(chapter);
            }
            let current_chapter = chapter_of.last().copied().unwrap_or(0);
            let current: Vec<usize> = (0..turns.len())
                .filter(|&i| chapter_of[i] == current_chapter)
                .collect();
            let tail_start = turns.len().saturating_sub(bridge_window);
            let bridge: Vec<usize> = (tail_start..turns.len())
                .filter(|&i| chapter_of[i] != current_chapter)
                .collect();
            (bridge, current)
        }

        for bridge_window in 0..=4 {
            for chapter_breaks in 0u32..(1 << 6) {
                let mut state = game();
                let mut limits = state.limits();
                limits.prose_bridge_turns = crate::limits::ProseBridgeTurns::new(bridge_window);
                let outline = WorldOutline::new(
                    WorldTitle::new("Title").unwrap(),
                    WorldDescription::new("A world").unwrap(),
                );
                let cast =
                    WorldCast::new([player("Alex"), player("Blair")], vec![], &limits).unwrap();
                let protagonist = cast.playable_index(0).unwrap();
                state = GameState::start(
                    Brief::new("brief").unwrap(),
                    World::new(outline, cast),
                    protagonist,
                    StoryStyle::default(),
                    limits,
                );

                for turn_index in 0..6u32 {
                    let opens_new = (chapter_breaks >> turn_index) & 1 == 1;
                    let marker = if opens_new {
                        ChapterMarker::NewChapter { title: None }
                    } else {
                        ChapterMarker::Continue { title: None }
                    };
                    commit(&mut state, marker);
                }

                let (bridge, current) = state.prose_context();
                let (naive_bridge, naive_current) =
                    naive_prose_context(state.turns(), bridge_window);
                assert_eq!(bridge.len(), naive_bridge.len());
                assert_eq!(current.len(), naive_current.len());
                assert!(bridge.len() + current.len() <= state.turns().len());
            }
        }
    }
}
