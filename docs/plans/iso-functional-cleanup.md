# Revised objective: iso-functional application cleanup

Explicit user direction: the existing headless/REPL product is already a lean MVP.
The next task is architectural cleanup preserving its functionality, not adding
stats, OOC, new gameplay, branching history or other roadmap capabilities.

Produce a reusable core application library that owns canonical session lifecycle,
commands/queries, admission, acceptance, persistence/recovery and shutdown policy.
CLI commands and the existing REPL are driving frontends; future frontend apps
can use the same library without reproducing coordination. This does not mean
putting orchestration into `cyoa-core`: retain domain/application separation and
inward dependencies. Infrastructure owns worker/subprocess/storage execution;
presentation owns interaction and terminal output/restoration. CLI wires adapters.

Preserve observable behavior, existing save format, both co-equal inference backends,
demo generator state, exact diagnostics, cancellation, storage recovery and all
registered invariants. No new gameplay or ergonomic feature is a prerequisite.
The sophisticated [roadmap](mvp-and-roadmap.md) remains future work; earlier text
calling additional stats/OOC features prerequisites for an MVP is superseded.

Implemented after review on 2026-11-03. The shared gate passed with 467 runtime
tests, 9 doctests, all 45 handwritten faults and the full domain-only sweep.
See the [ownership audit and retained evidence](../../reviews/2026-11-03-iso-functional-cleanup/README.md).
Existing assertions, registry and mutation locations follow the ownership moves.

## Concrete plan for review

