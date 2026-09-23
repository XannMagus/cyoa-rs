# Engine notes — extracted from `calibre/src/calibre/ai/cyoa.py`

These describe the Python source, not an overriding project specification.
[Project contracts](../docs/decisions/README.md) take precedence: notably exact-record
world deduplication with namesake preservation, ID suffix repair, configurable limits
and restoration policy, owned selection, Unicode lowercase, and later chapter titles.
Do not port a conflicting rule from this file over an existing project regression.

Purpose: everything needed to port the engine logic without re-reading the
Python source (though `reference/calibre/cyoa.py` is kept around for exactly
that, until the port is done). Line numbers refer to that copy.

See `PLAN.md` for the narrative version of most of this; this file is the
denser reference to check against while writing Rust.

## Constants

| Constant | Value | Line | Purpose |
|---|---|---|---|
| `GAME_SERIALIZATION_VERSION` | `4` | 37 | save format; migrations chain v1→v2→v3→v4 |
| `PROTAGONIST_ID` | `'protagonist'` | 42 | fixed id of the played character |
| `MAX_GENERATED_NPCS` | `8` | 79 | cap on world-gen NPCs |
| `MAX_MAJOR_EVENTS` | `30` | 164 | cap on the summary event list |
| `STORE_PROMPTS_IN_TURN_RECORDS` | `False` | 359 | debug only — storing prompts makes saves grow quadratically |
| `MIN_PROSE_CONTEXT_TURNS` | `3` | 437 | bridge window into the previous chapter |
| `MIN_PLAYER_CHARACTERS` | `2` | 1164 | validation floor (3–5 are requested) |
| `NUM_QUICK_ACTIONS` | `3` | 1276 | `len(REQUESTED_QUICK_ACTION_KINDS)` |
| character id truncation | 32 chars | 50 | `'-'.join(words)[:32].rstrip('-') or 'character'` |

## Schemas (LLM-facing)

All are Python `NamedTuple`s with `Annotated[type, 'description']` field docs.
The description text is what's captured verbatim in `schema_docs.toml`. Field
**order matters** — `narrative` is deliberately first in `StoryTurn` so it
streams early — and defaulted fields must be trailing, for the same
forward-compatibility reason `#[serde(default)]` matters in Rust.

- `PlayerCharacter` (82-86): `name`, `description`, `backstory`
- `NonPlayerCharacter` (89-94): `name`, `description`, `backstory`, `relationships`
- `WorldOutline` (97-104) — **LLM call #1**: `title`, `world_description`
- `GeneratedCast` (107-117) — **LLM call #2**: `characters: tuple[PlayerCharacter, ...]` (3-5), `npcs: tuple[NonPlayerCharacter, ...]` (3-8)
- `GeneratedWorld` (120-133) — state only, never requested directly; combines a `WorldOutline` with the generated cast
- `CharacterState` (136-156) — a cast entry inside `StorySummary`: `name`, `description`, `backstory`, `relationships`, `current_state` (defaulted), `id` (defaulted)
- `StorySummary` (167-178) — sent to the model as JSON every turn: `world`, `major_events: tuple[str, ...]` (≤30), `characters: tuple[CharacterState, ...]`, `current_situation`, `upcoming_events: tuple[str, ...]`
- `CharacterDelta` (181-222) — model-emitted, matches by `id`: `id` (required), `current_state` (required — always filled in), `name`/`description`/`backstory`/`relationships` (all defaulted to `''`, meaning "unchanged")
- `SummaryUpdate` (225-256) — **the per-turn delta**: `current_situation` (required), `character_updates: tuple[CharacterDelta, ...]`, `new_major_events: tuple[str, ...]`, `upcoming_events: tuple[str, ...] | None = None` (the one replace-not-merge field), `world: str = ''`, `consolidated_major_events: tuple[str, ...] = ()`
- `QuickActionKind` (264-276) — enum: `cautious`, `bold`, `social`, `investigate`, `other`
- `QuickAction` (312-317): `text`, `kind: QuickActionKind = other`
- `StoryTurn` (320-345) — **LLM call #3, per turn, and the streamed schema**: `narrative` (first!), `quick_actions: tuple[QuickAction, ...]` (exactly 3 requested), `scene_description`, `summary_update: SummaryUpdate`, `starts_new_chapter: bool`, `chapter_title: str | None`

## Non-LLM state types

