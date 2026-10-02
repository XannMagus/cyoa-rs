# Phase 1 — controller and worker implementation plan

Status: implemented offline through steps 1–5, with the completion handoff below.
Written 2026-10-02 against
`d22db81` on `phase-1/controller-and-worker`, after both adapters and Claude's
review repairs passed their gates. This expands item 6 of
[the Phase 1 sequence](phase1-headless-backends.md). Item 7 (headless/demo I/O) follows
it; item 9 remains a separate authenticated gate for each backend.

Read PLAN.md, the decision catalog and required-test registry first. Relevant
contracts: ARCH-001/002/003, STATE-001, STREAM-001, TEXT-001, BACKENDS-001,
ACCEPTANCE-001, IDENTITY-001/002, PROMPTS-001/003, LIMITS-001 and CHAPTER-001.
This plan refines existing decisions; it does not change game behavior.
The implemented tests are registered in the decision catalog. See the
[implementation evidence](../../reviews/2026-10-02-controller-worker/README.md).

Implementation commits: `a5cfd72` (controller), `641f9ba` (worker/runtime),
`6e2bec0` (composed story), `0619a38` (both adapter fixtures and mutations).
Public runtime intent lives in `runtime.rs` alongside `session.rs` and `worker.rs`.
Five originally planned worker TDD cases were added green-first; the evidence
record distinguishes those from observed red/green controller/mailbox tests.
The retained mutation baseline is now 34, including three presentation regressions.
Headless parsing, stoppable stdin/signal handling, demo and both live headless gates
remain pending. No new authenticated inference was run for this slice.

## Deliverable and boundary

Build a terminal-independent session controller and a reusable synchronous worker
runner in `cyoa-presentation`. The controller drives brief → outline review → cast
selection → opening → play, retains canonical state, and accepts a worker result
only when its request and base revision remain eligible. The runner moves one
`StoryUseCases<G>` into one request thread and recovers it after joining. Both
streaming and complete-only generators follow the same path.

This slice stops before a playable binary: no stdin reader, terminal rendering,
signal handler, concrete backend selection, demo loader, save adapter or TUI. A
runtime test driver exercises the controller/runner together. Headless I/O is the
next slice and must use this same controller, rather than reimplementing it.

