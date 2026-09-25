# Repairs following the process-supervisor review

Baseline: `f1f3547`. Each repair is committed separately; historical review probes
remain unchanged in their review directory. No new live-backend claim is made.

## R4 — diagnostics and provenance

Error-first: validation failures for outline/cast/turn responses lost attached
transport bytes (`validation_failures_retain_transport_diagnostics_for_every_request_kind`,
observed runtime red). Fixed `invalid()` to copy evidence. Added edge coverage for
cancellation at the application boundary after generation, then nominal metadata
propagation: both failed with empty diagnostics before implementing the mapping.
Generated results now carry evidence through that final cancellation check and
observed provider/model provenance reaches turn records. Unknown metadata remains
unknown; no backend/model is inferred.

Moved the diagnostic value and its existing byte-preservation regression from domain
to the application-owned port boundary, updating the registry. No story rules changed.
Logs: `/tmp/cyoa-fix-diagnostics-red.log`, `/tmp/cyoa-fix-evidence-red.log`.

## R2/R3 — authoritative input delivery and cancellation

The promoted incomplete-input and delayed-notifier probes each failed at runtime
against the old supervisor (`/tmp/cyoa-fix-{input,cancel}-red.log`). Zero child exit
now also requires every input byte to have reached the pipe; the corrected existing
broken-stdin regression expects failure, not success. A child need not prove it
semantically read already-delivered bytes. Cancellation reads the token even when
its notification is delayed, and checks it before accepting empty-output success.
The pipe remains a wake optimization, not the source of cancellation truth.

## R1/R5 — finite capture and explicit I/O/cleanup outcomes

The 100-byte cap regression failed at runtime by retaining 4,096 bytes, and a
write-only descriptor's fatal read was incorrectly accepted as would-block
(`/tmp/cyoa-fix-bound-red.log`, `/tmp/cyoa-fix-io-red.log`). Both pass after the
supervisor rewrite. Reads do at most 8 KiB per polling iteration, retain only the
configured prefix, and distinguish EOF, would-block/interruption, overflow and
fatal I/O. Diagnostic captures explicitly distinguish complete bytes from a prefix.
The record framer uses offsets into the owned capture, avoiding duplicate stdout
accumulation and the old discarded stderr copy. Resource mutation is encapsulated
in owning types rather than free helpers mutating caller-owned buffers.

Shutdown signals the group before collecting its tail, uses the same byte caps and
a separate finite capture/reap grace, and reports cleanup failures together with
the initiating error. Exit observation uses `waitid(WNOWAIT)` so the direct child's
PID remains reserved until group signaling; only then is it reaped. Drop is a
best-effort unwinding backstop, never normal-path evidence of successful cleanup.

Additional fault-seam tests exercise nonblocking setup, read, write, poll, exit
observation, kill and reap failures. Exact-bound arbitrary-byte capture and producer
cancellation/deadline tests were added after the rewrite and were initially green;
these are expanded verification, **not claimed as red-first TDD**. The first
unpaced deadline fixture hit its byte bound before the deadline; pacing that case
makes the intended deadline assertion independent of machine throughput. The
cancellation fixture remains unpaced. Existing framing regressions now exercise
the owning offset framer. No vendor protocol or story behavior changed.

## R3 follow-up — notifier lifetime and panic policy

Two runtime-red cases established that one panicking observer skipped later wake
callbacks and that a scoped subscription implemented using the old persistent
registration retained captured resources after scope exit. Implemented removable
registration ownership; the supervisor holds its registration only while running.
The flag is set before notification and remains authoritative. Notifications run
outside the mutex; all callbacks are attempted, then the first panic resumes.
Dropping a registration does not retract a callback cancellation already took.
Callers must keep callbacks short; a blocking callback cannot hide cancellation
from the supervisor's flag checks. Edge coverage for slot reuse, reentrant callback
registration and late subscriptions was added after the error fixes and passed on
its first run. Logs: `/tmp/cyoa-cancellation-lifetime-{red,green}.log`.

## Isolation and real cleanup evidence

The child inherited the repo cwd in an observed failing test; it now starts in a
private, unique `RequestWorkspace` owned by its `ProcessSpec`. `run` consumes that
spec, preserving any prepared schema files until process cleanup and explicitly
reporting directory cleanup failure. RAII covers panic unwinding. A second observed
red test caught tempfile's default directory permissions; construction now requests
Unix mode 0700. `tempfile` owns directory creation/removal rather than introducing
project-specific temporary-directory tooling. Logs: `/tmp/cyoa-workspace-red.log`,
`/tmp/cyoa-workspace-permissions-red.log`.

Added post-fix coverage for file lifetime/removal after success, nonzero exit,
consumer rejection/panic, cap, timeout, pre-cancellation and spawn failure. The
existing timeout test now obtains the child's PID through a handshake report and
asserts it disappeared. All process-existence assertions query the kernel with
signal 0; absent Linux procfs cannot produce a false pass. Runtime evidence in this
repair session is Linux only. Unix platforms lacking rustix's WNOWAIT waitid API
return Unsupported instead of compiling a weaker cleanup path.

## Persistent process mutations

The shared gate now requires eleven mutations: the original four Phase 0 cases
plus cap overflow, accepting incomplete input, ignoring the cancelled flag,
accepting a failed process, swallowing a fatal reader error, disabling the wake
subscription, and discarding the initiating cleanup cause. Each must compile and
fail its exact registered test between passing baseline/restored runs. The runner
now supports exact library unit tests as well as integration targets.

A controlled test temporarily uses a two-second poll interval and signals a
canceller only after the token check immediately preceding the poll. With silent
pipes, the wake must return within one second; the normal 25 ms fallback and all
authoritative flag checks remain unchanged. This test was added green-first and
then proved red through the retained mutation, not presented as earlier TDD.
The initial mutation merely removed the write and correctly survived: destruction
of the callback closed the only writer, producing POLLHUP, another valid wake.
The final mutation removes registration and holds the writer open, genuinely
removing wake delivery. All eleven runtime mutations were detected in
`/tmp/cyoa-fix-six-mutations.log`; compiler errors never count.

## Cleanup audit follow-up — interrupted reaping

A final audit found that repeated EINTR could bypass the reap deadline because its
check lived only in the no-exit branch. A bounded injection returning interruptions
for longer than the grace period reproduced a runtime failure: the old loop later
returned success. The deadline now applies before every attempt, regardless of the
previous result. The regression also checks that the Drop backstop reaps the actual
child after the explicit timeout. Log: `/tmp/cyoa-reap-interrupt-red.log`.

## Fixture audit follow-up — success diagnostics

The test-only `FixtureBackend` still dropped its locally captured diagnostics on
successful JSON decoding. A real-child regression reproduced empty stdout/stderr
on that path; success now attaches both streams as failures already did. This
keeps the test adapter an accurate example for future vendor adapters rather than
teaching the evidence-loss bug. Log: `/tmp/cyoa-fixture-diagnostics-red.log`.
