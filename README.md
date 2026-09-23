# cyoa-rs

A standalone Rust TUI port of calibre's "Create your own adventure" (CYOA)
game — an LLM-driven choose-your-own-adventure engine — driven by a CLI coding
agent's headless mode as the inference backend, so generation rides an existing
subscription instead of metered API credits. **Two backends are co-equal, not
primary/fallback:** `claude -p` (Claude Code headless mode) and `codex exec`
(OpenAI Codex CLI headless mode). See PLAN.md's "Backend parity and cross-agent
handoff" section — this project is expected to be implemented across sessions
using different coding agents on different machines, each with its own CLI
already authenticated.

This repo contains the Rust workspace scaffold, design research, and ported
prompt/schema reference content. No calibre checkout is needed.

## Status

The workspace and first domain slice are in place; gameplay is not implemented yet.

- `cyoa-core`: typed characters and summary memory, validated constructors,
  stable ids, and invariant-preserving character/event delta merging.
- `cyoa-application`: use-case layer with cancellation and the image port.
  Commands, queries, and story-generation ports will arrive with game types.
- `cyoa-infrastructure`: low-level JSON backend contract and disabled image adapter.
- `cyoa-presentation`: terminal interface, currently help and version output.
- `cyoa-cli`: the `cyoa` executable and composition root.
- `reference/`: the Python source and extracted material used to guide the port.

The port preserves Python's game behavior while using Rust ownership, borrowed
inputs, enums, and `Result` errors. Backend calls are synchronous and take exclusive
access to the adapter; the frontend will own canonical game state and commit only
validated responses. There are no implemented inference adapters yet, and their
live verification status remains recorded in the individual reference files.

Successful backend responses are constructed by parsing their original JSON;
callers cannot mutate the parsed value and original text independently. Input-token
counts enforce that cached tokens are a subset of total input. Cancellation sources
stay with the caller while workers receive observation-only tokens. The image seam
currently represents only the disabled outcome. JSON syntax validation is separate
from schema and turn validation, which will arrive with the remaining engine implementation.

The domain merge is covered by ported calibre scenarios and additional regression
tests: renames and same-turn aliases, id/name precedence, duplicate updates,
incomplete introductions, id collisions, event consolidation and capping, and
keeping versus clearing upcoming events. Stored domain objects expose read-only
access; incomplete update proposals are distinct from valid stored characters.
`CharacterDetails` checks for a description or backstory at construction, making
`Character::new` infallible. Every character delta field uses `Option<NonblankType>`:
`None` keeps the stored value and `Some` replaces it; blank replacements cannot be
constructed. Stored optional details use `None` for information not yet known.
Cast construction removes exact duplicate records first, then suffixes remaining
id collisions while preserving distinct namesakes. This requires no LLM call.
`reference/prompt-additions.toml` prepares an identity-check instruction for the
opening turn; integration awaits the prompt renderer. It does not enable semantic
merging or deletion of already stored cast members.
There is no serde or JSON dependency in the domain. Future boundary DTOs will map
blank fields and nullable lists into these domain changes. As permitted by the
plan, matching currently uses Unicode lowercase rather than full Python casefold
(so, for example, `ß` and `ss` are not equivalent).

A second slice adds `limits`, `world`, `style`, `turn`, and `game` (`GameState`).
Every tunable bound (max major events, max generated NPCs, the prose bridge
window, minimum playable characters) is its own type rather than a bare `usize`,
gathered into `Limits`, which `GameState` owns and never persists. World-cast
validation ports calibre's playable/NPC cleaning and capping rules. The
[domain review](reviews/2026-09-23-domain-slice/README.md) tracks open corrections
for binding protagonist selection to its world, exact audit
text, and consistent restored event limits. Exact-record deduplication preserves
distinct namesakes through world creation and summary ID assignment, deliberately
departing from calibre's name-based filtering. The zero-NPC-cap defect is fixed.
A turn's chapter proposal is a single
`ChapterMarker::{Continue, NewChapter}` carrying an optional title, and chapter
membership is derived from the marker sequence rather than stored — there is no
chapter index to fall out of sync with the turn log. This also deliberately
extends calibre: a chapter's title is the *last* title any of its turns supplied,
not only the first, so any turn can retitle its chapter. `GameState::commit_turn`
is infallible (every check calibre's `validated_turn` performed is already carried
by `StoryTurn`'s field types), and `rewind`/derived views (`current_summary`,
`chapters`, `prose_context`) are covered by ported and differential tests. Style
and character-edit propagation (`apply_character_edits`) are not yet ported.

