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
