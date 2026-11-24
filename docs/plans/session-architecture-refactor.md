# Session architecture refactor: design for review

Current implementation scope and review checkpoint:
[iso-functional cleanup plan](iso-functional-cleanup.md). The REPL MVP already
works; the broader roleplay model below/in linked documents is not a prerequisite.

Current technical refinement: [November 2 contract](2026-11-02-technical-contract.md)
and [slice plan](2026-11-02-implementation-plan.md), both proposed for review.

The previously deferred specification now has a
[domain/application contract proposal](2026-10-30-domain-application-contract.md).
It is for review, not approved or implemented; references below to deferral
describe the earlier interview checkpoint.

2026-10-30 follow-up: [accepted interview decisions](../decisions/2026-10-30-roleplay-and-session.md)
confirm ownership direction and opaque attempts, require contract stabilization
before migration, and place improved REPL before TUI. Concrete interfaces remain
for review; the detailed use-case specification is explicitly deferred.

Status: proposed design, 2026-10-17. The ownership direction below was agreed in
the architecture interview; concrete interfaces and migration remain subject to
confirmation. This document does not claim implementation or change coverage.

## Purpose and constraints

Use Clean Architecture, DDD and lightweight CQRS to make responsibility and
correctness explicit. Simplicity does not justify weaker typing, mixed
responsibilities or more coupling. This is a personal working prototype: preserve
existing behavior without expanding recovery, concurrency or platform scope.

Agreed direction:

- Domain owns business-facing generation and game-repository ports.
- Application owns canonical session state, command/query handlers, admission,
  result acceptance, autosave sequencing, recovery decisions and safe completion.
- Infrastructure implements business ports and application execution ports. It
  owns threads, subprocesses, serialization, filesystem operations and cleanup.
- Presentation owns input, rendering, edit buffers, signals, terminal restoration
  and output draining. Shared presentation concerns may remain in its current crate.
- No concurrent game changes during generation or storage. Concurrent edits and
  reconciliation are a future feature, requiring a separate design and tests.
- Recovery evidence must survive worker failure within the running application.
  Persisting outstanding recovery attempts across restarts is outside this work.
- No async runtime now. A later async design must preserve these responsibilities;
  these synchronous interfaces are not claimed to be future-proof against coloring.

## Evidence from the current code

| Location | Current responsibility | Consequence for the refactor |
| --- | --- | --- |
| `cyoa-presentation/src/session.rs`, `SessionController::complete` | Canonical state, request/revision checks, cancellation before acceptance | Move policy into application; retain late-cancel and stale-result checks |
| `cyoa-presentation/src/runtime.rs`, `SessionRuntime` | Dispatch and worker coordination | Split application orchestration from infrastructure execution |
| `cyoa-presentation/src/persistence.rs`, `PersistedSession` | Autosave, admission, uncertain writes, final save, output-drained acknowledgement | Move game/storage policy into application; keep output completion in presentation |
| `cyoa-presentation/src/worker.rs`, `WorkerRunner` | Owns the generation adapter and thread, preview mailbox, join | Move execution into infrastructure; retain adapter state across requests |
| `cyoa-presentation/src/storage.rs`, `StorageRunner` | Creates repositories per request and retains preparation across panic | Move into infrastructure; keep recovery evidence outside the failing worker |
| `cyoa-application/src/generation.rs`, `StoryGenerator` | Business capability mixed with application cancellation and transport diagnostics | Separate domain signature vocabulary from execution evidence before moving the port |
| `cyoa-application/src/persistence.rs`, `GameRepository` | Load/save plus explicit prepared bytes and infrastructure-stage failures | Move business port inward; hide concrete write evidence and storage mechanics |
| `scripts/check_architecture.sh` | Inward crate dependencies, external dependency allowlists, backend-local placement | Retain direction; update checks for the chosen ownership, not exceptions to it |

Moving trait declarations alone would create core-to-application dependencies.
Every type in each port's signature must be classified before moving it.

## Ownership and dependencies

Keep the existing five crates initially. No evidence requires a new crate.

| Layer | Proposed modules/types | Owns neither |
| --- | --- | --- |
| `cyoa-core` | Existing game aggregates; `StoryGenerator`; `GameRepository`; business save identity, snapshot, revision and outcomes | Worker keys, transport buffers, prepared JSON, terminal state |
| `cyoa-application` | `AdventureSession`; command/query handlers; generation/storage execution contracts; session revision and operation keys; execution failure evidence | Thread handles, helper wire protocol, terminal commands |
| `cyoa-infrastructure` | Existing generation engine and backend adapters; repositories; generation/storage executors; retained save attempts | Canonical game ownership, command admission, UI messages |
| `cyoa-presentation` | Headless driver and future TUI; input mapping, views, terminal I/O | Save/retry policy, game mutation, execution resources |
| `cyoa-cli` | Construction of concrete adapters and application session | Use-case branching and game rules |

Dependencies remain presentation -> application -> domain and infrastructure ->
application/domain. Only the composition root wires presentation to infrastructure.
Tests must follow the same direction; fixture executors implement inward ports.