- `TurnRecord` (362-382): `player_input`, `raw_response`, `turn: StoryTurn`, `summary: StorySummary` (the **post-turn** summary), `chapter: int`, `cost: float = 0`, `currency/provider/model: str = ''`, `instructions/prompt: str = ''` (empty unless `STORE_PROMPTS_IN_TURN_RECORDS`)
- `StoryStyle` (440-451): `art_style/pace/tone/narration: str = ''` — empty string means "first (default) entry of the corresponding table"
- `GameState` (454-522, `@dataclass`): `brief`, `world: GeneratedWorld`, `character_index: int`, `turns: list[TurnRecord] = []`, plus the four style fields **flat, not nested** (so adding a fifth style needs no migration). Properties: `style`, `character`, `current_chapter`, `current_chapter_turns`, `prose_context`, `current_summary`, `chapter_titles`.

## The delta merge — `updated_summary` / `updated_characters` (1318-1406)

This is the part most worth getting exactly right; see PLAN.md's "Risks" section
for why. Rules, precisely:

**`updated_characters(updates, previous)`:**
1. Start from `list(previous.characters)`. Build `by_id: {id: index}` and
   `by_name: {casefold(name): index}`.
2. For each `CharacterDelta`: try `by_id[u.id]` first, then fall back to
   `by_name[casefold(u.name)]` if `u.id` didn't match.
3. If matched and **already updated this turn** → skip silently (second update
   for the same character this turn is dropped).
4. If matched → replace fields: `name = u.name or c.name`, and for
   description/backstory/relationships/current_state:
   `u.field.strip() or c.field` (blank keeps old). If `u.name` is non-empty,
   update `by_name` to point the new casefolded name at this index (so a
   later delta in the *same* turn that refers to the new name still matches).
5. If not matched → this is a new character. Require `name` **and**
   (`description` or `backstory`) non-empty, else drop it entirely — nothing
   about a nameless/undescribed character survives past this turn.
6. New character's id: `unique_character_id(u.id or character_id_for_name(u.name), by_id)`
   — forces uniqueness so a model-invented collision can't merge two
   characters into one.
7. Return the full character tuple.

**`updated_summary(update, previous)`:**
- `world = update.world.strip() or previous.world`; empty after that → error
  (`InvalidAIResponse`).
- `current_situation = update.current_situation.strip() or previous.current_situation`;
  empty → error.
- `characters = updated_characters(...)`; empty → error.
- `events = clean_text_list(update.consolidated_major_events) or previous.major_events`
  (consolidation replaces the base list when the model sends one), then
  `major_events = clean_text_list(events + tuple(update.new_major_events))[-MAX_MAJOR_EVENTS:]`
  — dedup casefolded, preserve order, then keep only the **last 30**.
- `upcoming_events`: **the one replace-not-merge field.**
  `previous.upcoming_events if update.upcoming_events is None else clean_text_list(update.upcoming_events)`.
  `None` = unchanged (the common case — most turns leave this alone).
  `[]` = a real change: the last open thread was just resolved.

`clean_text_list` (1306-1315): strip each item, drop blank and casefold-duplicate
entries, preserve order.

## Validation (1164-1305)

- `validated_world` (1204): non-empty `title` and `world_description`, else error.
- `validated_npcs` (1183): drop NPCs with no name/description/backstory, drop
  ones whose name (casefolded) collides with a playable character, cap at
  `MAX_GENERATED_NPCS` (8).
- `validated_cast` (1218): requires `MIN_PLAYER_CHARACTERS` (2) usable playable
  characters after cleaning, else error.
- `validated_turn` (1409): `narrative.strip()` must be non-empty, else error.
  `quick_actions = selected_quick_actions(turn.quick_actions)`; **zero** surviving
  actions is fatal, 1 or 2 is accepted (see rationale below). `scene_description`
  may be empty — not fatal, just no image. `chapter_title` normalized to `None`
  when blank.
- `selected_quick_actions` (1279): strip each action's text, drop blank and
  casefold-duplicate ones. If ≤3 remain, keep them all (in original order).
  If >3, prefer **one action per kind** (`REQUESTED_QUICK_ACTION_KINDS` order:
  cautious, bold, social-or-investigate) over just taking the first three —
  using an **insertion-ordered** dict (`of_kind.setdefault`), then fill up to 3
  from the rest if fewer than 3 kinds are present, then re-sort by original
  position. **This ordering dependency is why the Rust port needs `IndexMap`,
  not `HashMap`, here.**

