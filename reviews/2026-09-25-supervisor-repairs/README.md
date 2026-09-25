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
