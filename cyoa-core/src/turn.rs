//! One turn of the story and its log entry.
//! Ports calibre's `QuickActionKind`/`QuickAction`/`StoryTurn`/`TurnRecord`
//! (cyoa.py:264-345, 362-382) and `selected_quick_actions` (cyoa.py:1279).

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use thiserror::Error;

use crate::{
    limits::MajorEventLimit,
    summary::{StorySummary, SummaryUpdate},
    text::{
        ChapterTitle, CurrencyCode, Instructions, ModelName, Narrative, PlayerInput, ProviderName,
        QuickActionText, RawResponse, RenderedPrompt, SceneDescription, matching_key,
    },
};

/// The vocabulary an action's approach is drawn from (calibre `QuickActionKind`).
/// An unrecognized wire value maps to `Other` at the boundary, never here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuickActionKind {
    Cautious,
    Bold,
    Social,
    Investigate,
    Other,
}

/// The kinds requested from the model, in the order the prompt asks for them
/// (calibre `REQUESTED_QUICK_ACTION_KINDS`). Each entry may be satisfied by
/// any one of its listed kinds.
pub const REQUESTED_QUICK_ACTION_KINDS: [&[QuickActionKind]; 3] = [
    &[QuickActionKind::Cautious],
    &[QuickActionKind::Bold],
    &[QuickActionKind::Social, QuickActionKind::Investigate],
];

/// Structural, not a tunable limit: the schema always asks for exactly this
/// many quick actions.
pub const QUICK_ACTION_COUNT: usize = REQUESTED_QUICK_ACTION_KINDS.len();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickAction {
    text: QuickActionText,
    kind: QuickActionKind,
}

impl QuickAction {
    pub fn new(text: QuickActionText, kind: QuickActionKind) -> Self {
        Self { text, kind }
    }

    pub fn text(&self) -> &QuickActionText {
        &self.text
    }

    pub fn kind(&self) -> QuickActionKind {
        self.kind
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("no usable quick actions survived selection")]
pub struct NoQuickActions;

/// One to `QUICK_ACTION_COUNT` actions: stripped, casefold-deduplicated, and
/// preferring one action per kind when the model offered more than fit,
/// restoring the model's original ordering (calibre `selected_quick_actions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickActions(Vec<QuickAction>);

impl QuickActions {
    pub fn select(actions: impl IntoIterator<Item = QuickAction>) -> Result<Self, NoQuickActions> {
        let mut seen = HashSet::new();
        let unique: Vec<QuickAction> = actions
            .into_iter()
            .filter(|a| seen.insert(matching_key(a.text.as_str())))
            .collect();
        if unique.is_empty() {
            return Err(NoQuickActions);
        }
        if unique.len() <= QUICK_ACTION_COUNT {
            return Ok(Self(unique));
        }

        // Insertion-ordered: the first action seen of each kind wins, matching
        // Python's dict.setdefault over an insertion-ordered dict.
        let mut of_kind: IndexMap<QuickActionKind, QuickAction> = IndexMap::new();
        for action in &unique {
            of_kind.entry(action.kind).or_insert_with(|| action.clone());
        }
        let mut chosen: Vec<QuickAction> = of_kind.into_values().take(QUICK_ACTION_COUNT).collect();
        if chosen.len() < QUICK_ACTION_COUNT {
            let picked: HashSet<_> = chosen
                .iter()
                .map(|a| matching_key(a.text.as_str()))
                .collect();
            for action in &unique {
                if chosen.len() >= QUICK_ACTION_COUNT {
                    break;
                }
                if !picked.contains(&matching_key(action.text.as_str())) {
                    chosen.push(action.clone());
                }
            }
        }
        let position: HashMap<_, _> = unique
            .iter()
            .enumerate()
            .map(|(i, a)| (matching_key(a.text.as_str()), i))
            .collect();
        chosen.sort_by_key(|a| position[&matching_key(a.text.as_str())]);
        Ok(Self(chosen))
    }

    pub fn as_slice(&self) -> &[QuickAction] {
        &self.0
    }
}

/// What a turn proposes about chapters. Both variants carry a title so a
/// chapter can be retitled by any of its turns, not only its first — the
/// chapter's effective title is the last `Some` among its turns, and `None`
/// leaves the current title (or the "Chapter N" fallback) untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChapterMarker {
    Continue { title: Option<ChapterTitle> },
    NewChapter { title: Option<ChapterTitle> },
}

/// One turn of the story, already normalized (calibre `validated_turn` folds
/// into these constructors: a blank narrative or an empty selection of quick
/// actions cannot be represented here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryTurn {
    narrative: Narrative,
    quick_actions: QuickActions,
    scene_description: Option<SceneDescription>,
    summary_update: SummaryUpdate,
    chapter: ChapterMarker,
}

impl StoryTurn {
    pub fn new(
        narrative: Narrative,
        quick_actions: QuickActions,
        scene_description: Option<SceneDescription>,
        summary_update: SummaryUpdate,
        chapter: ChapterMarker,
    ) -> Self {
        Self {
            narrative,
            quick_actions,
            scene_description,
            summary_update,
            chapter,
        }
    }

    pub fn narrative(&self) -> &Narrative {
        &self.narrative
    }

    pub fn quick_actions(&self) -> &QuickActions {
        &self.quick_actions
    }

    pub fn scene_description(&self) -> Option<&SceneDescription> {
        self.scene_description.as_ref()
    }

    pub fn summary_update(&self) -> &SummaryUpdate {
        &self.summary_update
    }

