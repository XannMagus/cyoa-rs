# Step 3 handoff: private Codex event state machine

Historical handoff, 2026-09-26. **Step 3 was implemented offline on 2026-09-27**;
see [the implementation record](../../reviews/2026-09-27-codex-step3/README.md).
The scope and obligations below were used for that slice. Next is adapter-plan
step 4 (transport reconciliation); neither vendor has an executable Backend.
Claude's own codec and adapter remain separate pending work.

Baseline: step-1 profile `70a669c` plus the step-2 implementation and
[review repairs](../../reviews/2026-09-26-codex-step2/review-and-repairs.md), committed
together with this handoff. Before starting, inspect subsequent history and the
working tree; do not attribute this existing work to the next implementing agent.
Read AGENTS.md, PLAN.md and the required contracts/registry/references first.
The [adapter plan](phase1-codex-adapter.md) and
[frozen profile](../../reference/02-codex-cli.md) govern behavior.

## Deliverable and ownership

Implement only step 3: a private, credential-free event state machine consuming
complete byte records and returning a candidate payload with protocol outcome.
Place it beneath `backends::codex_cli`, for example a private `protocol` module.
Keep vendor event DTOs and transitions inside infrastructure. Prefer a state enum
whose variants carry the required data; candidate existence is not success.

No process spawning, Backend::generate, transport reconciliation, auth preflight,
headless UI, supervisor changes, shared schema/wire/template changes or Claude
changes belong here. Do not expose arbitrary prompt/config overrides. Step 4 will
connect protocol outcome to the supervisor and own final response acceptance.
A codec must not construct a successful GenerationResponse or invoke on_json.

## Supported protocol

Use the exact policy in reference/02-codex-cli.md, including these obligations:

- Require thread.started with a nonempty identity, then turn.started, then exactly
  one completed agent_message with valid identity and string text, then one
  turn.completed. Retain decoded text byte-for-byte; validate payload JSON without
  trimming, concatenating or reserializing its evidence.
- Missing candidate/terminal, duplicate terminals (even identical), conflicting
  outcomes, wrong order, reused/invalid identities and multiple agent messages
  fail explicitly. A commentary message followed by JSON is ambiguous; do not
  select the first/last parseable JSON. Any failure invalidates candidate success.
- Reject unsupported event/item kinds, reasoning, tool execution and partial item
  events. Only extra fields on otherwise valid known envelopes are harmless.
  Top-level errors, including reconnect notices, fail this initial narrow profile;
  do not misdescribe every such notice as a terminal vendor failure.
- Invalid UTF-8, malformed JSON and invalid required fields are located protocol
  failures. At end-of-stream reject incomplete protocol state. Accept CRLF and a
  complete final record without newline using the supervisor's record contract;
  byte framing itself remains the supervisor's responsibility.
- Usage is optional telemetry. Distinguish unknown from zero; malformed telemetry
  becomes unknown. Use existing token normalization for invalid cached counts.
  Do not add auxiliary cache/reasoning counts into totals, fabricate cost or report
  a configured model as observed. Preserve any candidate on a later protocol
  failure for step 4's separate failure-evidence handling.

Protocol completion alone must never authorize acceptance: step 4 additionally
requires successful child exit, full stdin delivery, no cancellation/I/O/cap
failure, verified reaping and workspace cleanup. Do not redesign that future
failure API in this step.

## Error → edge → nominal tests

Use existing committed artifacts as independent inputs:
`reviews/2026-09-26-codex-profile/synthetic/expectations.json` and its 21 fixtures,
the successful live captures, and the explicitly unsupported live tool canary.
Read fixture provenance before assigning expectations. Do not regenerate golden
answers from the codec or call synthetic events live observations.

1. Error tests first: missing/failed/duplicate/conflicting terminal, truncated or
   invalid UTF-8/JSON, invalid identities/fields, wrong order, multiple messages,
   tools/unknown items, reconnect notices, and candidate retention after failure.
2. Edge tests: empty/invalid payload, exact whitespace/quotes/Unicode, harmless
   extra metadata, CRLF, final record without newline, missing/zero/malformed
   usage and cached greater than total. Exercise finish/end-of-stream explicitly.
3. Nominal tests: each supported live four-event transcript, exact decoded payload,
   independently specified usage, absent observed model/cost. No streaming claim.

Record meaningful runtime red before implementing each new behavior. Compiler or
fixture failures are setup errors; immediately passing tests are expanded coverage.
Retain actual logs. Preserve the existing preparation tests and eleven mutations.
Register real regressions under applicable ARCH-003, TEXT-001 and STREAM-001
contracts without promoting BACKENDS-001 to implemented. Step 6 owns the full new
codec mutation suite; if a focused mutation is added now, require passing baseline,
exact behavioral failure and passing restored code.

## Completion and next boundary

Run `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`, and
`bash scripts/check_contracts.sh`. Review complete callers and failure ownership,
not just green tests. Document supported/rejected cases and report affected IDs.
Stop for review before commit unless committing is separately authorized.

Step 4 will reconcile the codec with real process outcomes, audit candidate payload
retention in cancellation/timeout errors, and test the actual Backend with fixture
children. Those are future obligations, not step-3 completion evidence.
