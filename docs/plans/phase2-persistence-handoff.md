# Phase 2 persistence: Astra planning handoff

Prepared 2026-10-03 from merged `main` at `1663f62`, also `origin/main`.
Working branch: `phase-2/persistence`, created directly from that main baseline.
This handoff commissions architecture and implementation planning. Persistence
is not implemented; this document does not establish new enforced contracts.

## Task and completion condition

Produce `docs/plans/phase2-persistence.md`: an implementation-ready plan for
save/load/list, atomic writes and backup, migrations, autosave, and headless
save/load/rewind integration. Inspect the actual code and existing contracts;
resolve routine technical choices with reasoned proposals. Identify any product
policy that genuinely requires a user decision, without inventing approval
requirements for ordinary architecture choices.

This stage covers planning and documentation. A complete plan names concrete
interfaces, ownership, legal transitions, save representation, failure semantics,
acceptance tests, risk order and thematic implementation commits. It must give
another implementing agent enough detail to proceed without redesigning the
architecture. Commit the plan and necessary documentation updates atomically.
Runtime code, dependency additions and persistence implementation belong to the
subsequent implementation stage. Do not create placeholder tests or mark pending
contracts enforced during planning.

## Read first

Follow [AGENTS.md](../../AGENTS.md): read [PLAN.md](../../PLAN.md) in full, then
[project decisions](../decisions/README.md) and
[required tests](../decisions/required-tests.json). Read
[engine notes](../../reference/00-engine-notes.md) and your own available CLI's
reference as instructed there. These contracts override historical source/layout
sketches. Also read the [Phase 0 acceptance record](../decisions/phase0-acceptance.md)
and current [README status](../../README.md).

For the completed runtime, use the
[controller/worker plan](phase1-controller-and-worker.md),
[presentation handoff](phase1-presentation-handoff.md), and actual implementation.
Some dated handoffs have historical mutation counts; the current gate has 37
handwritten behavioral mutations plus the automated domain sweep.

Current evidence:

- [Runtime/Nextest gate](../../reviews/2026-10-03-nextest-workspace/README.md):
  337 runtime tests and seven compile-fail doctests passed; both mutation gates
  passed. Local evidence, not a remote CI claim.
- [Backpressure repair](../../reviews/2026-10-03-headless-backpressure/README.md):
  queued nonblocking output, finite stall/drain bounds, cancellation and cleanup.
- [Domain mutations](../../docs/testing/domain-mutations.md): domain-only source
  and tests, pinned tools, reviewed equivalent exclusions; no survivor allowance.
- Both backend adapters passed their live adapter gates. Codex passed the shipped
  live headless gate; Claude's independent live headless gate remains pending.
  This does not block vendor-independent persistence planning. No live model
  calls are necessary for this planning task.

## Existing requirements to carry forward

These are already recorded decisions, not new suggestions:

1. **Layer boundaries (ARCH-001/ARCH-002).** Domain stays independent of serde,
   JSON, terminal and filesystem concerns. Application owns use cases and
   inward-owned persistence ports. Infrastructure implements save DTO mapping,
   JSON/filesystem behavior and those ports. Presentation drives commands and
   queries; `main.rs` wires concrete implementations. Preserve lightweight CQRS
   without a bus, event sourcing or separate databases.
2. **Restore rules (LIMITS-001/STATE-001).** Persist original limits explicitly.
   User-facing restore must deliberately select current or original settings;
   every stored summary's event cap and future prose context follow that choice.
   Preserve original metadata through repeated save/load cycles. Raising a cap
   cannot resurrect discarded history. Generation-only NPC caps/playable minimums
   must not prune or reject an established world on load. Revalidate the saved
   playable position through `World::select` against the reconstructed world.
3. **Save representation (PLAN persistence/serde sections).** Versioned envelope
   with title, saved_at and turn_count above game; flat saved style keys. Keep
   turn records, stored summaries, canonical world/selection, limits and audit
   raw_response. Do not save prompts by default; account explicitly for existing
   optional debug prompt traces. Defaulted source fields remain defaulted, required
   fields remain required. Save DTOs are distinct from tolerant generation DTOs.
4. **Compatibility.** Migrate through JSON Value before typed deserialization;
   reject unknown future versions clearly. Specify a migration scaffold from v1
   and frozen format fixtures, including forward-compatible optional fields.
   Do not claim support for invented historical formats. Calibre import is later
   work; if discussed, absent original-limit metadata cannot be fabricated.
