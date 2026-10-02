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

The Phase 0 engine works through scripted generation. Phase 1 now has transport
outcome types and a vendor-neutral process supervisor tested with real children
on Linux. The [process review](reviews/2026-09-25-process-supervisor/README.md)
and [repair record](reviews/2026-09-25-supervisor-repairs/README.md) document the
correctness gaps found after the first implementation and their fixes: bounded
capture/cleanup, exact request delivery, authoritative cancellation, diagnostics
propagation and owned request directories.

Claude evidence was refreshed on 2026-09-25; Codex's
[0.157.1 protocol profile](reviews/2026-09-26-codex-profile/README.md) was frozen on
2026-09-26 using bounded bundled-request probes, with full account/tool isolation
still unverified. Codex has an executable adapter and a completed live adapter gate
on 0.159.3 (outline, cast, opening, continuation and controlled cancellation);
Claude's executable adapter and headless play remain pending.
See the [Phase 1 plan](docs/plans/phase1-headless-backends.md)
and each backend reference file for the remaining work; offline transport tests
do not establish live vendor behavior.

Codex adapter **step 2 is implemented**: isolated schema adaptation and owned,
offline invocation preparation, with [review repairs and evidence](reviews/2026-09-26-codex-step2/review-and-repairs.md).
**Step 3 is implemented offline**: a private Codex event state machine validates
the frozen complete-only profile and retains exact candidate evidence on failure.
See the [implementation record](reviews/2026-09-27-codex-step3/README.md).
**Step 4 is implemented**: the actual Codex Backend reconciles protocol and process
outcomes, preserves failure evidence, and checks subscription auth at construction.
[Evidence](reviews/2026-09-27-codex-step4/README.md) distinguishes real-child tests
from the bounded live outline smoke. **Step 5 is implemented offline**: composed
story acceptance checks identity, limits, chapters, cancellation, cleanup failures
and explicit retry through actual fixture children
([evidence](reviews/2026-09-28-codex-step5/README.md)). **Step 6 is implemented**:
seven persistent adapter mutations protect protocol acceptance, shared/peer schema
isolation and failure evidence ([record](reviews/2026-09-28-codex-step6/README.md)).
**Step 7 passed live on 2026-10-02**: four successful generations and controlled
cancellation with observed PID/workspace cleanup and unchanged game state
([evidence](reviews/2026-10-02-codex-step7/README.md)). Next is the
[Claude adapter slice](docs/plans/phase1-claude-adapter-handoff.md), with independent
live acceptance, followed by presentation/headless work. No playable UI is claimed.

- `cyoa-core`: checked domain types, namesake-preserving casts and stable IDs,
  summary deltas, typed limits, owned protagonist selection, turns, chapters and
  rewind. Current/original restore policies preserve original settings and reapply
  active event caps to every snapshot. Character-edit propagation is still pending.
- `cyoa-application`: inward-owned generation ports and commands for outline, cast,
  turns and rewind; cancellation tokens with scoped wake notifications, transport
  diagnostics and the image port. No JSON/vendor types.
- `cyoa-infrastructure`: validated bundled templates, schema generation, wire
  mapping, generation orchestration, incremental narrative extraction,
  request-recording complete/chunked scripted transports and a process supervisor
  exercised against an in-repo subprocess fixture. Claude schema adaptation is
  isolated in `backend_compat::claude_cli`; advisor-suppression instructions exist
  in evidence and plans only. Codex schema/invocation preparation and private
  protocol decoding and an executable Codex Backend are implemented;
  Claude's executable Backend remains pending.
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
copies the current source to a temporary directory and verifies that twenty-nine deliberate
regressions fail their exact registered behavioral tests. Compiler failures and
missing tests do not count as detected regressions. The isolated build uses cached
Cargo dependencies after Clippy; the first mutation build costs additional time.
Run `bash scripts/check_contract_mutations.sh` for only that check.
Set `CYOA_MUTATION_EVIDENCE_DIR` to retain each baseline, mutant and restored log.

See the [acceptance sequence](docs/decisions/phase0-acceptance.md) and
[TDD/evidence record](reviews/2026-09-25-phase0-acceptance/README.md). Phase 1
is [real subprocess adapters and a playable headless loop](docs/plans/phase1-headless-backends.md)
— transport foundations and Codex Backend reconciliation are implemented and
Linux-tested, including composed Codex story acceptance and persistent adapter mutations;
Claude implementation and the full authenticated acceptance gates remain separate
obligations.

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