Existing use cases are sufficient. `take_turn` mutates a worker-owned snapshot
and returns `Result<(), GenerationFailure>`; the returned snapshot already has its
turn committed. Outline generation returns `Generated<WorldOutline>`; cast generation
returns `World`. Do not add another JSON port or call `StoryGenerator` directly to
bypass these use cases. Existing success types do not expose all transport evidence
(in particular `generate_world` consumes the cast's generation attachment); this
slice must not claim new success telemetry. Failure evidence stays intact.

One small application refinement may be needed for local rewind: extract a
vendor-neutral stateless rewind command helper and keep `StoryUseCases::rewind`
delegating to it. The controller can then invoke that application command while
the runner retains its ready generator. Preserve all rewind behavior and registered
tests; do not create a second generator merely to perform a local command.

## Code homes and dependencies

| Home | Responsibility |
|---|---|
| `cyoa-presentation/src/session.rs` | Controller, typed intents/events, lifecycle and read-only views |
| `cyoa-presentation/src/worker.rs` | Owned request execution, thread lifecycle and bounded progress handoff |
| `cyoa-presentation/tests/session.rs` | Deterministic controller transitions with synthetic typed outcomes |
| `cyoa-presentation/tests/worker.rs` | Real threads with inward-owned fake generators and handshake controls |
| `cyoa-cli/tests/controller_backends.rs` | Compose controller/runner with each actual adapter and the existing fixture child |
| `docs/decisions/required-tests.json` | Register implemented tests under the owning decisions |

Add `cyoa-core` as a direct dependency of presentation for domain inputs/views;
the architecture gate already permits this. Presentation imports only application
and domain types. Concrete engines, adapters and fixture scenarios stay in
`cyoa-cli` cross-layer tests and eventual `main.rs` wiring. Use `escargot` in CLI
dev-dependencies as already specified in the Phase 1 plan to build the existing
fixture binary with `--locked --offline`, respecting `CARGO_TARGET_DIR`.
Fetch any new locked dependency once before requiring the offline gate.

## Ownership and execution

The event-loop thread owns `SessionController` and `WorkerRunner<G>`. No shared
mutex protects the canonical game, and the worker has no reference to it.

1. Validate intent on the controller; allocate a checked `RequestId`, record the
   current `SessionRevision`, create a fresh cancellation source, and produce an
   owned typed work request. Busy requests reject synchronously without queueing.
2. The runner transfers its ready use cases to a new thread together with that
   request and an observation-only token. Only this thread touches the generator.
3. The worker runs exactly one existing generation use case. Turn work receives
   a clone of canonical `GameState`, applies `take_turn` to it, and returns that
   snapshot only on success. Outline/cast work receives owned brief/outline/limits.
4. Progress callbacks publish decoded narrative only into the request mailbox;
   they never render or block waiting for the event loop.
5. The thread returns the use cases and one typed terminal outcome. The runner
   checks `JoinHandle::is_finished()` before joining on the event-loop thread;
   a completion notification alone is not permission for a blocking join.
6. After join, recover use-case ownership first, then deliver the outcome to the
   controller. Only now may the runner accept another request. Adapter return
   already entails child reap/workspace cleanup; joining also proves worker exit.

Prefer `thread::Builder::spawn` so thread creation failure is an explicit outcome.
Keep the ready use cases recoverable on spawn failure (an owned handoff cell,
taken by the spawned thread, is acceptable); do not lose the generator inside a
dropped failed-spawn closure. A handoff cell is not shared canonical game state.

Worker states are `Ready(use_cases)`, `Running(handle, request metadata,
mailbox)` and `Faulted(worker_failure)`. Request threads are sequential, not a
pool. This preserves the existing mutable backend across retries without creating
a second inference worker or repeating auth preflight on every normal request.

A thread panic becomes a distinct worker failure when joined. Preserve canonical
state; mark the runner faulted because use-case ownership is lost. Do not fabricate
backend diagnostic bytes or offer an ordinary retry on that destroyed generator.
Shutdown remains available. Recovery/reconnection is future explicit work.

## Types, lifecycle and admission rules

Use separate private-field checked `RequestId` and `SessionRevision` newtypes
(monotonic `u64`, checked increment, no wrap/reuse). Overflow rejects the transition
before it changes state or starts work. A request key contains both values.
Revisions track accepted canonical changes, not turn count: rewinding must not reuse
a revision. Request IDs advance on every new attempt, including explicit retries.

Keep orthogonal concerns in enums with required data:

- Canonical stage: `Brief`, `OutlineReview { brief, outline, limits, style }`,
  `CastSelection { brief, world, limits, style }`, `Playing { game }`.
- Operation: `Ready`, `Running { pending }`, `Cancelling { pending }`,
  `Failed { retry_intent, failure }`, `Faulted { failure }`, `Closing { ... }`,
  `Closed`. A pending request owns its key, source, typed expected outcome and
  original retry intent; never infer these from screen text.
- Work: generate outline, generate cast, take turn; matching terminal outcomes
  carry an outline, world, already-updated game snapshot, or `GenerationFailure`.
  Wrong-kind completion is a protocol fault, never an unchecked cast or commit.

Controller transitions are inherent methods returning typed effects/outcomes;
effects contain work dispatch/cancellation/shutdown intent, not terminal calls.
Runtime interprets effects. Queries expose borrowed canonical views and separate
tentative progress. No public mutable access to canonical state.

| Intent | Admission and resulting behavior |
|---|---|
| Submit brief | Validate nonblank input, start outline generation; success enters outline review |
| Replace outline | Only during outline review and when ready; construct checked title/description, advance revision, no inference |
| Accept outline | Start one cast request using the accepted/edited outline and retained limits/style |
| Select protagonist | Only during cast selection; use `World::select`; invalid position changes nothing and makes no call |
| Start opening | Explicit controller intent after selection; one `Continue` request on the zero-turn game |
| Continue/player input/interesting event | Only in play when ready; one typed turn request |
| Choose quick action | Resolve checked ordinal from the current committed turn; use identical `Player` path with its text |
| Retry | Only after a generation failure/cancellation and after join; replay original typed intent with unchanged base state and a fresh key/source |
| Cancel | Running → Cancelling, clear visible preview, signal source; repeated cancel and idle cancel are harmless |
| Rewind | Only in play when ready; reject while busy, use existing domain/application behavior, advance revision on success |
| Quit | Reject further work, cancel outstanding request, stay Closing until worker joined, then Closed |

A failed request leaves its previous canonical stage exactly intact. Failure UI
data does not replace the game. `GenerationFailure` is moved intact (including
exact candidate and byte diagnostics), not reconstructed from `Display` text.
Choosing another valid intent after failure discards the old retry intent only
when that new intent is admitted. Blank player input means Continue at the later
headless parsing boundary, not an invalid `PlayerInput`.

Normal edits, selection changes and rewind are rejected while busy for this phase.
A terminal event is accepted only if operation is Running, its key equals the
active key, its base revision equals the canonical revision, its result kind
matches, and cancellation has not been observed. Then replace the applicable
canonical stage, advance revision and remove the active request. A returned game
snapshot replaces the canonical game; never call `commit_turn` again.

Wrong request/revision events and duplicate terminal/progress events are ignored
without modifying the current request. Defensive key checks remain even though
production has one worker: tests inject obsolete events directly, and future
screen/session replacement must retain this boundary.

## Cancellation races and shutdown

Event-loop order defines acceptance. A result already accepted before a cancel
intent remains committed; later idle cancel does not undo it. If cancel is accepted
first, the transition to Cancelling invalidates success even if the worker has
already queued that success. Discard any returned success snapshot; keep the prior
canonical state. Store any available rejected-success audit data separately from
canonical state rather than pretending the backend returned a cancelled error.
Backend failures retain their original diagnostics/classification, with the UI's
cancellation disposition represented separately when needed.

Cancelling is not Ready: no retry, rewind or new generation until join. After join,
return to a failure/cancelled operation with the original retry intent available.
A late cancelled result cannot become eligible merely because the next request
has begun. Quitting never enables retry. Dropping the runtime cancels active work
and joins as a cleanup backstop; provide an explicit shutdown path for normal use.

Production adapters have finite deadlines/cleanup bounds. Rust cannot preempt an
arbitrary stuck generator or user callback; do not invent a guaranteed bounded
join for all `G`. The runner supplies nonblocking callbacks, and real adapter tests
establish actual cancellation/cleanup. Fake generators must cooperate with tokens
and use bounded test watchdogs. Runtime panic/disconnection is observable.

The later headless loop must avoid a forever-blocked stdin thread: use stoppable
Linux readiness-based input, incremental reads and a finite event-loop poll interval
(target 50 ms), not `read_line` on the controller thread. A signal handler only
notifies the loop; cancellation and shutdown happen in ordinary code. EOF, stdin
error, stdout failure and quit all enter the same Closing path. Implement and test
that OS input/signal seam in item 7, not claim it solved by worker tests here.

## Bounded preview handoff

Use one request-scoped, mutex-protected append-only narrative mailbox and a
take/drain operation. The mutex protects progress only; never hold it during
generation, canonical transitions, terminal output or join. Callbacks hold it only
for bounded copying, not channel backpressure. Terminal results travel through
the thread return value, so a full preview path cannot lose completion.

Introduce checked presentation preview limits, independent of domain `Limits` and
subprocess capture bounds. Initial proposal: at most 1 MiB of retained preview per
request, counted across delivered and undelivered text. Check additions without
overflow and never split a UTF-8 scalar. On exhaustion, stop accepting previews
and record an explicit incomplete-preview marker; generation continues and final
validated narrative still commits normally. No silent truncation or invented
generation failure. A slow consumer can therefore consume at most the same bounded
preview budget. Only one request exists, so buffers cannot accumulate by request.

Controller drains/coalesces progress per loop iteration, checks its key, and ignores
it while Cancelling/Closing or after terminal acceptance. Terminal processing drains
or discards the remaining mailbox deterministically before ending the request.
Authoritative prose remains the returned game; normalization and disagreement are
expected. Headless item 7 will mark tentative output and print a corrected/full
passage on disagreement or incomplete preview, otherwise avoid duplicate prose.

## Atomic implementation sequence

Each row is a passing commit, with actual error → edge → nominal red/green evidence,
new registry entries, documentation and relevant contract tests. Do not leave
intentionally red commits or pre-register nonexistent tests.

| Step | Change | Acceptance evidence |
|---|---|---|
| 1 | Typed controller lifecycle, keys/revisions, synthetic terminal acceptance, borrowed views | Reject overlap/wrong stage/wrong result kind; stale and duplicate events; edited outline and checked selection; no double turn commit |
| 2 | Owned runner, nonblocking polling/join, bounded progress mailbox | Spawn failure retains generator; panic faults runner; cancellation while silent; queued-success race; retry gets fresh token and same generator; paused consumer cannot block cleanup |
| 3 | Compose controller + runner across complete synthetic story | Outline/edit/cast/opening/continuation/chapter/retitle/failure/retry/cancel/rewind; full state equality on failure; exact requests/counts and evidence |
| 4 | Cross-layer fixture tests for both concrete backends and persistent presentation mutations | Actual adapter cleanup through controller; stale-success/cancelled-success/no-double-commit regressions; all prior 31 mutations retained |
| 5 | Document completion and hand off to headless/demo implementation | fmt, Clippy, full contract gate; truthful implemented/pending matrix; exact headless input/output/signal seam left for item 7 |

Controller tests should inject events without real time. Thread tests coordinate
with start/progress/release handshakes, not sleeps proving ordering. Establish both
orders of the success/cancel race and ensure the fake worker is always released
and joined even if assertions fail. watchdog failures are harness failures, not
detected behavioral mutations. Use dependency injection for spawn-failure testing.

In step 3, freeze independent expectations from the existing Phase 0 scenario; reuse
or version its data without changing existing assertions. Cover both restoration
policies and zero NPCs through an internal typed test constructor/session seed,
without adding a public save/load command. Compare canonical IDs, limits, summary,
chapter titles and next-request context, not only printed narrative. Progress must
be tentative even for complete-only responses.

Step 4 belongs in CLI tests because presentation cannot depend on infrastructure,
even for tests. Use the one existing fixture executable for Claude and Codex; assert
direct PID/workspace absence and one generation child per intent. Auth preflight is
counted separately. Verify idle cancellation, candidate/nonzero failure and returned
success queued before cancel. Pure controller tests additionally inject stale keys
and base revisions. Mutations must remove the actual eligibility checks; their
registered tests must fail behaviorally, pass before/after restoration, and terminate
with cleanup. Retain existing mutation semantics when patches move.

## Completion and following work

The slice is done when controller ownership, cancellation disposition, exact failure
evidence, safe worker reuse, bounded progress and stale-result rejection are exercised
through their real callers and the full gate is green. PRODUCT-001 may gain partial
controller coverage only; TUI, disk persistence and exports stay pending. Update
STREAM-001's presentation coverage precisely; do not mark headless acceptance complete.
PROMPTS-003 remains pending and bundled-only construction remains in place.

Then implement item 7's parsed `play --headless --backend <claude|codex>` or `--demo`,
memory-only lifecycle, outline editing, action/retry/cancel commands, stoppable input,
separate story/control output, preview reconciliation and binary acceptance. No
implicit backend choice/fallback, automatic retry or session persistence is added.
Follow with each backend's shipped-command live gate. Controller work does not
require spending subscription quota; existing adapter evidence is not relabeled
as a new headless result.