Status: approved and implemented on 2026-11-03 ("okay this seems
fine, resume the goal" after reviewing storage integration). This plan covers the existing implementation only.
The [October 17 design](session-architecture-refactor.md) supplies rationale;
the broad November roleplay design is future work, not a dependency here.

### What a caller gets

`cyoa-application` becomes the reusable library owning the complete current
session behavior. A caller supplies generation/storage execution adapters,
submits typed commands, calls a nonblocking advance method and reads query views.
It does not need `cyoa-presentation` or terminal types. The REPL is the first such
caller. Standalone list/inspect/load use cases remain available independently of
an active story session, with no unnecessary vendor authentication.

No HTTP server, IPC frontend protocol, async runtime, new crate or universal
job framework. A future frontend's composition root can depend on infrastructure
to select real adapters, while its interaction code depends on application views.

Existing commands and behavior remain:

| Use case | Application interface responsibility |
| --- | --- |
| Submit brief | Admit one outline request, expose review state and failure |
| Replace/accept outline | Validate stage, invalidate stale replies, generate cast from the edited outline |
| Select character | Check ownership/index, establish zero-turn game, save, then start exactly one opening |
| Turn/action/continue/event | Admit current intent, preserve preview/final distinction, accept once, autosave |
| Retry/cancel | Retry only the recorded failed intent; cancellation wins unaccepted completion and joins before reuse |
| Save/copy | Save current accepted revision; bind copy only on success; reconcile exact uncertain attempt |
| List/inspect | Read-only repository queries and typed results, no game mutation or vendor auth |
| Load | Explicit source/copy/limits policy, coherent replacement, preserve old session on failure |
| Rewind | Existing linear turn removal and snapshot/limit rules; persist before another generation |
| Finish | Join/cancel generation, settle current write and bounded final save; preserve failures |

The CLI still handles flags and backend selection; REPL still handles slash syntax,
outline edit buffers and formatting. There is no new user command or changed
save/turn format. The domain remains a single selected protagonist and linear log.

### Source responsibilities and destinations

| Current code | Destination / treatment |
| --- | --- |
| `cyoa-presentation/src/session.rs` | Application-owned private canonical lifecycle/admission/acceptance; public read-only views replace controller exposure |
| `cyoa-presentation/src/runtime.rs` | Application command dispatch and generation coordination; executor dependency through inward port |
| `cyoa-presentation/src/persistence.rs` | Application autosave, binding, source/restore policy, recovery and final-save coordination |
| `cyoa-presentation/src/worker.rs` | Infrastructure generation executor; application owns request/progress/completion contracts |
| `cyoa-presentation/src/storage.rs` | Infrastructure storage executor; retains preparation capture independently of failing worker |
| `cyoa-presentation/src/headless.rs` | Remains presentation; use application commands/views, own output completion locally |
| `cyoa-presentation/src/terminal.rs` | Terminal input/flags/signals remain presentation; general blocking-job execution moves behind an infrastructure execution seam if shared outside terminal use |
| `cyoa-application/src/generation.rs` | Business port/signature vocabulary moves inward; application use cases remain; technical evidence stays application |
| `cyoa-application/src/persistence.rs` | Business values/port move inward, application commands/queries remain; opaque attempt replaces exposed encoded-byte evidence |
| `cyoa-cli/src/main.rs` | Composition root wires application and infrastructure, then calls presentation; no session-policy branches |
| Infrastructure wire/templates/helper/codec | Retain implementation and protocol/format; update inward type imports only |

Application may internally split lifecycle, persistence and execution coordination
into cohesive modules. No obligation to create one giant `AdventureSession` type.
Its public interface must not hand clients a mutable controller or make them
sequence `select -> save -> opening` themselves.

### Port and signature migration

Keep typed generation operations `outline`, `cast`, `turn`; do not introduce the
future setup/rules/OOC/succession methods. Business generation results retain
checked value, exact raw response and provenance. Separate technical evidence from
business failure without dropping it: Cancelled/Unavailable/Timeout/Transport/
InvalidResponse/Configuration, spawn failure and worker panic remain distinguishable.
An unmade call stays `None`; an empty received response stays `Some(empty)`.

The domain cannot import the application cancellation token/transport buffers.
Use inward-owned observation capabilities for the business signature; the
infrastructure invocation context retains its concrete cancellation token with
race-free wake subscriptions. The supervisor must still wake when stdout is idle,
not merely poll a domain flag. Preserve successful and failed diagnostics in
application execution reports; do not discard them when extracting domain values.

Retain save identity/revision/stamp/receipt/copy/source/limits invariants inward.
Move no filesystem path, helper DTO, stdout/stderr, thread handle or encoded JSON
into domain. Enumeration/inspection view contracts remain application-owned.
Repository queries may keep `&mut self` for their mutable resource ownership.

The repository write port has an associated opaque attempt type. A required
observer publishes that attempt before first mutation; infrastructure privately
retains its encoded bytes/metadata and the executor captures it outside the worker.
Application can retain and pass the handle to a fresh worker but cannot inspect
bytes. Keep create/expected-stamp replace/reconcile distinct. Preserve Unchanged,
Replaced and Unknown visibility separately from success/durability/cleanup errors;
a late cancellation cannot erase a durable receipt. No new registry or restart
journal. The helper wire retains its private exact-byte preparation machinery.

### Execution and application acceptance

Application declares separate generation and storage execution ports. Each owns
typed requests/events with operation keys and base revisions, admits one relevant
job at a time, reports start failure, supports cancel/closing and yields terminal
events only after join/cleanup. Executors do not choose autosave, opening, retries
or canonical acceptance. Stateful demo generation stays owned across requests.

`advance()` performs bounded, nonblocking event handling and applies the existing
stale/duplicate/kind/revision/cancellation rules. It cannot block on disk I/O or a
live thread join. Returned turn games are already resolved owned snapshots;
acceptance replaces canonical state once, never commits a second time.

Read-only views expose stage, story/actions, bounded preview, failure evidence,
binding/durability and completion obligations. Their concrete shapes should reuse
current types where semantically appropriate, without requiring presentation
imports. A query cannot confer authority to bypass command admission.

Application completion means generation/storage obligations are satisfied. The
REPL then performs its existing absolute-deadline output drain and restores flags.
No `output_drained()` application method: that acknowledgement is local to the
presentation adapter. I/O errors remain exit failures even after successful saving.

### Library storage integration

The current supervised repository starts the executable passed to `HelperConfig`
with `--cyoa-storage-helper-v1`. Current CLI passes its own executable, whose
composition root handles that mode before Clap/auth. Preserve that CLI behavior.
Do not assume an arbitrary embedding app's `current_exe()` implements the helper.

Library docs/example must show explicit helper configuration: an embedding app
can supply the installed `cyoa` executable as helper, or route the same existing
public infrastructure `run_internal` entry point before its own frontend startup.
The application library itself neither parses process arguments nor locates the
helper. No need for another installed binary or changed helper protocol in this
refactor. A controlled library-client test exercises the explicit-helper path.

## Dependency-ordered implementation slices

| Slice | Concrete result | Required evidence |
| --- | --- | --- |
| 0 Baseline | Run pinned shared gate on current implementation; retain log/outcome | Baseline failures recorded separately, not blamed on refactor or hidden |
| 1 Business vocabulary | Map each generation/repository signature type to domain or application; introduce execution reports without behavioral changes | All callers compile; field/category mappings preserve exact audit and visibility; no dependency allowlist exception |
| 2 Opaque evidence | Handle and required observer wired through existing repository/helper/executor flow | Publish-before-mutation, panic survival, fresh-worker reconcile, failed-copy retention, inaccessible encoded bytes |
| 3 Execution ports | Existing worker/storage implementation moved to infrastructure behind application ports | Nonblocking poll, cancellation wake/join, generator reuse, spawn/panic paths, storage capture independent of worker |
| 4 Session library | Policy moved to application; stage/persistence/closing commands and views | Current selection/opening/autosave/load/rewind/acceptance/counter/quit tests pass at new locations with same assertions |
| 5 CLI/REPL adapter | Headless and main drive public application interface; output drain local | Existing command/output/save fixtures, CLI auth/query sequencing, EOF/SIGINT/output failure behavior |
| 6 Embedding proof and audit | Non-presentation client drives public library; remove superseded ownership wrappers and document integration | Compile/run client with no presentation dependency; full gate, registry/mutations/contracts/ADRs updated |

Each slice keeps a runnable product; the feature set is unchanged. Short-lived
reexports can reduce mechanical churn but must not become an alternate canonical
session. Tests follow their policy/execution ownership without creating inward
dev-dependencies on infrastructure or presentation. Test fixtures implement inward
ports; real adapter/headless compositions remain in outward integration tests.

## Regression evidence and acceptance

Use `bash scripts/check_contracts.sh` before and after implementation. It checks
registered-test presence, architecture, Nextest workspace runtime tests, Cargo
doctests/compile-fail examples, 45 handwritten mutations and the domain-only
automated mutation sweep with pinned tools. Presence is not proof of a passing
run, and mutation results are not proof against every possible fault.

Protect all existing assertions, particularly:

- Exact world/character identity, selection ownership, limits restoration, merge,
  chapter/prose context and raw audit behavior in domain tests.
- Full frozen engine/controller/headless stories and the exact nine-request
  Phase 0 scenario; no new inference call or automatic retry.
- Save v1 round trips and strict validation; atomic writes/backup rotation/locks,
  source compatibility, explicit restore policy and backup recovery.
- Initial zero-turn save before one opening, failed autosave, uncertain writes,
  failed save-copy, stale receipts, interrupted load and reserved final-save capacity.
- Cancellation while subprocess is silent, late cancelled completion, join/reap,
  spawn/worker failure and bounded previews.
- Input read-ahead limits, non-UTF-8 input/diagnostics, blocked/broken output,
  bounded final draining and failure-preserving shutdown.
- Both adapters driven by synthetic real children and demo restart/rewind/retry.

Add focused acceptance for the architectural change:

1. An external-style client depends on core/application plus outward adapter
   composition, **not presentation**. It submits brief, accepts/edits world,
   selects, observes save-before-opening, takes/retries/cancels a turn, rewinds,
   saves/loads and closes using the public application interface. It renders
   nothing and uses no stdin/signals/terminal globals.
2. A caller using controlled execution ports sees exactly the same accepted game
   and persistence obligations through failures/stale events as the current REPL.
3. A real infrastructure executor integration test exercises explicit helper
   executable configuration without assuming frontend `current_exe()` behavior.
4. Compile-fail or accessibility checks prevent external mutation of canonical
   session internals and inspection of opaque prepared bytes.
5. Application completion occurs while a presentation sink is still blocked;
   output drain remains bounded independently and does not suppress prior errors.

These tests prove the specified cases, not universal equivalence. Source review
must additionally check signature ownership, no policy left in frontend, no I/O
hidden in application through `std`, and no dropped diagnostics. Backend request
profiles remain unchanged; existing live evidence retains its historical scope.
If an actual backend invocation/schema change becomes necessary, stop treating
it as an import-only move and apply the project's own-backend live evidence rule.

## Contract changes and exclusions

ARCH-001/002 and PRODUCT-001/STREAM-001 need ownership descriptions and test
registrations updated when implementation moves. TEXT-001 must retain byte-exact
execution evidence and missing/empty distinction. Required-test moves/renames
update the registry with the same assertions. Update handwritten mutation target
paths without weakening their outcome checker. Review automated domain mutation
exclusions if added/moved domain code changes their locations or assumptions.

STATE-001, IDENTITY-001/002, LIMITS-001, CHAPTER-001, PROMPTS-001/002/003,
ARCH-003, BACKENDS-001, ACCEPTANCE-001 and TESTING-001 behavior remains unchanged.
No fixed-protagonist replacement, new terminal turn variant, format bump, branch
replay, OOC/stats feature, context truncation, export, GUI or TUI in this cleanup.
Preserve current default paths, CLI flags, public command semantics and platform
support; library reusability does not imply a new platform certification.

## Review checkpoint

Before-refactor gate now passed: **460 runtime tests, 7 doctests, all 45 handwritten
mutations**, plus 181 generated domain mutants (101 caught, 80 unviable, no survivors).
Two caught cases were per-test nontermination failures. Full command/evidence:
[baseline record](../../reviews/2026-11-02-iso-functional-baseline/README.md).
This is observed existing coverage, not implementation completion or proof of
universal equivalence.

The user explicitly required reviewing the concrete plan before implementation.
This document is that checkpoint. Approval is for the iso-functional slices above,
not the deferred game model. The active goal is not complete merely because this
plan exists: implementation and the passing shared gate were required afterward
and are recorded in the linked completion audit.
