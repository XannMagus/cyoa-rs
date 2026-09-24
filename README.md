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

The Phase 0 engine works through scripted generation; playable CLI/TUI flows and
real inference adapters are still pending.

- `cyoa-core`: checked domain types, namesake-preserving casts and stable IDs,
  summary deltas, typed limits, owned protagonist selection, turns, chapters and
  rewind. Current/original restore policies preserve original settings and reapply
  active event caps to every snapshot. Character-edit propagation is still pending.
- `cyoa-application`: inward-owned generation ports and commands for outline, cast,
  turns and rewind; cancellation tokens and the image port. No JSON/vendor types.
- `cyoa-infrastructure`: validated bundled templates, schema generation, wire
  mapping, generation orchestration, incremental narrative extraction, and
  request-recording complete/chunked scripted transports. Claude schema adaptation
  is isolated; neither live subprocess adapter is implemented.
- `cyoa-presentation`: terminal help/version output; gameplay UI is pending.
- `cyoa-cli`: executable and composition root.
- `reference/`: source material and each backend's separate live-verification record.

The port deliberately preserves distinct namesakes, repairs ID collisions with
suffixes, supports later chapter retitling and configurable limits, and uses Unicode
lowercase rather than full Python casefold. [Project contracts](docs/decisions/README.md)
override conflicting Python behavior. Nonblank business-text types normalize input;
audit text preserves every byte. Raw JSON and narrative previews cannot authorize a
turn commit. Errors and observed cancellation leave the complete game unchanged;
retry is explicit. Canonical UI ownership and stale-worker rejection remain future
presentation work.

`GenerationTemplates::bundled()` validates configuration structure and owns fallible
rendering. World, cast and turn requests use the same instance. The opening identity
review is included once in the existing opening request, without an extra model
call. Public arbitrary overrides remain unavailable until
[PROMPTS-003](docs/decisions/README.md#prompts-003-arbitrary-overrides-require-business-invariant-validation)'s
semantic configuration validator is implemented; structural checks are insufficient.

The complete acceptance scenario generates a world, cast and five successful turns,
with a malformed response, explicit retry, streamed cancellation and rewind. It
checks request counts, namesake updates, chapter continuity, bounded memory and
exact future context. This is scripted evidence, not verification of live models,
process cancellation, disk persistence or export.

Build and check with a stable toolchain supporting Rust edition 2024:

```sh
cargo run -p cyoa-cli -- --help
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
bash scripts/check_contracts.sh
```

The shared local/CI gate requires Bash, jq, Git, tar and the Rust toolchain; no
Python is involved. It checks registered test presence, ignored tests, coverage
claims, inward dependencies, workspace tests and compile-fail examples. It also
copies the current source to a temporary directory and verifies that four deliberate
regressions fail their exact registered behavioral tests. Compiler failures and
missing tests do not count as detected regressions. The isolated build uses cached
Cargo dependencies after Clippy; the first mutation build costs additional time.
Run `bash scripts/check_contract_mutations.sh` for only that check.

See the [acceptance sequence](docs/decisions/phase0-acceptance.md) and
[TDD/evidence record](reviews/2026-09-25-phase0-acceptance/README.md). The next phase is
[real subprocess adapters and a playable headless loop](docs/plans/phase1-headless-backends.md),
with a detailed commit sequence, TDD cases, process-cleanup requirements and separate
authenticated acceptance gates for the two co-equal backends.

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
