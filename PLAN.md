# Port calibre's CYOA game to a standalone Rust TUI, backed by a CLI coding-agent's headless mode

## Context

Calibre ships a built-in AI game, "Create your own adventure" (CYOA): an LLM-driven
choose-your-own-adventure engine with world generation, a generated cast, and an
open-ended turn loop. Its engine lives in `/home/ahmed/calibre/src/calibre/ai/cyoa.py`
(~2475 lines, deliberately Qt-free) with a Qt GUI in
`/home/ahmed/calibre/src/calibre/gui2/cyoa/`.

The user wants this specific functionality but:

- does not use calibre and does not want Python;
- does not want to pay per-token for a metered API, and does not want to run a local
  LLM. They hold subscriptions to more than one CLI coding agent (Claude Code, OpenAI
  Codex CLI) across different machines, and want generation to ride whichever
  subscription is available on the machine they're working from, not a specific
  vendor.

Calibre's CYOA reaches every provider through one pluggable interface
(`AIProvider.generate_structured_output`, `cyoa.py:64-70`) with only three call sites,
so the engine is cleanly separable from the backend. The plan is therefore a faithful
port of the engine logic and prompts into Rust, with **two co-equal backends** that
each shell out to a CLI coding agent's headless/non-interactive mode as a subprocess:
`claude -p` (Claude Code) and `codex exec` (OpenAI Codex CLI). Because the engine owns
all state and the model owns none, a stateless one-shot subprocess per call is an
exact match for the original design rather than a compromise, and it holds equally
for both backends.

**Neither backend is the "real" one with the other as a fallback.** This project will
likely be implemented across sessions using different coding agents on different
machines (a Claude Code session on one machine, a Codex session at home) — each of
which comes with live, already-authenticated access to its *own* CLI (`claude` or
`codex` respectively) but not necessarily the other. See "Backend parity and
cross-agent handoff" below for how that shapes the work.

Intended outcome: a standalone Rust TUI binary, in its own new project directory
(not the calibre repo), that plays the same game, saves and resumes, and exports the
finished story to Markdown/EPUB.

## Decisions taken

| Area | Decision |
|---|---|
| UI | Full TUI with `ratatui` |
| Inference | Stateless one-shot CLI call per turn, behind a pluggable `Backend` trait, with **two co-equal implementations**: `claude -p` (Claude Code headless mode) and `codex exec` (OpenAI Codex CLI headless mode) — see "Backend parity and cross-agent handoff" below |
| Prompts | Ported **verbatim** from calibre, externalized to TOML (tunable without recompiling) |
| v1 scope | World gen, cast gen, character select, turn loop, save/resume, TUI, **plus Markdown/EPUB export** |
| Images | Out of scope for v1, but the image backend is defined as a trait with a stub impl so a local Stable Diffusion backend (ComfyUI / A1111 HTTP, or `stable-diffusion.cpp`) drops in later without refactoring |

## Reference implementation: what must be ported faithfully

The value in the original is concentrated in a handful of non-obvious design decisions.
These are the parts worth copying exactly rather than reinventing.

### The memory model (the whole point of the design)

The model never sees the full transcript. Context is bounded by two mechanisms:

1. **A Python-owned `StorySummary`** (`cyoa.py:167-178`) serialized to JSON and sent in
   full every turn: world, `major_events` (capped at `MAX_MAJOR_EVENTS` = 30),
   `characters` (each with a stable lowercase `id`), `current_situation`,
   `upcoming_events`.
2. **Prose context** (`GameState.prose_context`, `cyoa.py:501-510`): the turns of the
   current chapter, plus — only until the new chapter holds `MIN_PROSE_CONTEXT_TURNS`
   (3) turns of its own — a *bridge* of turns from the previous chapter. Without the
   bridge, a chapter break collapses the prose context to a single passage and takes
   the ground out from under "continue seamlessly from where the chapter's prose ends".

The model emits only a **`SummaryUpdate` delta** each turn, never the whole summary.
The comment at `cyoa.py:1374-1379` gives the reason: asking the AI to re-emit the whole
summary every turn "can silently lose the plot".

### The delta merge (`updated_summary` / `updated_characters`, `cyoa.py:1318-1406`)

Subtle and worth porting rule-for-rule:

- **Empty means unchanged.** Every field left blank keeps its existing value
  (`new.strip() or old`), so "unchanged" is the default rather than something the model
  has to achieve by retyping text it was just sent.
- **Characters are matched by stable `id` first, casefolded `name` second.** Fiction
  renames characters constantly ("the stranger" turns out to be Marlo); matching by id
  updates the entry instead of forking it in two. The name fallback exists because the
  model cannot be relied on to carry ids forward.
- A second update for a character already updated this turn is **silently dropped**
  (`cyoa.py:1339-1340`).
- A new character is dropped unless it has a name **and** (description or backstory) —
  without those, nothing about them outlives the turn.
- Model-invented ids are forced unique (`unique_character_id`) so a collision cannot
  merge two characters into one.
- `major_events` = `(consolidated_major_events or previous)` + `new_major_events`,
  deduped casefold, then **`[-30:]`**. The model is asked to consolidate as the list
  nears the cap; truncation is the backstop for when it doesn't.
- **`upcoming_events` is the one replace-not-merge field.** `null` = unchanged (the
  common case), a list = full replacement, `[]` = the last thread was resolved. There is
  no way to say "drop this one" in a list of bare strings, hence replacement.

### Error and validation posture

- `next_turn()` **never raises and never retries** (`cyoa.py:1448+`). Errors are returned
  as values; retry is user-driven from the UI. Port this posture directly — it maps
  naturally onto `Result` in Rust.
- On provider error, **state is left untouched**.
- `validated_turn` (`cyoa.py:1409`) rejects an empty narrative and zero quick actions,
  but tolerates an empty `scene_description` and accepts 1 or 2 quick actions. The
  comment at `cyoa.py:1271-1275` explains why: throwing away a passage of prose the
  player has already paid for, because a duplicate action was deduped away, is a far
  worse trade than rendering the two that survived.
- `selected_quick_actions` (`cyoa.py:1279`): strip, drop blanks and casefold-duplicates,
  cap at 3, preferring one action per kind over simply taking the first three, and
  restoring the model's original ordering.

### Derived state makes rewind trivial

`GameState` stores only `brief`, `world`, `character_index`, `turns[]` and four style
keys. Chapter position, current summary and prose context are all **derived from the
turn log**, so `rewind` is `del state.turns[-n:]` (`cyoa.py:535-540`). Preserve this —
it is why the feature gets undo essentially for free.

Related subtlety (`apply_character_edits`, `cyoa.py:543+`): player edits to *durable*
character fields (name/description/backstory/relationships) are written into **every**
stored turn's summary so they survive rewinding, while `current_state` is written to the
**last turn only**, since it is a snapshot of where the story stood as that turn ended.

### Chapters