    pub fn chapter(&self) -> &ChapterMarker {
        &self.chapter
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[error("cost amount must be finite and non-negative")]
pub struct InvalidCostAmount;

/// A notional list-price estimate, never subscription billing (see
/// `01-claude-cli.md`'s `total_cost_usd`/`costBasis:"list"` finding). Finite
/// and non-negative by construction, so it can soundly implement `Eq`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostAmount(f64);

impl CostAmount {
    pub fn new(amount: f64) -> Result<Self, InvalidCostAmount> {
        if !amount.is_finite() || amount < 0.0 {
            return Err(InvalidCostAmount);
        }
        Ok(Self(amount))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl Eq for CostAmount {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPriceEstimate {
    amount: CostAmount,
    currency: CurrencyCode,
}

impl ListPriceEstimate {
    pub fn new(amount: CostAmount, currency: CurrencyCode) -> Self {
        Self { amount, currency }
    }

    pub fn amount(&self) -> CostAmount {
        self.amount
    }

    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }
}

/// Who and what generated a turn, and an optional cost estimate. All optional:
/// a scripted or replayed backend has none of this.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GenerationProvenance {
    pub provider: Option<ProviderName>,
    pub model: Option<ModelName>,
    pub cost: Option<ListPriceEstimate>,
}

/// The exact instructions and prompt sent for a turn. Kept out of
/// `TurnRecord` by default (calibre `STORE_PROMPTS_IN_TURN_RECORDS`): storing
/// it makes a saved game grow quadratically with a chapter's length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTrace {
    pub instructions: Instructions,
    pub prompt: RenderedPrompt,
}

/// Everything about how a turn was generated, apart from the turn's own
/// content and the post-turn summary. Grouped by name, like
/// `CharacterDetailsFields`, because `input` and `prompt_trace` are both
/// optional and otherwise indistinguishable at a positional call site
/// (`None, .., None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnGenerationRecord {
    pub input: Option<PlayerInput>,
    pub raw_response: RawResponse,
    pub provenance: GenerationProvenance,
    pub prompt_trace: Option<PromptTrace>,
}

/// The log of one exchange with a backend: enough to replay, rewind, or audit
/// what it returned (calibre `TurnRecord`). `input` is `None` both for the
/// opening turn and for an "interesting event" turn, which discards typed
/// input the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRecord {
    input: Option<PlayerInput>,
    turn: StoryTurn,
    raw_response: RawResponse,
    summary: StorySummary,
    provenance: GenerationProvenance,
    prompt_trace: Option<PromptTrace>,
}

impl TurnRecord {
    pub fn new(turn: StoryTurn, summary: StorySummary, record: TurnGenerationRecord) -> Self {
        Self {
            input: record.input,
            turn,
            raw_response: record.raw_response,
            summary,
            provenance: record.provenance,
            prompt_trace: record.prompt_trace,
        }
    }

    pub fn input(&self) -> Option<&PlayerInput> {
        self.input.as_ref()
    }

    pub fn turn(&self) -> &StoryTurn {
        &self.turn
    }

    pub fn raw_response(&self) -> &RawResponse {
        &self.raw_response
    }

    pub fn summary(&self) -> &StorySummary {
        &self.summary
    }

    pub fn provenance(&self) -> &GenerationProvenance {
        &self.provenance
    }

    pub fn prompt_trace(&self) -> Option<&PromptTrace> {
        self.prompt_trace.as_ref()
    }

    pub(crate) fn set_major_event_limit(&mut self, limit: MajorEventLimit) {
        self.summary.set_major_event_limit(limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(text: &str, kind: QuickActionKind) -> QuickAction {
        QuickAction::new(QuickActionText::new(text).unwrap(), kind)
    }

    #[test]
    fn all_blank_or_empty_is_rejected() {
        assert!(QuickActions::select([]).is_err());
    }

    #[test]
    fn casefold_duplicate_actions_are_dropped_preserving_order() {
        // A blank action's text cannot be constructed at all: QuickActionText
        // is nonblank, so blank-filtering is a boundary-DTO concern, not this
        // constructor's — only casefold deduplication happens here.
        let selected = QuickActions::select([
            action("Hide", QuickActionKind::Cautious),
            action("hide", QuickActionKind::Cautious),
            action("Push forward", QuickActionKind::Bold),
        ])
        .unwrap();
        let texts: Vec<_> = selected
            .as_slice()
            .iter()
            .map(|a| a.text().as_str())
            .collect();
        assert_eq!(texts, ["Hide", "Push forward"]);
    }

    #[test]
    fn more_than_three_prefers_one_action_per_kind_in_original_order() {
        let selected = QuickActions::select([
            action("Talk", QuickActionKind::Social),
            action("Hide", QuickActionKind::Cautious),
            action("Attack", QuickActionKind::Bold),
            action("Search", QuickActionKind::Investigate),
            action("Flee", QuickActionKind::Cautious),
        ])
        .unwrap();
        let texts: Vec<_> = selected
            .as_slice()
            .iter()
            .map(|a| a.text().as_str())
            .collect();
        assert_eq!(texts, ["Talk", "Hide", "Attack"]);
    }

    #[test]
    fn fewer_kinds_than_actions_fills_from_the_remainder_in_original_order() {
        let selected = QuickActions::select([
            action("A", QuickActionKind::Cautious),
            action("B", QuickActionKind::Cautious),
            action("C", QuickActionKind::Cautious),
            action("D", QuickActionKind::Bold),
        ])
        .unwrap();
        let texts: Vec<_> = selected
            .as_slice()
            .iter()
            .map(|a| a.text().as_str())
            .collect();
        assert_eq!(texts, ["A", "B", "D"]);
    }

    #[test]
    fn cost_amount_rejects_negative_and_nonfinite() {
        assert!(CostAmount::new(-1.0).is_err());
        assert!(CostAmount::new(f64::NAN).is_err());
        assert!(CostAmount::new(f64::INFINITY).is_err());
        assert!(CostAmount::new(0.0).is_ok());
    }
}
