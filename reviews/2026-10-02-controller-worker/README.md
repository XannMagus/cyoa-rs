# Controller and worker implementation evidence

Implementation follows `docs/plans/phase1-controller-and-worker.md` from `d22db81`.
Offline evidence only; headless commands, input shutdown and both live headless
gates remain pending.

## Step 1 — canonical lifecycle

New session tests ran against an incomplete controller whose completion method
returned `Ignored`: all four failed at runtime (expected `Failed`/`Committed`, and
retry remained unavailable). After implementing terminal acceptance they all pass.
The tests cover busy admission, exact failure evidence, cancellation of queued
success, fresh retry tokens/IDs, stale/duplicate results, wrong-kind faults,
edited outlines, checked selection, one committed opening, rewind and close.
Application rewind is extracted into a stateless command; the existing method
delegates without changing behavior. No controller/worker test is a live CLI claim.

## Step 2 — owned runner and runtime

The mailbox budget test was observed red against an unimplemented publisher
(empty output instead of `é雪`); cumulative, scalar-safe publication now passes.
Other thread tests were added after the runner and are green-first, not claimed
as observed TDD. One fixture error was repaired: an optional started notification
panicked after its receiver was deliberately dropped; notification now tolerates
an absent observer. Silent cancellation uses a registered wake and start handshake.
Tests exercise generator reuse, queued success followed by cancel, panic faults,
drop cancellation/join and a producer completing 10,000 callbacks before any poll.
Injected spawn failure retains use-case ownership for explicit retry. Overflow and
wrong-base-revision checks are separately tested. Presentation tests and warnings-
denied Clippy pass. The runtime owns the controller and runner; no input/terminal
thread exists yet.

## Step 3 — composed story

CLI-layer tests compose the real GenerationEngine, ChunkedBackend, application
use cases, runtime and controller. The existing frozen Phase 0 fixture is reused
unchanged. Nine requests cover outline/edit/cast, five accepted turns, malformed
output and a cancelled preview. Canonical equality, independent namesake IDs,
chapter retitling/bridge memory, null/empty threads, caps and captured next prompts
are asserted. Separate seeding tests cover both restore policies and zero NPCs.
These acceptance tests are green-first after integration; initial compiler errors
(wrong domain accessor/restore argument order) are not red-TDD evidence. The first
cancellation handshake paused too early (raw JSON before narrative); the fixture
now waits until a decoded narrative prefix has been supplied. Tests pass after
that fixture correction. `escargot` is a CLI dev-dependency only, fetched with
its lockfile for the next real-child composition step.

## Step 4 — both adapter boundaries and mutations

`cyoa-cli/tests/controller_backends.rs` composes both actual adapters with the same
existing Rust fixture binary, built via escargot in the active target directory.
For each peer: outline/cast/opening, candidate followed by exit 7, explicit retry,
silent cancellation, explicit retry and quit during a silent request. Assert one
auth child plus eight generation children, exact failure candidate/non-UTF-8
stderr, identical retry stdin, unchanged canonical game and absent PID/workspace.
A second test joins successful real-child outline work before requesting cancel;
the queued success cannot advance the canonical brief stage. This is synthetic
protocol evidence, not authenticated vendor behavior. These composed tests are
green-first; sensitivity is independently checked by persistent mutations.

Three new mutants target stale worker acceptance, cancellation disposition, and
committing an already-committed snapshot again. The original 31 are retained.
Compiler failures, absent tests and stale patches remain harness failures.