The model sets `starts_new_chapter` + `chapter_title`; the engine decides what that
means. The chapter counter increments only when `state.turns` is non-empty, so the
**first turn cannot open chapter 1**. Untitled chapters fall back to "Chapter N".

### Streaming

`narrative` is deliberately the **first field** of `StoryTurn` so it arrives early
(`cyoa.py:320-345`, and `StoryTurn._fields[0]` is what gets streamed at `cyoa.py:1439`).
`StreamingStringField` (`structured.py:377-440`) is a minimal JSON *scanner*, not a
parser: it tracks nesting depth, in-string state, escapes and `\uXXXX` surrogate pairs —
just enough to know which top-level key a string belongs to — and skips any text before
the opening brace so code fences don't confuse it.

### Constants

| Constant | Value | Purpose |
|---|---|---|
| `MAX_MAJOR_EVENTS` | 30 | summary event cap |
| `MIN_PROSE_CONTEXT_TURNS` | 3 | bridge window |
| `MAX_GENERATED_NPCS` | 8 | world-gen NPC cap |
| `MIN_PLAYER_CHARACTERS` | 2 | validation floor (3–5 requested) |
| `NUM_QUICK_ACTIONS` | 3 | quick actions per turn |
| `PROTAGONIST_ID` | `'protagonist'` | fixed id of the played character |
| character id truncation | 32 chars | `character_id_for_name` |

---

## Backend parity and cross-agent handoff

**Both backends below are equally important. Neither is the "real" one with the
other as a fallback** — v1 is not done until `ClaudeCliBackend` and
`CodexCliBackend` are both implemented and both verified live.

The practical reality driving this: this project is implemented across sessions,
possibly by different coding agents on different machines — a Claude Code session
has a live, already-authenticated `claude` CLI riding its own subscription; a
Codex session (e.g. at home) has a live, already-authenticated `codex` CLI riding
its own plan. Neither session can be assumed to have the other vendor's CLI
authenticated, or even installed. That shapes how work on each backend proceeds:

- **Verify your own backend against your own harness.** If you're implementing or
  verifying `ClaudeCliBackend`, do it from a session with `claude` authenticated —
  run real `claude -p` calls and record actual output, the way `01-claude-cli.md`
  was built (`--help` first, then real invocations, then a real streaming call).
  If you're implementing or verifying `CodexCliBackend`, do the equivalent with a
  real, authenticated `codex exec`. Don't guess at the other vendor's live
  behavior from inside a session that can't reach it.
- **Scaffold the backend you can't test from its published API/CLI docs, and say
  so.** Both backends' trait implementations, config plumbing, and CLI
  invocation-building can and should exist even before they're live-verified —
  don't block on auth. Write that scaffolding against the vendor's own published
  documentation (official CLI docs, `--help` output) rather than invented
  behavior, and mark every such assumption inline, e.g.
  `// UNVERIFIED: from OpenAI's published codex exec docs, not run live — see reference/02-codex-cli.md`.
  This is exactly the state `CodexCliBackend` is in right now: flags are
  confirmed to exist, but no real generation has been observed.
- **Each backend's `reference/0N-*-cli.md` file is the single source of truth for
  what's actually been verified**, split into a "Confirmed" section (only things
  actually run and observed against a live call) and an "Open questions" section
  (documented behavior from `--help`/docs, not yet exercised). Whichever agent
  picks up a backend next reads its own vendor's doc, resolves the open
  questions with a real transcript, and moves them into "Confirmed" — never by
  editing the *other* backend's doc secondhand, and never by marking something
  confirmed without having actually run it.
- **Neither backend blocks the other, or the rest of the build.** `cyoa-core`'s
  `Backend` trait, the engine, prompts, merge/validate logic, persistence, and
  TUI are backend-agnostic and are built and tested via `ScriptedBackend`/
  `--demo` without either CLI (see Phase 0). A session with only `codex`
  available can do all of that, plus finish and verify `CodexCliBackend`, while
  `ClaudeCliBackend` sits scaffolded-but-unverified for whoever next has
  `claude` — and symmetrically the other way around.

---

## Backend: `claude -p` (Claude Code headless mode)

**Status: verified live** against the installed `claude` binary — see
`reference/01-claude-cli.md` for the full transcript evidence. **The flag surface
below is confirmed working; several plausible-looking alternatives are actively
wrong.**

### The invocation

```
claude -p \
  --safe-mode \
  --tools "" \
  --permission-prompts none \
  --no-session-persistence \
  --model sonnet \
  --system-prompt "<instructions>" \
  --json-schema '<json schema>' \
  --output-format stream-json --verbose --include-partial-messages
```

with the **user prompt written to stdin** (not passed as an argv string — prompts are
many KB and contain arbitrary player text).

### Why each flag

| Flag | Reason |
|---|---|
| `--safe-mode` | Disables CLAUDE.md, skills, plugins, hooks, MCP servers, custom agents. **Auth, model selection and permissions keep working normally.** This is the correct isolation flag. |
| `--tools ""` | Disables all built-in tools. Structured output still works (see below). |
| `--permission-prompts none` | Anything that would prompt is denied automatically — guarantees the subprocess can never hang waiting for a human. |
| `--no-session-persistence` | Truly stateless; no transcript files accumulating per turn. |
| `--json-schema` | **Native structured output.** No need to inject a TypeScript schema rendering into the prompt the way calibre's fallback path does. |
| `--output-format stream-json --verbose --include-partial-messages` | Required together to get incremental deltas. |

### ⚠️ Do NOT use `--bare`

`--bare` looks like the right isolation flag and is not. Its own help text: *"Anthropic
auth is strictly `ANTHROPIC_API_KEY` or apiKeyHelper via `--settings` (OAuth and keychain
are never read)."* Using it would force API-key auth and **defeat the entire purpose of
this project** — riding the subscription. `--safe-mode` gives the isolation without
touching auth.

### How structured output actually arrives

It is implemented as a **forced tool call** named `StructuredOutput` (`stop_reason` comes
back as `tool_use`, `num_turns: 2`). `--tools ""` does not block it. Observed stream:

```
{"type":"stream_event","event":{"type":"content_block_start",
  "content_block":{"type":"tool_use","name":"StructuredOutput","input":{}}}}
{"type":"stream_event","event":{"type":"content_block_delta",
  "delta":{"type":"input_json_delta","partial_json":"{\"narrative\": \"The firelight gu"}}}
{"type":"stream_event","event":{"type":"content_block_delta",
  "delta":{"type":"input_json_delta","partial_json":"ttered as the"}}}
...
{"type":"result", ... "structured_output":{...}, "total_cost_usd":0.00664, "usage":{...}}
```

**This is the key finding for the port:** the `partial_json` fragments are exactly the
raw-JSON-text stream that calibre's `StreamingStringField` is designed to consume. The
scanner ports 1:1 — feed each `partial_json` into it and it yields the decoded characters
of `narrative` as they arrive. Keeping `narrative` as the first field of the schema
remains essential for the same reason it is in calibre.

