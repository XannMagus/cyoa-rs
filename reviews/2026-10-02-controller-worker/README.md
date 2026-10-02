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