5. **Durability (PRODUCT-001/PLAN).** Atomic file replacement with one `.bak`;
   `tempfile::NamedTempFile::persist` is the recorded starting choice. Saves live
   under ProjectDirs/XDG data paths with `<slug>.assets/` reserved for later images.
   Autosave after every canonical committed turn and on quit, without save prompts.
6. **Ownership and cancellation (STREAM-001).** Persist canonical accepted state,
   never previews, rejected results or worker-owned speculative snapshots. A stale,
   duplicate or cancelled completion cannot commit or trigger a false autosave.
   Preserve worker cancellation/join, revision checks, explicit generation retry
   and headless shutdown behavior. Do not hold a state mutex during model work.
7. **Domain identity/history.** Namesakes and repaired IDs remain independently
   addressable and ordered; no name-only consolidation on load. Preserve later
   chapter retitles, rewind behavior, snapshot data, null/empty semantics and
   exact audit text. Derived chapter/context views are not competing authorities.
8. **User surface.** Phase 2 includes save/load/list, rewind and headless commands
   (`/save`, `/load`, `/rewind`, existing `/quit`). Existing domain/application
   rewind already works; headless does not yet expose it. TUI, export, character
   editing, public prompt overrides and images remain later work.

## Inspect these concrete seams

- `cyoa-core/src/game.rs`: private GameState, `start`, `restore`, active/original
  limits, turn snapshots, `rewind`, derived chapters/context.
- `cyoa-core/src/world.rs`: `WorldCast::new` applies generation-time deduplication,
  cap and minimum. It is **not** an established-world restoration constructor.
  Determine the minimal checked reconstruction API needed without changing
  generation behavior or silently relaxing stored-state invariants.
- `cyoa-core/src/turn.rs`, `character.rs`, `summary.rs`, `limits.rs`, `style.rs`,
  `text.rs`: checked construction, IDs/order, immutable records and audit fields.
- `cyoa-application/src/generation.rs`: application generation commands and rewind;
  there is no persistence port or save/load use case yet.
- `cyoa-presentation/src/session.rs`: sole canonical owner, request keys/revisions,
  `from_game` seeds a validated **in-memory** game (not a disk-load boundary).
- `cyoa-presentation/src/runtime.rs`, `worker.rs`: admitted effects, completion only
  after join, cancellation authority, no concrete infrastructure dependencies.
- `cyoa-presentation/src/headless.rs`, `headless/output.rs`, `terminal.rs`:
  existing memory-only notice, commands, nonblocking event loop and output bounds.
- `cyoa-cli/src/main.rs`, `cyoa-presentation/src/commands.rs`: composition and CLI
  settings; generation auth currently precedes play. Examine whether listing or
  inspecting saves should require vendor auth, rather than inheriting that order.
- Existing regressions: `cyoa-core/tests/restore_limits.rs`,
  `cyoa-infrastructure/tests/generation_use_cases.rs`,
  `cyoa-infrastructure/tests/phase_zero_acceptance.rs`,
  `cyoa-cli/tests/controller_story.rs`, `controller_backends.rs`, `headless.rs`,
  `headless_output.rs`, `demo_retry.rs`, and presentation session/worker tests.
  Generation wire DTOs in infrastructure are reference material, not the save
  format. Verify constructor limitations directly before naming proposed APIs.

## Decisions the plan must resolve

Separate requirements above from newly proposed policy. For each consequential
choice, give the chosen approach, alternatives rejected, failure behavior and test.

- Exact envelope/game DTO schema: IDs, optional fields, original versus active
  limits, full turn/summary fidelity, provenance and optional debug prompt traces.
  Decide validation of metadata inconsistent with the game and structurally valid
  but semantically invalid/corrupt histories; avoid silent repairs that lose data.
- Inward-owned repository/use-case interfaces, checked save identifiers and listing
  views; mapping boundaries; clock/time metadata; owning session/save identity.
- Save-name collisions, path traversal, slug changes/renames, symlinks, overwrite
  authorization and multiple processes writing the same save. State platform and
  filesystem assumptions instead of claiming untested universal guarantees.
- Atomicity versus crash durability: temporary placement, file/directory syncing,
  backup rotation ordering, first save, crash windows and recovery. Distinguish
  failures before replacement from errors after replacement/while syncing. A
  corrupted primary must not silently destroy the last known-good backup.
- Autosave result versus generation result: if a turn is already committed but its
  disk write fails, define dirty state, visible outcome, retry operation and what
  may run next. Storage retry must not regenerate the turn or recommit its response.