The final `{"type":"result"}` line carries a pre-parsed `structured_output` object (so the
engine does not have to parse the accumulated JSON itself, though it should validate it),
plus `total_cost_usd` and `usage`.

### Gotcha found while testing: do not ask for JSON in the system prompt

The first test used a system prompt saying *"Output only JSON"* **together with**
`--json-schema`. The model double-encoded — it serialized an entire nested object into a
single string field. With `--json-schema`, the system prompt must describe the fields
*semantically* (`narrative = the prose passage`) and never mention JSON or formatting.

This has a direct consequence for the verbatim prompt port: calibre's instructions are
written for a provider abstraction that sometimes needs prompt-injected schemas.
**Audit the ported instruction text for any JSON/formatting directives and strip them**,
keeping only the semantic field rules. The Markdown formatting instructions
(`markdown_instructions()`) are about the *prose inside* `narrative` and must stay.

### Other verified facts

- Baseline overhead is ~1000 input tokens per call even with `--safe-mode`.
- `total_cost_usd` is reported with `"costBasis":"list"` — it is **notional list pricing,
  not subscription billing**. Display it as an estimate or not at all; do not present it
  as money spent.
- `--max-budget-usd` exists as a guard rail if desired.
- Exit codes: 0 success, 1 failure (including rate limits), 2 partial, 130/143 signals.
  Rate limits surface as a non-zero exit plus an error message; there is no queueing, so
  the retry decision belongs to the game — which matches calibre's "never auto-retry,
  let the player retry" posture exactly.
- Check `is_error` in the result envelope in addition to the exit code.

---

## Backend: `codex exec` (OpenAI Codex CLI headless mode)

**Status: flags confirmed, live generation not yet verified.** Codex CLI
(`npm i -g @openai/codex`, or `npx @openai/codex`) ships an analogous headless
mode, `codex exec`, confirmed against the real installed binary's `--help` output
— but not yet run end-to-end against a real model response, because that needs an
interactive `codex login` (ChatGPT OAuth) no session has performed yet. Per
"Backend parity and cross-agent handoff" above, this is expected: whoever
implements/finishes this backend from a session with `codex` actually
authenticated should run the live tests `01-claude-cli.md` ran for the other
backend, and update `reference/02-codex-cli.md` accordingly. Until then, treat
everything below as "flags exist and take these forms, scaffolded from published
docs/`--help`", not "behavior verified".

### The invocation (flags confirmed to exist; not yet run against a live model)

```
codex exec \
  --json \
  --ephemeral \
  --sandbox read-only \
  --ignore-user-config \
  --ignore-rules \
  --skip-git-repo-check \
  --output-schema '<path to JSON schema file>' \
  '<user prompt>'
```

(or the prompt on stdin — `codex exec` reads from stdin when no positional
argument is given, same shape as `claude -p`.)

| Flag | Believed purpose (mirrors the `claude -p` table above) | Confidence |
|---|---|---|
| `--json` | JSONL event stream to stdout (`thread.started`, `turn.started`, `item.completed`, `turn.failed`, ...) — the streaming hook | Confirmed shape exists; event *contents* during a real generation not observed |
| `--output-schema <file>` | Structured output — schema is a **file path**, not inline JSON like `claude`'s `--json-schema`, so the backend must write a temp file per call | Confirmed as a flag; whether it streams partial fragments (needed for `StreamingStringField`) or only validates the final message is **unverified** |
| `--ephemeral` | No session persistence, matches `--no-session-persistence` | Confirmed flag |
| `--sandbox read-only` | Model-issued shell commands can't write; closest analog to denying tool side effects | Confirmed flag; unlike `--tools ""` this doesn't disable tools, just constrains what they can do |
| `--ignore-user-config`, `--ignore-rules` | Skip `~/.codex/config.toml` and project `.rules`/AGENTS.md-equivalent files | Confirmed flags exist; **not confirmed** these add up to the same isolation guarantee as `--safe-mode` (MCP servers, plugins etc. may still load) |
| `--skip-git-repo-check` | Needed since a generated Rust game has no reason to run inside a git repo Codex recognizes | Confirmed flag |

No flag was found that fully disables tool availability the way `claude`'s
`--tools ""` does — needs live testing to see what a `read-only` sandbox actually
lets the model attempt and how errors surface if it tries something the sandbox
blocks mid-turn.

### Auth: the same trap as `--bare`, verified

`codex login` (default) is ChatGPT-plan/subscription auth — what this project wants.
`codex login --with-api-key` explicitly switches to metered `OPENAI_API_KEY` billing —
the `--bare` trap, but opt-in here rather than a single flag on the exec call itself.
**Action item: `cyoa doctor` should check `codex login status` the way it checks
`claude`'s auth, and warn if API-key auth is active**, since nothing on the `codex exec`
invocation itself prevents it.

### Open questions before `CodexCliBackend` can be marked verified

1. **Does `--output-schema` stream the structured fields incrementally**, the way
   Claude's forced `StructuredOutput` tool call does via `input_json_delta`? If Codex
   only validates the final message, `narrative`-first + `StreamingStringField` may
   need a different feed path for this backend (e.g. treat plain `item.completed`
   agent-message deltas as the raw stream instead of tool-call JSON deltas).
2. Exact `item.*` event shapes during a real multi-turn generation (only
   `thread.started` / `turn.started` / `turn.failed` were observed here, from a request
   that failed at auth before producing content).
3. Whether `--ignore-user-config` + `--ignore-rules` is a complete isolation story, or
   whether MCP servers / plugins configured for the account still load during `exec`.
4. Exit code and `is_error`-equivalent conventions on failure (rate limit, sandbox
   violation, malformed schema) — needed for the same "never auto-retry, surface as a
   value" posture as the Claude backend.
5. Token/cost reporting equivalent to `total_cost_usd`, if any, for the "estimate, not
   billed" display.

The `Backend` trait impl, config wiring, and invocation-building for
`CodexCliBackend` can be **scaffolded now** from the confirmed `--help` flags
above — don't block that on auth. What should wait for a real authenticated run
(from a session with `codex` logged in) is **marking it verified**: resolve these
open questions, write `reference/02-codex-cli.md`'s "Confirmed" section from an
actual transcript the way `01-claude-cli.md` was, and only then consider the
backend equal-status-complete alongside `ClaudeCliBackend`.

### Backend trait shape

One method, mirroring `AIProvider.generate_structured_output`:

```rust
trait Backend {
    fn generate(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        schema: &serde_json::Value,
        on_text: &mut dyn FnMut(&str),   // raw JSON fragments, for the streaming scanner
    ) -> BackendResult;   // parsed value + raw text + usage/cost + error-as-value
}
```

Errors are returned, never panicked — same posture as `next_turn()`.

---

## Project layout

### Clean Architecture, DDD, and lightweight CQRS

Adopt the following logical layers. Preserve calibre's game behavior while using
DDD to express meaningful domain concepts and invariants, without introducing
repositories, factories, or aggregates merely by convention.