Design rationale worth preserving in a comment (1271-1275, verbatim):
> The AI is asked for one quick action of each requested kind, but they are
> only a convenience: the player can always type an action of their own.
> Throwing away a passage of prose the player has already paid for because the
> AI repeated itself and one of the three was deduplicated away would be a far
> worse trade than rendering the two that survived, so any at all are accepted.

## Error/retry posture

`next_turn()` (1448+) **never raises and never retries.** Provider errors and
validation errors both become a returned error value with the raw response
text preserved; on any failure, `state` is left completely untouched. Retry is
a user action from the UI, not automatic. Port this as `Result`, not
exceptions/panics.

## Chapters and derived state

- The model sets `starts_new_chapter` + `chapter_title` each turn; the engine
  decides what it means: `next_turn` increments the chapter only when
  `state.turns` is **already non-empty** — the very first turn can never open
  chapter 1.
- `GameState.current_chapter` = `turns[-1].chapter if turns else 0`.
- `chapter_titles` fills in `"Chapter {n+1}"` for any chapter whose first turn
  left `chapter_title` blank.
- `GameState.prose_context` (501-510) returns `(bridge, current)`:
  - `current` = all turns whose `chapter == current_chapter`
  - `bridge` = turns among the **last `MIN_PROSE_CONTEXT_TURNS` (3)** turns
    that belong to an *earlier* chapter than the current one
  - Because chapter numbers are non-decreasing along `turns`, both halves are
    contiguous — in Rust this is a `partition_point` + slice, not an
    allocation.
- `rewind(state, n)` = `del state.turns[-n:]`. Everything else (summary,
  chapter, prose context) is derived from `turns`, so rewind needs no other
  bookkeeping. Guard: `0 < n <= len(turns)`, else `ValueError`.
- `apply_character_edits` (543+): edits to *durable* character fields
  (name/description/backstory/relationships) are written into **every**
  stored turn's summary, so they survive a later rewind. `current_state` is
  written to the **last turn only** — it's a snapshot of where the story stood
  as that turn ended, and an earlier turn should still show the character as
  they were then.

## Streaming

`narrative_streamer()` (1434) wraps `StreamingStringField('narrative')`
(`structured.py:377`). This is a minimal JSON **scanner**, not a parser: it
tracks nesting depth, in-string state, backslash-escapes, and `\uXXXX`
surrogate pairs — just enough to know which top-level key a string value
belongs to — and skips everything before the first `{` so leading code fences
or prose don't confuse it. `feed(fragment) -> str` returns newly-decoded
characters of the target field, or `""` once the field is done or never
appears.

Verified against the real `claude -p` backend (see `01-claude-cli.md`): the
`partial_json` deltas it emits are exactly this kind of raw-JSON-fragment
stream, so the scanner design transfers directly — no adaptation needed beyond
the Rust port itself.

## Lifecycle (from the GUI, `gui2/cyoa/*.py`, for reference)

1. **Brief** — free text or a premade world brief (untranslated English, since
   "AI models work best with English instructions" — comment at `world.py:83`).
2. **World generation** (LLM call #1) — `WorldOutline` only, no characters yet,
   deliberately: the cast is generated *after* the player accepts/edits the
   world, so it matches the edited version.
3. **World edit** — hand-editing only; there is no "refine with AI" call.
   Going back to the brief page invalidates in-flight calls and starts over.
4. **Cast generation** (LLM call #2) — `GeneratedCast`, validated/cleaned.
   Re-editing the world after this offers to regenerate the cast.
5. **Character selection** — pick which `PlayerCharacter` to play; NPCs and
   playables are both editable here.
6. **Opening turn** (LLM call #3, first of many) — `state.turns` is empty, so
   `turn_prompt` takes the "Begin the novel..." branch. The initial
   `StorySummary` is synthesized by Python (`initial_summary`, 404-428), not
   generated: `current_situation = 'The adventure has not yet begun.'`, every
   NPC's `current_state = 'Has not yet appeared in the story.'`
   (`NPC_NOT_YET_MET`, line 387), protagonist id = `PROTAGONIST_ID`.
7. **Steady-state loop** — free text, a quick action's text, or "something
   interesting happens" (which discards any typed input and asks the model to
   pay off an `upcoming_events` thread if one exists) as the turn input.

Not in v1 scope, kept as reference for later: scene/portrait image generation
(`scene_description` field, `character_portrait_prompt`/`scene_image_prompt`
at 878-893), premade worlds table, EPUB/read-the-story UI.
