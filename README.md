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

This repo contains the implemented Phase 0–2 Rust workspace, design research, and ported
prompt/schema reference content. No calibre checkout is needed.

## Status

**Phases 0–2 are complete; next is Phase 3, the TUI play screen over an existing
save.**

- Phase 0, the scripted engine:
  [acceptance](docs/decisions/phase0-acceptance.md).
- Phase 1, the [real CLI backends and headless play](docs/plans/phase1-headless-backends.md).
  - Both co-equal backends have frozen protocol profiles, real-child fixture
    tests, persistent adapter mutations and passed live gates. The live headless
    gates ran on 2026-10-02 for Codex 0.159.3
    ([evidence](reviews/2026-10-02-headless/README.md)) and on 2026-10-03 for
    Claude 2.1.286 ([evidence](reviews/2026-10-03-claude-headless/README.md)).
  - What each vendor's CLI was actually observed to do is recorded in
    `reference/01-claude-cli.md` and `reference/02-codex-cli.md`. Offline tests
    don't establish live vendor behaviour.
- Phase 2, [persistence](docs/plans/phase2-persistence.md): canonical autosave,
  explicit restore policy, atomic primary/backup writes with recovery, and
  bounded storage helpers ([acceptance audit](reviews/2026-10-05-persistence-s7/README.md)).
  - Filesystem sync ordering and process-kill recovery are tested on Linux.
  - Physical power-loss durability on arbitrary filesystems is not established.
- The pre-Phase 3 cleanup is recorded in
  [the 2026-10-05 review](reviews/2026-10-05-cleanup/README.md).
- Astra's independent 2026-10-05 review found three boundary bugs (ambiguous Codex
  control keys, ambiguous Claude auth status, input read-ahead); its
  [findings](reviews/2026-10-05-repository-review/README.md) and
  [repairs](reviews/2026-10-05-repository-review/repairs.md) are merged here.

Each [project contract](docs/decisions/README.md) states whether it is enforced,
and a Partial one names its remaining gap. Dated evidence lives in `reviews/`
and [the decision history](docs/decisions/history.md).

- `cyoa-core`: checked domain types, namesake-preserving casts and stable IDs,
  summary deltas, typed limits, owned protagonist selection, turns, chapters and
  rewind. Current/original restore policies preserve original settings and reapply
  active event caps to every snapshot. Character-edit propagation is still pending.
- `cyoa-application`: inward-owned generation ports and commands for outline, cast,
  turns and rewind; cancellation tokens with scoped wake notifications, transport
  diagnostics and the image port. Typed persistence ports/commands/queries apply
  explicit restore policy and preserve storage outcomes. No JSON/vendor types.
- `cyoa-infrastructure`: validated bundled templates, schema generation, wire
  mapping, generation orchestration, incremental narrative extraction,
  request-recording complete/chunked scripted transports and a process supervisor
  exercised against an in-repo subprocess fixture. Claude schema adaptation and the
  local advisor-suppression instruction live in `backend_compat::claude_cli`. Codex
  and Claude each have isolated request preparation, a private protocol codec and an
  executable `Backend` reconciled with the supervisor. Separate save DTOs and a
  bounded version-one codec validate historical state without replay or repair. A
  Linux atomic local repository and a supervised storage helper (one bounded child
  per operation) implement the persistence port. Vendor-neutral transport plumbing
  shared by both adapters lives in `backends::transport`.
- `cyoa-presentation`: session controller, owned worker/runtime, the storage runner
  and persistence coordinator (autosave, explicit load/rewind, final save on quit),
  explicit CLI intent and Linux headless input/output with cancellation and shutdown.
- `cyoa-cli`: executable and composition root, which also serves as the internal
  storage helper entry point.
- `reference/`: source material and each backend's separate live-verification record.

The port deliberately preserves distinct namesakes, repairs ID collisions with
suffixes, supports later chapter retitling and configurable limits, and uses Unicode
lowercase rather than full Python casefold. [Project contracts](docs/decisions/README.md)
override conflicting Python behavior. Nonblank business-text types normalize input;
audit text preserves every byte. Raw JSON and narrative previews cannot authorize a
turn commit. Errors and observed cancellation leave the complete game unchanged;
retry is explicit. The terminal-independent controller enforces canonical ownership
and stale-worker rejection; the headless view drives this same controller.

## Play headless (Linux)

```sh
cargo run -p cyoa-cli -- play --headless --demo
cargo run -p cyoa-cli -- play --headless --backend codex
cargo run -p cyoa-cli -- play --headless --backend claude
cargo run -p cyoa-cli -- list
cargo run -p cyoa-cli -- inspect SAVE_ID
cargo run -p cyoa-cli -- play --headless --demo --load SAVE_ID --limits original
```