| Layer | Responsibility |
|---|---|
| Domain | Vendor-agnostic business logic: worlds, characters, turns, summaries, merge rules, validation, chapters, rewind, and game invariants. No terminal, CLI protocol, filesystem, or vendor concerns. |
| Application / use cases | Orchestrates world generation, starting a game, taking a turn, saving, and exporting through domain operations and inward-owned ports such as `StoryGenerator` and `GameRepository`. |
| Presentation | Driving interfaces: Clap commands, headless input/output, TUI screens, and any future HTTP API. Translates external input into application commands and query results into views. |
| Infrastructure | Driven adapters: Claude/Codex subprocesses, filesystem persistence, configuration loading, and export encoders. Implements ports required by the application/domain. |

Dependency direction is inward:

```text
Presentation   --> Application --> Domain
Infrastructure --> application/domain ports
main.rs wires concrete implementations into the application
```

The application may hold and invoke infrastructure implementations through traits
owned by the application (or domain where intrinsically appropriate). It must not
import concrete presentation or infrastructure types. Presentation-specific
callbacks and framework types do not cross this boundary; progress is exposed
through inward-owned contracts. The executable is the composition root and may
depend on all layers to wire them together.

Use lightweight CQRS: commands express intent and may change state (`TakeTurn`,
`Rewind`, `EditCharacter`); queries return read-only views (`CurrentStory`,
`AvailableActions`). Commands may return their outcome and errors. This does not
require separate databases, a message bus, or event sourcing. Keep the existing
turn log and derived state model. Failed generation must not commit partial state.

Application generation ports should speak in meaningful world/cast/turn types.
Raw JSON, CLI event envelopes, and vendor usage formats belong behind adapters;
normalize and validate external output before it can change domain state. The
current generic JSON backend interface is scaffolding for a lower-level adapter
boundary, not the intended domain-facing generation API.

Separate vendor and save DTOs from domain types when their constraints differ.
Tolerant wire deserialization must not weaken domain invariants. Do not duplicate
types when there is no meaningful distinction. Preserve the existing external
defaults, normalization rules, and migration behavior through explicit mapping.

These decisions supersede conflicting placement in the original module inventory
below (notably persistence, subprocess adapters, and JSON schemas) and the former
"one layer of types" rule. The original two-crate scaffold was the starting
point, not a requirement to fit all four layers into two crates. Exact module/crate
splits enforce these dependencies; this
decision records architecture. The scaffold has now been split as follows; game
logic is being ported incrementally; application use cases remain to be implemented.

### Current workspace layout

```text
cyoa-core/            domain types and business rules (characters/summary merge implemented)
cyoa-application/     orchestration and ports; cancellation and image port today
cyoa-infrastructure/ external adapters; JSON transport and disabled image adapter
cyoa-presentation/   terminal interface, depending inward on application
cyoa-cli/            executable composition root
```

`scripts/check_architecture.sh` (Bash + `jq`) checks workspace dependency direction in CI,
including dev/build dependencies, and rejects direct terminal/JSON dependencies
in domain and application. Infrastructure and presentation may depend on application
and domain, but not on each other. Only the composition root imports both. The old
generic JSON `Backend` lives in infrastructure; domain-facing generation ports and
CQRS use cases will be introduced alongside the game types, not fabricated before
their contracts are known.

The first domain slice now provides `CharacterId`, named text value objects,
`Character`, a nonempty `CharacterCast` with unique ids, `StorySummary`, and bounded
`MajorEvents`. `CharacterCast` stores an `IndexMap<CharacterId, Character>`:
iteration preserves insertion order and lookup uses stable ids. Construction
first deduplicates exact records (id, name, and every detail) with an ordered set,
keeping the first occurrence. It then repairs remaining duplicate ids with
sequential suffixes (`-2`, `-3`, ...), preserving every distinct record. It reserves
all supplied ids before assigning suffixes so repairs cannot
take an id supplied by a later entry. Subsequent prompts must use these repaired
ids. This normalization is an intentional extension to calibre's behavior, not a
claim that calibre rejected duplicate input ids. Equal names or ids alone do not
establish that two people are the same. These construction steps make no LLM calls.
The first generation after cast selection (the opening turn) must additionally
request a cast-identity check using `reference/prompt-additions.toml`; this adds
instructions to an existing call, not a separate deduplication call. Preserve
distinct namesakes and reuse established ids rather than introducing aliases as
new characters. The prompt addition is prepared but not yet wired: the prompt
renderer has not been implemented. It cannot guarantee semantic deduplication or
delete/consolidate existing cast entries with the current SummaryUpdate schema.
An actual semantic consolidation operation would need an explicit future schema
and domain change; do not silently treat prose or name similarity as a merge command.
Cast equality includes order, unlike ordinary map equality. The merge uses this
map directly instead of rebuilding an id index for each turn.
Constructors establish stored-state invariants. Proposed deltas
remain permissive: absent ids/names/details may be ignored when they cannot
introduce a useful character. All character delta fields use `Option<NonblankType>`:
`None` means unchanged, and `Some` supplies a nonblank value. Boundary DTOs map blank
wire strings to `None`. Stored `CharacterDetails` has private fields and a checked
constructor requiring description or backstory; `Character::new` therefore returns
`Self` without rechecking that invariant. `updated()` returns a new valid value
without changing the original. It is infallible because a delta cannot erase
required fields or the existing cast. Required world/situation validation happens
when entering the domain, before merging.

`UpcomingEventsUpdate::{Keep, Replace}` expresses nullable wire-list semantics
inside the domain; future DTO mapping must preserve omitted/null versus empty.
Optional world/name/id fields likewise represent normalized blank wire values;
this does not change the schemas sent to the model. `MajorEventLimit` is positive
and defaults to 30; the bounded event collection carries its limit through merges.
Matching follows the previously accepted Unicode-lowercase approximation to
Python casefold. No JSON or serde dependency has been added to the domain.

### Original workspace inventory (subject to the layer boundaries above)