## Business ports

`StoryGenerator` continues to express outline, cast and turn generation using
domain inputs and validated outputs. `GameRepository` expresses saving, loading
and reconciling a particular save attempt. Retain create-versus-replace intent and
expected-version checks: a single save operation can take a `SaveDestination` enum
without erasing those distinctions. Backup selection remains explicit.

Move save snapshots, identity and revision vocabulary inward where needed.
Read-only listing/inspection views and their query port belong to application;
they do not require constructing or modifying the current session's aggregate.
Source compatibility and explicit original/current restoration rules remain checked.

Business ports must not import `cyoa-application`. Separate operation failures
from execution failures. Repository errors express unavailable/missing/invalid/
conflicting/unconfirmed saves; worker panic and helper-stage diagnostics are
carried in an application execution report. Infrastructure preserves exact
diagnostics and maps them into that report instead of adding stdout/stderr fields
to domain entities. Successful response diagnostics must survive too.

For generation, retain domain audit values (raw response and provenance) with the
validated generated value. Keep transport evidence in the execution report.
Introduce only the domain capabilities actually needed for cancellation observation
and tentative narrative observation; the existing cancellation token can implement
them, while subscription/wake machinery stays out of domain. The infrastructure
request adapter retains the concrete token, including its idle-process wake support.
Do not substitute periodic polling for the existing race-free notification path.

The implementation should establish a typed boundary mapping for every current
error category before changing the port, including missing response versus empty
response and cancellation after a candidate was received.

## Commands, queries and handlers

`AdventureSession` owns the canonical game and private lifecycle state. Handlers
are application methods or small cohesive handler structs, not a mandatory bus or
one public class per command. There is no second mutable copy in presentation.

Commands include submit brief, accept/replace outline, select protagonist, take
turn, retry generation, rewind, save, save-copy, load, cancel and finish session.
Queries expose story/lifecycle views, available actions, persistence status and
diagnostics. Capability views help presentation render controls; handlers still
validate admission when a command arrives. Views confer no mutation authority.

`AdvanceSession` is an application command that collects execution events and
applies completion handlers. It can change canonical state, so it is not disguised
as a query. Presentation calls it from its event loop, then runs read-only queries.
It returns promptly when work is still running and performs no disk I/O or live
thread join. No separate mutable controller is exposed through a query.

## Execution ports and concrete adapters

Application defines separate `GenerationExecution` and `StorageExecution` ports.
Each accepts owned typed requests, supports cancellation/closing and yields typed
progress/completion events. Start failure is explicit. Work carries an operation
key and base revision; event variants identify their operation and outcome.

Infrastructure implementations evolve from the existing runners. Generation
retains its generator between requests (required for demo behavior). Storage can
reconstruct a repository after failure without losing the previous save attempt.
Both retain resource ownership until joined; completion means work has been joined
and cleanup assessed. Polling never joins a live worker. Drop remains a cleanup
backstop, not evidence of success. No generic job framework is introduced.

Application decides which business operation to request and what follows its
outcome. Infrastructure invokes the business adapter and reports execution facts;
it never chooses to autosave, start an opening turn, retry inference or accept a
new canonical game. A common executor interface does not imply a common lifecycle.

## Opaque save attempts without losing evidence

The opaque attempt must represent a particular write, not just the current game.
Metadata and intended encoded bytes remain exactly those of that attempt.

Proposed representation: the repository port has an associated attempt type, and
the storage execution port exposes that type to application as an opaque value.
Infrastructure implements it with a cloneable handle whose private shared backing
owns the prepared document. Domain/application can retain or return the handle but
cannot inspect encoded bytes. This avoids a core-to-infrastructure dependency and
avoids a global string-token registry or unbounded process-lifetime archive.

The write contract requires publication of a recoverable attempt before the first
possible storage mutation. A typed attempt observer is required by the repository
write operation and supplied internally by the infrastructure executor; it carries
the associated opaque handle, never prepared bytes. The executor keeps an
independent capture outside the worker. A panic before publication means no
prepared mutation; a panic afterward retains an unconfirmed attempt in its report.
This preserves the current mandatory evidence sink's guarantee rather than turning
publication into optional constructor configuration.

Application stores the handle in the corresponding failed/uncertain state. A new
worker can reconcile through that handle after the old worker is gone. Handle
ownership, rather than worker lifetime, retains the private evidence. An earlier
uncertain attempt survives a failed save-copy and is released only when no live
operation, failure or retry state needs it. Successful attempts do not accumulate
indefinitely. The associated-type wiring and capture-before-mutation tests are
required to validate this representation during implementation.

No recovery runs after an acknowledged successful save. Explicit `/save` retries
an unconfirmed attempt; shutdown retains its existing bounded final-attempt policy.
Byte comparison, syncing, backup rotation, collision handling and conflict checks
remain infrastructure responsibilities with their current behavior.

## State and acceptance guarantees

Use private enums and checked transition methods. Keep genuinely independent
state separate; do not build one giant enum containing every cross-product.