Enter a brief, accept the outline with an empty line (or `/edit` its title and
description), then choose a one-based character number. Play with ordinary text,
an empty line to continue, or `/action N`. Ordinary numbers remain player text;
`//` sends an initial slash. `/event` requests an interesting event. `/help`,
`/inspect`, `/diagnostics`, `/retry`, `/cancel` and `/quit` are available.
Retry repeats the failed intent only when a failure/cancellation has finished.
Unsupported commands make no inference call. Selection saves the zero-turn game
before its automatic opening; each accepted turn and rewind autosaves. `/quit`,
EOF and idle Ctrl-C save the last canonical state before exit. A failed save
retains the story in memory and blocks further play until `/save` succeeds;
storage retry makes no inference call. Wait for `Saved: ... durability=Clean`
before sending the next story command in scripts.

Use `/save`, `/save-copy`, `/list [AFTER_ID]`, `/load SAVE_ID --limits
current|original [--backup]` and `/rewind N` during idle play. `/save-copy` creates
a new slot and switches only after success. Every load requires an explicit
limits policy; `current` uses this executable's default settings, while `original`
uses the game's creation settings. A loaded zero-turn game waits for explicit
continuation. Backup recovery creates a fresh slot and preserves the original
primary/backup. List and inspect need no vendor credentials; valid live resumes
authenticate the explicitly chosen backend after save validation. Demo saves
require `--demo`; live saves accept either selected backend.

Saves live under `$XDG_DATA_HOME/cyoa/saves` on Linux, with the normal user-data
fallback. Global `--data-dir ABS_PATH` selects an app directory whose `saves/`
subdirectory is managed. Relative paths are rejected. Saves are version-one JSON
with an exact previous-byte backup; IDs displayed after saving are stable.

Story prose goes to stdout; prompts, actions, diagnostics and preview status go to
stderr. Preview prose is tentative until a committed marker; failures discard it,
and a disagreeing final passage is printed as a correction. Ctrl-C during
generation cancels and joins the worker; idle Ctrl-C, `/quit` and EOF exit. Input
while busy is rejected except the control/query commands. Scripts must wait for
stage prompts rather than queue an entire game; EOF during generation cancels it.
Input lines are UTF-8, bounded to 64 KiB including CR/LF delimiters when present;
buffered bytes from later lines do not count against the current line.
A closed or stalled output pipe is an I/O
error and closes the session instead of blocking cancellation.

Choose exactly one backend or demo. Optional `--executable`, `--home`,
`--config-dir` and `--model` apply only to a selected vendor; there is no fallback
or API-key login change. HOME/PATH and the chosen vendor's config-directory
environment variable supply defaults. Auth preflight runs before the UI and is
cancellable by Ctrl-C. Demo `harbour-v1` replays the frozen Phase 0 story through
the same controller and engine, without a vendor executable, auth or network;
five prerecorded turns then explicitly exhaust. Choices do not alter its fiction.

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
cargo install cargo-mutants --version 27.1.0 --locked
cargo install cargo-nextest --version 0.9.132 --locked
bash scripts/check_contracts.sh
```

The shared local/CI gate requires Bash, jq, Git, tar and the Rust toolchain; no
Python is involved. It checks registered test presence, ignored tests, coverage
claims and inward dependencies, then runs workspace runtime tests with Nextest
and doctests (including compile-fail examples) with Cargo. The `workspace`
Nextest profile disables retries and runs the complete suite even after a failure.
Pinned tool versions are checked before starting the gate. It also
copies the current source to a temporary directory and verifies that every deliberate regression in the manifest
fails its exact registered behavioral test. Compiler failures and
missing tests do not count as detected regressions. The isolated build uses cached
Cargo dependencies after Clippy; the first mutation build costs additional time.
Run `bash scripts/check_contract_mutations.sh` for only that check.
Set `CYOA_MUTATION_EVIDENCE_DIR` to retain each baseline, mutant and restored log.

For ordinary tests without the mutation sweeps:

```sh
cargo nextest run --workspace --lib --bins --tests --profile workspace --locked --offline
cargo test --workspace --doc --locked --offline
```

Cargo remains responsible for registered-test discovery and the isolated targeted
mutation checks, whose strict runtime-failure parser is unchanged. The benchmark
and backpressure repair evidence are in
[the benchmark](reviews/2026-10-03-nextest-benchmark/README.md) and
[the headless repair](reviews/2026-10-03-headless-backpressure/README.md).

The gate also runs **automatically generated mutations across `cyoa-core`**, using
only the domain's own tests. Install the pinned tools once as above. Run
`bash scripts/check_domain_mutations.sh` for that sweep alone; reports live in
`mutants.out/` and CI uploads them. Survivors fail the gate; compile-invalid
mutations are separate from detections. Counts change as the domain grows: each gate
run prints its own tally, and dated tallies are kept with the reviews that ran them.
See [scope and limitations](docs/testing/domain-mutations.md) and the
[initial evidence](reviews/2026-10-03-domain-mutations/README.md).

The phase-by-phase status is in [Status](#status) above.

## Where to start

1. Read `PLAN.md` in full. It has the architecture, the reasoning behind
   every non-obvious decision, the build order, and a ranked list of the
   risky parts. Phases 0–2 are complete. Continue with Phase 3's TUI play screen
   over an existing save, using the Phase 2 acceptance audit and credential-free
   demo for iteration.
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