**Cargo workspace, two crates.** The wall between engine and I/O is enforced by the
dependency graph, not by discipline (calibre's equivalent rule — "cyoa.py must not import
Qt" — is only a comment).

```
cyoa/
├── cyoa-core/     lib: engine. No ratatui, no crossterm, no std::process.
└── cyoa-cli/      bin: TUI + backends + config
```

Add a CI check so the wall is machine-checked:
`cargo tree -p cyoa-core | grep -q ratatui && exit 1`.

### cyoa-core

| Module | Responsibility |
|---|---|
| `ids.rs` | `CharacterId` newtype, `character_id_for_name`, `unique_character_id` |
| `types/` | `world.rs`, `turn.rs`, `summary.rs`, `state.rs` — plain serde structs |
| `state.rs` | `GameState` impl: `prose_context`, `current_summary`, `chapter_titles`, `rewind`, `apply_character_edits`, `start_game` |
| `merge.rs` | `updated_summary`, `updated_characters`, `clean_text_list` — **the crown jewels** |
| `validate.rs` | `validated_world/_cast/_npcs/_turn`, `selected_quick_actions` |
| `limits.rs` | `Limits { max_major_events, max_generated_npcs, min_prose_context_turns, … }` |
| `prompts/` | TOML loading, minijinja rendering, `defaults/*.toml` via `include_str!` |
| `styles.rs` | Pace/Tone/Narration/ArtStyle tables, `style_for_key` |
| `schema.rs` | `schemars` → JSON Schema + description injection from TOML |
| `backend.rs` | `Backend` trait, `GenRequest`/`GenResponse`, `CancelToken`, `BackendError` |
| `stream.rs` | `StreamingStringField` port |
| `engine.rs` | `generate_world`, `generate_cast`, `next_turn` — return errors, never panic, never retry |
| `persist.rs` | Save envelope, migrations, atomic writes |
| `export/` | `markdown.rs`, `epub.rs` |
| `image.rs` | `ImageBackend` trait + `NullImageBackend` stub |

### cyoa-cli

`main.rs` (clap: `play`/`new`/`list`/`export`/`doctor`), `config.rs` (XDG), `logging.rs`
(tracing to file — a TUI cannot log to stdout), `backends/{claude_cli,codex_cli,replay}.rs`,
`worker.rs`, `tui/{app,event,markdown,theme,screens/*,widgets/*}.rs`.

## Type design

### Rust domain modeling requirements

Preserve the Python engine's behavior, while expressing the implementation in
idiomatic Rust rather than translating Python's structure mechanically. Use the
type system to make invalid states unrepresentable wherever practical; this is a
general architectural requirement, not just a request for runtime validation.

- Prefer domain-specific types over bare primitives in domain interfaces and
  stored state. Distinct concepts such as character ids, chapter indices, and
  token counts should have named types rather than interchangeable strings or
  integers. Ordinary primitives remain appropriate inside these types and for
  incidental implementation details.
- Semantic wrappers or aliases are acceptable even without extra validation.
  An alias documents intent but does not establish a distinct Rust type: use a
  newtype when accidentally mixing two concepts should be a compile error.
- Where values have invariants, use private fields and checked constructors.
  Expose accessors and operations that preserve those invariants; do not provide
  mutation or deserialization paths that bypass them.
- Represent mutually exclusive states with enums whose variants carry exactly
  the data each state needs. Avoid independent flags and optional fields that
  permit contradictory combinations.
- Use marker types / typestate where legal transitions can usefully be enforced
  at compile time. Use runtime enums for state machines driven by runtime events,
  such as the TUI; typestate is an available technique, not a requirement to
  parameterize every state machine.
- Derive redundant data from a single source where possible. When retaining both
  raw and parsed data, construct them together and prevent independent mutation.
- Validate untrusted input at boundaries. Static guarantees begin after that
  validation; a typed backend response alone does not establish schema or game
  validity. Preserve raw diagnostics on failure and the engine's error-as-value,
  no-automatic-retry, state-untouched-on-error behavior.
- Test important runtime invariants and use compile-fail examples where useful
  to demonstrate operations the public API deliberately forbids.

These requirements supersede the earlier blanket "Everything else stays
`String`" recommendation. They do not call for mechanically duplicated types,
a generic patch framework, or stricter rejection of model output than calibre's
behavior requires. Keep external representations serde-compatible, preserve defaults and
the `None` versus empty-list distinction, and normalize recoverable model output
as specified below.

**Domain types and boundary DTOs have distinct responsibilities.** Use separate
vendor/save DTOs where necessary to preserve tolerant wire behavior and strong
domain invariants. Share types only where their constraints match. Normalization
(trim/dedup/cap) and rejection retain calibre's semantics; mapping into domain
types must pass through invariant-preserving constructors or operations.

**Rule for serde at external boundaries:** every Python field with a default becomes `#[serde(default)]`; every
field without one stays required. That reproduces `instantiate()` exactly and is what lets
you add fields later without writing a migration.

Keep the four style keys as **flat fields in the saved game representation**, not a nested struct — calibre's
stated reason (`cyoa.py:455`): adding a fifth style then needs no migration.

### The three places Rust needs care

**1. `CharacterId` newtype.** The one thing genuinely easy to confuse with a name, which is
the entire point of `cyoa.py:136`.

```rust
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CharacterId(String);
```

Use semantic types for other domain concepts as appropriate under the requirements
above; retain strings as their underlying representation where suitable.

**2. `Option<Vec<T>>` for `upcoming_events` only.**

```rust
#[serde(default)]
pub upcoming_events: Option<Vec<String>>,  // None = unchanged, Some([]) = all threads resolved
```

Every other list is `#[serde(default)] Vec<T>`. **Resist making them all `Option` "for
symmetry" — the asymmetry is the design**, and flattening it is exactly how the
replace-vs-merge distinction gets lost in a port. Guard it with a test named
`upcoming_events_null_is_not_empty`.

**3. "Empty means unchanged" gets exactly one helper**, not a scattered idiom:

```rust
/// The Python `new.strip() or old` idiom: a blank delta field keeps the old value.
fn merged(new: &str, old: &str) -> String {
    let t = new.trim();
    if t.is_empty() { old.to_string() } else { t.to_string() }
}
```

Do **not** model this as `Patch<T> { Keep, Set(T) }` with a custom deserializer. It looks
more type-safe but loses the raw text for a "view raw response" pane, needs a custom
deserializer on five fields, and makes the emitted schema diverge from `string`, which is
what the model must actually produce.

### `QuickActionKind` — unknown variants must not fail a turn

Serde's `#[serde(other)]` does **not** work on plain externally-tagged unit-variant enums.
Write a manual `Deserialize` mapping anything unrecognised to `Other`. This is load-bearing:
it turns a hallucinated `"aggressive"` into a cosmetic issue rather than a discarded passage
of prose the player already waited for.

### Python→Rust mismatches worth flagging

| Python | Rust | Note |
|---|---|---|
| `dict` insertion order | `HashMap` | **`of_kind` in `selected_quick_actions` relies on insertion order** (`list(of_kind.values())[:3]`). Use `indexmap::IndexMap`. Silent, non-deterministic bug otherwise — the single most likely porting mistake. |
| `casefold()` | `to_lowercase()` | Not identical (ß→ss), immaterial for names. Don't add `unicase`. |
| `events[-30:]` | `v.drain(..v.len().saturating_sub(n))` | Watch the underflow. |
| `del turns[-n:]` | `truncate(len - n)` | Guard `0 < n <= len` first, return `Err`. |
| tuples | `Vec<T>` | Immutability was a Python nicety; `&`/`&mut` gives it for free. Not `Box<[T]>`. |
| `_replace()` | direct field mutation | The Rust will be *shorter* here; don't emulate functional-update style. |

### `prose_context` is zero-copy

Chapter numbers are non-decreasing along `turns`, so **both halves are contiguous slices**:

```rust
/// (bridge, current) — both contiguous because chapter numbers are non-decreasing.
pub fn prose_context(&self) -> (&[TurnRecord], &[TurnRecord]) {
    let n = self.turns.len();
    if n == 0 { return (&[], &[]); }
    let chapter = self.turns[n - 1].chapter;
    let current_start = self.turns.partition_point(|t| t.chapter < chapter);
    let window_start = n.saturating_sub(self.limits.min_prose_context_turns);
    (&self.turns[window_start.min(current_start)..current_start], &self.turns[current_start..])
}
```

`debug_assert!` that chapters are sorted — `partition_point` depends on it and a bad
migration could break it silently.

## Schema generation

**Use `--json-schema` with native structured output. Do not port calibre's TypeScript
interface renderer** (`structured.py:243`) — that exists only because calibre must support
providers with no structured-output mode. The verified backend has one, so that entire
subsystem is unnecessary.

Split ownership:

- **Structure** (field names, types, order, nesting) — owned by the Rust structs via
  `#[derive(JsonSchema)]`. It cannot live in TOML because it must match what serde will
  deserialize; divergence is a runtime parse failure.
- **Descriptions** (calibre's `Annotated[str, '...']` prose) — owned by
  `schema_docs.toml`, keyed by dotted field path. These are prompt text you will tune at
  2am and they have zero coupling to the deserializer.

At startup, walk the schemars `Schema` and inject descriptions from the map.

**The anti-drift device is a test, and it is the most valuable test in the repo:**

```rust
#[test]
fn schema_docs_cover_every_field_and_no_others() {
    // assert_eq! rust field paths == toml doc keys, both directions
}
```

Rename a field → fails. Add one → fails until documented. Stale key → fails. This is
strictly better than calibre, where a description can rot into inaccuracy unnoticed.

**Property order is load-bearing.** `serde_json::Map` is a `BTreeMap` (alphabetical) unless
you enable `serde_json/preserve_order`. `narrative` must be the first property or streaming
is pointless — alphabetically it would be third. Enable the feature and add a test asserting
`StoryTurn`'s first property is `narrative`.

## Streaming

**Hand-roll the scanner; port `StreamingStringField` near line-for-line.** No crate fits:
`struson` and `json-event-parser` are pull parsers over a `Read` (they block, inverting the
control flow), and `serde_json::StreamDeserializer` handles concatenated documents, not
partial ones. The Python is ~120 well-commented lines; the Rust is ~160.

Feed it the `partial_json` fragments from `input_json_delta` (verified above).

Three Rust-specific hazards:

1. **UTF-8 chunk boundaries.** Fix this *below* the scanner: the subprocess reader keeps a
   tail buffer and emits only valid `&str` (`Utf8Error::valid_up_to()`). Since the CLI
   output is newline-delimited JSON, `BufRead::read_line` makes the problem disappear
   entirely. Keep `feed(&str)` so the scanner never sees bytes.
2. **Lone surrogates.** Rust `char` cannot hold one. Port `pending_high_surrogate` as
   `Option<u16>`; combine on a following low surrogate, else flush `U+FFFD`. Flush any
   pending surrogate when the string ends.
3. **Iterate `chars()`, not bytes.**

Test with `proptest`: for arbitrary `StoryTurn` JSON and arbitrary chunk splits,
per-chunk output must equal whole-input output must equal the real `narrative`. Plus escape
torture (`😀`, lone halves, `\u` truncated at end of stream), leading code fences,
field-not-first, and object-ends-without-field.

## TUI architecture

`ratatui` + `crossterm`, single render thread, message passing for everything else.

```rust
enum Screen {
    Saves, Brief { .. }, WorldGen { partial: String }, WorldEdit { .. },
    CastGen { .. }, CastSelect { .. }, Play(PlayState), Fatal { .. },
}
enum TurnPhase {
    Idle,
    Generating { partial: String, started: Instant, cancel: CancelToken, input_echo: String },
    Failed { error: String, raw: String, input_echo: String },
}
```

Style selection lives in a **modal overlay** reachable from `Brief` and `Play`, not as its
own lifecycle step — one fewer gate between "I typed an idea" and "I am reading prose".

### Threading model — this is the important part

- **Input thread**: blocking `crossterm::event::read()` → channel. Lives forever.
- **Worker thread**: spawned per LLM call. Gets an **owned clone** of what it needs (or a
  pre-rendered `TurnRequest` built on the UI thread). Cloning `GameState` costs microseconds
  against a 30-second call.
- **The UI thread keeps sole ownership of canonical state and commits the returned turn when
  `Event::Generated` arrives.** No `Arc<Mutex<GameState>>`, no lock held across a render.
  This mirrors calibre's "state untouched on error" guarantee and removes an entire class of
  concurrency bug.
- `on_text` runs on the worker thread and does exactly one thing: `tx.send(Event::Chunk(..))`.
  It must never touch the terminal or `App`.
- **Cancellation is cooperative + forceful**: the backend registers its `Child` so flipping
  the token both stops the read loop and kills the process. Without this, Esc during a
  60-second generation leaves an orphan `claude`.

### Narrative pane — do not use `Paragraph` naively

`Paragraph::new(all_prose).wrap(..)` re-wraps the whole story every frame; by chapter 8
that is hundreds of KB per frame *during streaming*. Maintain a wrapped-line cache:

```rust
struct NarrativeCache {
    width: u16,
    lines: Vec<Line<'static>>,   // committed turns, wrapped and styled once
    committed_turns: usize,
    tail: Vec<Line<'static>>,    // in-flight partial, re-wrapped per chunk (~800 words max)
}
```

Render `&lines[offset..offset+height]` directly. Rebuild on resize. **The line cache is not
optional.**

Also: drain and concatenate pending chunks before redrawing — chunks arrive faster than
60fps and you do not want to re-wrap per token. Use `unicode-width` for wrapping, not
`.len()`.

Scroll: sticky-bottom, auto-follow while streaming unless the user scrolled up; restore on
`End`. Markdown styling is inline-only (`**bold**`, `*italics*`, paragraph breaks) because
the prompt forbids headers and lists — ~60 lines producing `Vec<Span>`, no markdown crate in
the render path.

### Keys

`1`/`2`/`3` pick quick actions **only when the input box is empty** (otherwise digits type).
A quick action submits its text through the identical code path as typed input.
`Ctrl-S` save, `Ctrl-Z` rewind (confirm), `Ctrl-G` something-interesting-happens, `Ctrl-E`
export, `Ctrl-P` settings, `Esc` cancel generation, `Ctrl-C` quit (autosave first).

Keep `fn update(app: &mut App, ev: Event) -> Redraw` pure with respect to the terminal, so
the whole state machine is unit-testable with synthetic events.

## TOML prompt externalization

Defaults embedded via `include_str!`; user overrides at
`$XDG_CONFIG_HOME/cyoa/{prompts,styles,schema_docs}.toml`.

**Merge per-key, not per-file.** Parse both to `toml::Value`, deep-merge with user winning
at leaf level. Whole-file override means a user who tweaked one sentence never receives any
later prompt improvement — and prompts are precisely what you will keep improving.

**Templating: `minijinja`**, not `format!` placeholders. The turn prompt has real control
flow (bridge present, first turn vs later, interesting-event, threads list, player input
present). With plain substitution you would assemble those branches in Rust and pass
pre-joined strings, putting half the prompt back in the source and defeating the purpose.

Turn on `UndefinedBehavior::Strict` so a typo'd variable is an error, not a silent empty
string, and render every template against a synthetic context **both in a test and in a
startup self-check** — a broken override should fail at launch, not mutilate the prompt on
turn 40.

### `[limits]` is the one place this makes the system *more correct*

In calibre `MAX_MAJOR_EVENTS = 30` lives in the enforcement code *and* is interpolated into
three prompt strings. Here `[limits]` is a single source: the `Limits` struct is passed into
the engine **and** injected into every template context, so they cannot disagree. Document
clearly that editing `[limits]` changes engine behaviour, not just text.

### ⚠️ Strip JSON/formatting directives from the ported prompts

Because we use `--json-schema`, there must be **no** "respond with only valid JSON" fragment
(that caused the double-encoding in testing). Describe fields semantically only. Keep
`markdown_instructions()` — it governs the prose *inside* `narrative`, which is different.

Do add one line: *"Emit the fields of the JSON object in the order they appear in the
schema."* Calibre gets ordering free from Python field order; here it should be asked for.

## Persistence

- `directories::ProjectDirs` → saves in `$XDG_DATA_HOME/cyoa/saves/<slug>.json`, with
  `<slug>.assets/` reserved for images.
- Envelope carries `title`/`saved_at`/`turn_count` **above** `game`, so the load screen
  needs no separate index file.
- **Migrations from day one**, even at version 1: read → `serde_json::Value` → migrate chain
  → `from_value`. Costs nothing now, agony to retrofit. Keep frozen fixtures per version and
  a test that each still loads. Reject `version > CURRENT` with a clear message.
- **Atomic writes** via `tempfile::NamedTempFile::persist`, keeping one `.bak`. A corrupt
  save after 200 turns of a story someone enjoyed is the worst possible failure mode and one
  backup makes it survivable.
- Autosave after every committed turn and on quit. No save prompts.
- **Do not store prompts** in `TurnRecord` (quadratic growth — calibre's
  `STORE_PROMPTS_IN_TURN_RECORDS = False`); keep it as a debug config flag. **Do** store
  `raw_response` — it is what makes "the model returned garbage, show me" possible.

## Export

**Markdown** — straight string building, no crate. Title, brief, prologue (world
description), dramatis personae, then per chapter: heading, and per turn the player input as
a blockquote followed by the narrative.

**EPUB** — `epub-builder` + `pulldown-cmark`. Port the structure of
`gui2/cyoa/epub.py`: `prologue.xhtml`, `dramatis-personae.xhtml`, one
`chapter-NNN.xhtml` per chapter, and `EPUB_CSS` (`epub.py:71`) copied verbatim — it already
handles the floating-portrait layout you will want when images land. Metadata: title from
`world.title`, author from `$USER` (as `epub.py:199` does), tag `CYOA`.

In both: the cast comes from `current_summary().characters` (protagonist first), **not** from
`world` — the AI introduces characters as the story goes. That is `cast_of()` at
`epub.py:307`. Skip the generated cover in v1 (calibre's uses its own cover machinery).

## Crates

`ratatui` + `crossterm`; `tui-textarea` (multi-line editing — writing this yourself is a week
you don't need to spend); `serde`/`serde_json` **with `preserve_order`**; `schemars`;
`toml`; `minijinja`; `thiserror` in core and `anyhow` in the bin; `std::process`;
`which`; `directories`; `tempfile`; `epub-builder`; `pulldown-cmark`; `unicode-width`;
**`indexmap`**; `clap`; `tracing` + file appender; `insta`/`proptest`/`rstest` for tests.

Explicitly **not**: `tokio`, `async-trait`, `reqwest` (v1 makes no HTTP calls), `unicase`.
There is exactly one LLM call in flight at a time and the work is "spawn a process, read
stdout" — async buys nothing and forces the TUI into a runtime bridge.

## Testing

**Port calibre's test suite first** — `cyoa.py:1569 find_tests()` already has `make_world`,
`make_update`, `make_turn`, `make_cast` and a `FakePlugin`, covering exactly the logic most
likely to be mis-ported. Translating those before writing the implementation gives you a
runnable specification.

Must-have cases:

- **Merge**: match by id; fall back to casefolded name; blank keeps old; second update for
  the same character dropped; new character requires name + (description or backstory);
  duplicate invented id uniquified; rename updates the `by_name` map mid-loop.
- **Events**: consolidated replaces, new appends, dedup casefolded, cap keeps the *last* 30.
- **`upcoming_events`**: `None` keeps, `Some([])` clears, `Some([..])` replaces.
- **`prose_context`**: 0 turns, all one chapter, boundary at every position with 1–6 turns —
  as a **differential test** against a naive transcription of the Python filter, which
  cheaply validates the `partition_point` assumption.
- **Chapter**: first turn with `starts_new_chapter: true` must stay chapter 0.
- **`selected_quick_actions`**: casefold dedup, one-per-kind preference, original order
  preserved, all-blank → empty.

Plus: `ScriptedBackend` and a `ChunkedBackend` that replays responses in seeded pseudo-random
chunk sizes to exercise streaming deterministically; recorded fixtures via a
`CYOA_RECORD_DIR` env var on the real backend, curated into an end-to-end test (world → cast
→ 4 turns including a chapter break, a malformed response, an unknown action kind, a
duplicate id); `insta` snapshots of the assembled system prompts and turn prompts at three
game states — **these are what make TOML prompt externalization safe, because you see the
prompt diff in review**.

Ship `cyoa play --demo` on the replay backend: TUI development without burning quota or
waiting 30 seconds per iteration.

## Build order

**Phase 0 — engine, no I/O.** Types + serde + `Limits` + prompts TOML + minijinja + schema
generation + `StreamingStringField` + merge/validate + `engine::{generate_world,
generate_cast, next_turn}` + `ScriptedBackend`. Ported calibre tests pass. Nothing playable,
but every hard algorithm is done and proven.

**Phase 1 — walking skeleton: headless, real model.** Whichever CLI backend the
implementing session can actually authenticate to (`ClaudeCliBackend` or
`CodexCliBackend` — see "Backend parity and cross-agent handoff") + `cyoa play
--headless`: print prose to stdout, read a line from stdin, loop, streaming as it
arrives. **This is the ship-quality checkpoint** — if the prompts produce bad
fiction or the JSON comes back malformed, you find out here, cheaply, before any
TUI exists. Calibre has exactly this shape in `develop()` (`cyoa.py:1514`). Keep
`--headless` forever; it is the best debugging tool in the project. Whichever
backend goes through this gate first, treat the walking skeleton as unfinished
until the *other* backend has also cleared it in a later session — v1 doesn't
ship on only one verified backend.

**Phase 2 — persistence.** Save/load/list, atomic writes, migration scaffold, autosave,
rewind. Headless gains `/save`, `/load`, `/rewind`, `/quit`.

**Phase 3 — TUI, play screen only.** Boot straight into an existing save. Narrative pane +
line cache + scroll, input, quick actions, streaming, spinner, cancel, error/retry. Biggest
single chunk — doing it against a save file and `--demo` means instant iteration with no LLM
latency.

**Phase 4 — TUI lifecycle.** Saves → brief → world gen (streaming) → world edit → cast gen →
character select → play.

**Phase 5 — export.** Markdown (an hour), then EPUB.

**Phase 6 — polish.** Style overlay, character editing, token display, interesting-event,
`config init`/`doctor`, help, calibre save import.

**Phase 7 — image seams.** Wire `ImageBackend` + asset directory + portrait/scene prompt
templates, all disabled. (Define the trait in Phase 0; this is just plumbing.)

## Risks, ranked

1. **The delta merge.** ~90 lines encoding a dozen subtle decisions. A subtle mis-port
   produces a game that works fine for ten turns and then quietly forks a character in two.
   Write the tests first; consider the differential test against a literal transcription.
2. **Narrative pane performance.** The naive approach is visibly janky by chapter 5 during
   streaming.
3. **Field ordering.** `narrative`-first is a convention the model may violate. Mitigated by
   `preserve_order`, the explicit prompt line, and a test — but design the TUI so a failure
   degrades to "spinner, then the whole passage appears" rather than breaking.
4. **Prompt drift through TOML.** You have deliberately made prompts editable, so they can be
   broken. Strict-undefined + startup self-check + snapshots are the three guards.
5. **`selected_quick_actions` ordering** — `HashMap` vs ordered `dict`. Silent and
   non-deterministic. `IndexMap`.
6. **Unicode edges** — surrogates, UTF-8 chunk boundaries, CJK width. Each is a one-line fix
   and a two-hour debugging session if missed.
7. **Rate limits.** Subscription limits surface as exit code 1 mid-story. Surface it as a
   clear, resumable error — the autosave means nothing is lost.
8. **`CodexCliBackend` streaming shape is unverified.** If `--output-schema` turns out not
   to stream partial fields the way Claude's forced tool call does, the `Backend` trait's
   `on_text` contract may need a second variant (whole-field vs. incremental), or the Codex
   backend simply doesn't support streaming for v1 and falls back to spinner-then-reveal.
   Resolve this with a live `codex login` + test call before writing `CodexCliBackend`,
   not while writing it.

## Deliverable for this session: a self-contained seed repo

The implementation happens in a later session on another machine, so this session's
output is a **seed directory that can be `git init`'d, pushed to a personal GitHub repo,
and checked out anywhere — with no dependency on having calibre cloned.**

Proposed location: **`~/code/cyoa-rs/`** (alongside your other projects; say the word if
you want it elsewhere).

```
~/code/cyoa-rs/
├── README.md                    how to resume: prerequisites, what's here, where to start
├── PLAN.md                      this plan, verbatim
├── LICENSE                      GPL-3.0 (see below)
├── NOTICE.md                    attribution and provenance
├── .gitignore                   Rust
└── reference/
    ├── 00-engine-notes.md       merge rules, prose_context, validation, lifecycle,
    │                            constants — everything needed to port without the source
    ├── 01-claude-cli.md         verified flags, the --bare trap, a captured stream sample
    ├── prompts.toml             every prompt string, verbatim, already in target format
    ├── styles.toml              pace / tone / narration / art-style tables, verbatim
    ├── schema_docs.toml         every schema field description, verbatim
    ├── premade-worlds.toml      the starter world briefs
    └── calibre/                 TEMPORARY: copies of the Python sources, delete when done
        ├── cyoa.py              the engine being ported
        ├── structured.py        StreamingStringField, instantiate, strip_code_fences
        └── epub.py              EPUB structure and CSS to reproduce
```

`reference/calibre/` is just **copies of three Python files, kept as scaffolding and
deleted once the port is finished.** They are what actually delivers "resume without
checking out calibre": the notes are a summary, but mid-implementation you will want to
check the original for an edge case, and `epub.py`'s CSS is meant to be copied across
as-is. Their license headers stay intact while they are there.

Add a line to `README.md` saying they are temporary, so a future you (or anyone else)
knows they are reference material rather than part of the build.

### ⚠️ Licensing — this decides your repo's license

`src/calibre/ai/cyoa.py` header: **`License: GPLv3 Copyright: 2026, Kovid Goyal`**, and
calibre ships GPL-3.0 in `LICENSE`.

A faithful port that reproduces the prompt strings, schema descriptions and algorithms
verbatim is a **derivative work** — and deleting the copied Python files later does not
change that, because the derivative part is the ported prompts and logic, not the copies.
So:

- The repo must be **GPL-3.0**, and must stay GPL-3.0 if published.
- `NOTICE.md` should state plainly that it is a Rust port of calibre's CYOA feature,
  © Kovid Goyal, GPL-3.0, with a link upstream.
- This is fine for a personal project and for publishing on GitHub. It only becomes a
  constraint if you later want to make it closed-source or permissively licensed — which
  would mean rewriting the prompts and algorithms from scratch, not just reorganizing them.

Worth deciding now rather than after the repo is public. I'll set it up as GPL-3.0 unless
you say otherwise.

Note this session will **not** `git init`, commit, or create anything on GitHub — the
directory is left ready for you to do that on your own machine.

## Verification

- `cargo test -p cyoa-core` — ported calibre suite, merge/prose-context differential tests,
  proptest on the stream scanner, schema-docs sync test, save round-trip and migrations.
- `cargo test -p cyoa-cli` — `update()` reducer tests, `TestBackend` frame snapshots.
- `cyoa doctor` — checks **both** backends: locates the `claude` binary, runs one tiny
  `--json-schema` call, reports model and auth state; locates `codex`, checks
  `codex login status`, and warns if API-key auth is active (see the codex-backend
  section above). A missing/unauthenticated CLI for one backend is reported, not
  fatal — `cyoa` should run fine on whichever backend is actually configured and
  available.
- **Phase 1 manual gate:** `cyoa play --headless`, enter a brief, confirm a coherent world,
  a usable cast, then play ~6 turns through at least one chapter break. Verify: prose streams
  incrementally; quick actions differ in kind; a renamed character does not fork (inspect the
  saved summary); `upcoming_events` persists across turns that send `null`.
- `cyoa play --demo` — full TUI run on recorded fixtures, no network.
- Export a finished game to Markdown and EPUB; validate the EPUB with
  `epubcheck` if available, and open it in a reader.
- Re-run a saved game after adding a new optional field to confirm the
  `#[serde(default)]` forward-compatibility story actually holds.