- Lifecycle carries brief/outline/cast/game data appropriate to its stage.
- Active work variants carry their own request key, base revision and retry intent.
- Persistence distinguishes acknowledged revision, write in progress, known
  unsaved failure and unconfirmed save carrying its opaque attempt. A bare
  `Uncertain` flag without evidence is not constructible.
- A saving state retains prior persistence status, including an earlier uncertain
  attempt when save-copy is attempted. Binding changes only on accepted success.
- Closing variants carry the outstanding obligation: finish generation, finish
  current write, perform bounded final save, or complete with an outcome.
- Deferred opening is a continuation of successful initial saving, not an
  independent boolean that can start generation twice.
- Preserve separate generation and storage failures when both are meaningful.

Execution adapters implement trusted ports, not a security boundary against
malicious implementations. Private construction and typed requests prevent
accidental mismatches where practical; runtime checks still reject stale,
duplicate, wrong-kind, wrong-revision and cancelled results. Do not claim that
types alone prove event freshness or that a successful disk effect can be undone.

Retain the current owned-snapshot generation model. Only application acceptance
replaces canonical state; it never commits an already committed returned turn a
second time. No prepared-turn merge or concurrent editing is introduced here.

## Required transition behavior

| Trigger/outcome | Application action |
| --- | --- |
| Valid selection | Establish zero-turn canonical game, save, then start exactly one deferred opening on success |
| Generation preview | Update tentative progress only; no canonical change or save |
| Current successful generation | Check cancellation/key/revision, accept once, autosave accepted turn |
| Failed/cancelled/stale generation | Preserve canonical game and available evidence; no autosave of rejected content |
| Save success | Acknowledge only matching request/revision; bind if appropriate; satisfy any applicable final-save obligation |
| Save failure before mutation | Preserve game, leave it unsaved and block further game changes |
| Save outcome unconfirmed | Preserve game and opaque attempt; explicit save requests reconciliation |
| Failed save-copy | Preserve previous binding and any earlier uncertain attempt |
| Load failure | Preserve game, binding, generation retry and presentation input buffers |
| Accepted load | Advance session identity/revision, replace game/binding coherently, save required policy changes or recovered backup |
| Finish session | Join generation, finish/reconcile applicable write or make the bounded final attempt; preserve worker/I/O failure outcome |

Read-only operations, cancellation and finish remain available according to the
existing admission rules. No blanket permission expansion is inferred from CQRS.
Counter exhaustion must still reserve capacity for required continuations/final save.

Application completion excludes terminal output. After application reports its
generation/storage work complete, presentation drains output under the existing
absolute deadline and restores terminal state. Input/output failure requests safe
application completion but remains an exit failure even if the final save succeeds.

## Contract and verification impact

Primary architectural changes: ARCH-001 (session ownership, business/execution
port ownership and preparation publication), ARCH-002 (opaque evidence and state
construction), STREAM-001 (application acceptance versus presentation completion),
PRODUCT-001 (canonical ownership and execution placement), TEXT-001 (evidence
mapping). BACKENDS-001, LIMITS-001 and ACCEPTANCE-001 retain their behavior and
must continue to be exercised through the new orchestration.

Before implementation lands, update PLAN and those contracts with the approved
ownership change; revise ADRs 0002, 0003, 0011 and 0013 as appropriate. Keep current
coverage statuses honest. Move/rename tests with required-tests.json updates,
preserving assertions. Update handwritten mutation patches when their targets
move; stale/compiler-invalid patches still fail rather than count as detections.
New source in domain remains subject to its domain-only mutation gate; business
port movement is not permission to add threads, serialization or storage machinery.

Required implementation checks include application admission/transition tests,
opaque-attempt publication and lifetime through panic/fresh-worker/failed-copy
tests, cancellation wake and late-acceptance tests, unchanged public headless
stories, both adapter fixture paths, shutdown/terminal separation and compile-fail
examples for prohibited mutation/construction. Preserve exact audit/diagnostic
bytes and success/failure distinctions. Run the shared contract gate at
implementation completion; this design-only step uses documentation checks.

## Migration sequence

1. Inventory full business-port signatures and define inward vocabulary plus
   application execution reports; establish the new mappings without behavior changes.
2. Introduce opaque attempt handles and capture outside storage workers, with
   publication ordering and retention tests, before removing old evidence access.
3. Move business ports into domain and execution adapters into infrastructure;
   migrate their tests and update architecture checks without relaxing direction.
4. Move canonical lifecycle/persistence policy into application command/query
   handlers; establish legal state variants and preserve existing transitions.
5. Adapt headless presentation to commands, AdvanceSession and queries; separate
   terminal output completion from application session completion. Wire in main.
6. Remove superseded wrappers, update contracts/ADRs/test registry/mutations and
   complete behavior-preserving acceptance. Phase 3 then uses the same application seam.

These are dependency-ordered design stages, not yet self-contained implementation
tickets. Avoid maintaining two independent canonical session implementations while
migrating. Exact Rust signatures are to be finalized against compile-time ownership
and existing regressions; the responsibilities and required outcomes above govern them.