Build and check with a stable Rust toolchain supporting edition 2024:

```sh
cargo run -p cyoa-cli -- --help
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bash scripts/check_architecture.sh
```

The architecture check uses Bash and `jq`; no Python tooling is required.

`Cargo.lock` is tracked because this workspace ships an application. CI also checks
inward workspace dependencies, including dev/build dependencies, and rejects direct
terminal/JSON dependencies in domain and application. The application imports only
domain types and owns its ports; infrastructure implements them; presentation calls
use cases. Only the composition root can depend on all layers. Remaining Phase 0
work includes world/turn state, validation and derived-state tests, application use
cases, prompts, schemas, streaming, and a scripted generation adapter.

## Where to start

1. Read `PLAN.md` in full. It has the architecture, the reasoning behind
   every non-obvious decision, the build order, and a ranked list of the
   risky parts. Follow the phased build order in there — Phase 0 (engine, no
   I/O) through Phase 1 (a real, playable headless loop against the actual
   model) is the fastest way to find out if anything about the design is
   wrong, before any TUI code exists.
2. `reference/00-engine-notes.md` — the game logic (schemas, the delta-merge
   rules, validation, chapter/rewind logic) in dense reference form.
3. `reference/01-claude-cli.md` and `reference/02-codex-cli.md` — the exact
   flags for each backend, what's actually been verified live vs. scaffolded
   from published docs, and each vendor's own version of the "looks correct,
   silently breaks the subscription billing" trap (`--bare` for Claude). Read
   whichever one matches the CLI you actually have authenticated, and if you
   can finish verifying the *other* one this session, do — see PLAN.md's
   "Backend parity and cross-agent handoff".
4. `reference/prompts.toml`, `reference/styles.toml`, `reference/schema_docs.toml`
   — every prompt string, style-table entry, and schema field description,
   copied verbatim from calibre. **Read the warning comment at the top of
   `prompts.toml` before using it** — it's the honest source text, not a
   ready-to-load config; it still contains JSON-formatting instructions from
   calibre's provider-agnostic prompt path that must be stripped for
   `--json-schema` use (see `01-claude-cli.md`'s "Gotcha" section for why).
5. `reference/calibre/` — **temporary** copies of the three calibre Python
   files this is ported from (`cyoa.py`, `structured.py`, `epub.py`). Kept
   around so you can check an edge case against the original without cloning
   calibre. Delete this directory once the port is complete and no longer
   needed for reference — the licensing position (see below) does not change
   when you do.

## Prerequisites

- Rust (stable toolchain)
- **At least one** of the two backend CLIs, installed and authenticated to its
  subscription (not an API key — see the `--bare`/`--with-api-key` traps in the
  reference docs):
  - `claude` (Claude Code) — verify with `claude --version` and a manual
    `claude -p` call
  - `codex` (OpenAI Codex CLI) — verify with `codex --version` and
    `codex login status`
- No calibre installation or checkout required

## License

**GPL-3.0** — see `LICENSE` and `NOTICE.md`. This is a derivative of GPLv3
code (calibre's CYOA feature, © Kovid Goyal), not a choice made freely for
this project; see `NOTICE.md` for why, and PLAN.md's "Licensing" section if
you're ever tempted to relicense it.
