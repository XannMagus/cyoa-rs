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