- Save/load/rewind during active generation: reject, cancel-and-join, or another
  explicit safe policy. Define revision/identity behavior so an old worker cannot
  overwrite a newly loaded game. Plan load failure with exact old-state preservation.
- Normal quit, EOF, idle/active SIGINT, input/output errors and worker panic: define
  which canonical snapshot is saved, failure reporting, exit status, cleanup order
  and bounds. File I/O must not accidentally undo headless cancellation guarantees;
  choose and justify effect scheduling rather than adding an async runtime by habit.
- What can be saved before character selection/the first turn, first-save timing,
  and whether saves include only GameState or unfinished lifecycle stages. Define
  user behavior in stages where saving/loading/listing is unavailable.
- Rewind durability and re-saving: persist the rewound canonical state, restore
  limits/history correctly and resume generation with the expected next prompt.
- Demo saves/resume and source/backend identity: avoid replay drift or silently
  changing to live inference. Keep both inference backends co-equal; justify any
  save metadata rather than serializing concrete runtime/backend objects.
- Load selection of current/original limits without enabling unauthorized arbitrary
  config overrides (PROMPTS-003). Define unknown style/default behavior faithfully.
- Listing corrupt/future-version files, backup recovery, filesystem and input size
  bounds, and user-visible diagnostic wording. Do not report successful durability
  from a failed or partial write.

## Required deliverable structure

The plan should include:

1. Goal, scope and explicit implemented-versus-planned status.
2. Requirement/contract matrix with relevant decision IDs and existing evidence.
3. Architecture and named ports/commands/queries/DTOs, ownership and effect flow.
4. Save schema, mapping/validation, restore policy and compatibility strategy.
5. Failure/state-transition table for write/load/autosave/rewind/quit and generation
   races, with durability outcomes distinguished from in-memory outcomes.
6. Ranked risks, resolved proposals and genuinely unresolved user decisions.
7. Dependency-ordered atomic implementation steps. Each step names changed modules,
   error/edge/nominal regressions, registry updates and its completion gate.
8. Acceptance matrix: round trips of the full story (including audit bytes, namesakes,
   IDs, selection, styles, chapters and rewind); changed limits through repeated
   save/load/rewind; zero-turn saves; corrupted/missing/future-version saves;
   compatibility fixtures; write/backup/crash-window failures; listing/path policy;
   autosave failure and explicit storage retry; stale/cancelled completion/load
   races; binary headless save/load/resume and demo behavior, using temporary data
   roots and no vendor credentials. Identify feasible fault injection surfaces.
9. A completeness audit mapping **every requirement and decision above** to plan
   sections, proposed acceptance tests and implementation steps; record remaining
   uncertainty rather than declaring coverage from prose alone.

## Verification and handoff discipline

Planning: verify referenced files/APIs and internal consistency; use
`bash scripts/check_architecture.sh` and `git diff --check` if appropriate.
Documentation-only planning does not need repeated mutation sweeps or live model
calls. Leave normative contracts/coverage unchanged unless recording an explicit
user decision; label new proposals clearly. Commit the completed planning artifacts
atomically. Do not push, merge or change main as part of this planning task.

Implementation later: error → edge → nominal development; meaningful regressions
and real required-test registry updates; `bash scripts/check_contracts.sh` before
completion. That gate runs workspace Nextest (`workspace`, no retries/no fail-fast),
all Cargo doctests, all 37 isolated behavioral mutations and pinned cargo-mutants
against only core and core's tests. Never weaken assertions/exclusions to get green.
New domain behavior must also be scrutinized by the domain sweep. Infrastructure
does not need equivalent automated mutation coverage. If refactoring moves a
handwritten mutation's target, update it without changing the protected behavior.

## Goal to activate after switching to Astra

Use the text in [phase2-persistence-goal.txt](phase2-persistence-goal.txt), or:

```text
/goal Produce and commit an implementation-ready Phase 2 persistence plan in docs/plans/phase2-persistence.md, following docs/plans/phase2-persistence-handoff.md. Inspect the actual code and project contracts; specify architecture, save schema and compatibility, failure behavior, autosave/load/rewind/quit transitions, acceptance tests and dependency-ordered atomic implementation steps. Audit every requirement in the handoff against the plan. Resolve routine technical choices with reasoned proposals and identify genuinely unresolved user decisions. This goal covers planning and documentation only; retain implemented behavior and truthful contract coverage.
```
