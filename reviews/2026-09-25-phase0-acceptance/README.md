# Remaining Phase 0 TDD record

## Step 5 — typed application generation and scripted transport

`generation_use_cases` first ran with three failing error tests: cancellation,
failed-turn diagnostics/state/single attempt, and invalid outline/insufficient cast.
The minimal use-case/adapter scaffolding returned unavailable; the implementation
added inward-owned ports, one-call rendering/decoding/validation and success-only
commit. The same three tests then passed.

Next the restore-policy edge test failed because scripted replay was unavailable.
The nominal full lifecycle test was added before implementing replay; both failed
at runtime (3 existing passed, 2 failed). Implementing the reusable scripted
transport made all five pass. Early compile errors in fixture imports/accessors
were corrected before recording runtime red results. The temporary local test
transport was then replaced with the reusable implementation for all five cases.

Local run logs: `/tmp/cyoa-step5-errors-red.log`, `errors-green.log`, `edge-red.log`,
`nominal-red.log`, and `green.log` (each with the same `cyoa-step5-` prefix).
Assertions concern captured requests and complete state, not calls to domain merge
in lieu of the adapter. Full streaming and adversarial acceptance follow separately.

## Step 6 — streaming and chunk replay

Error cases first: truncated/invalid scanner preview and preview followed by a
transport failure both failed (empty preview). Edge cases then failed for escaped
surrogate/scalar strings and top-level versus nested selection. The nominal
arbitrary-split test failed too. Implementing the enum-based scanner and forwarding
decoded previews through the adapter made all four scanner tests and the transport
failure test pass. The scanner accepts valid UTF-8 chunks and is not a JSON validator.

The cancellation-after-preview test then failed because replay was still complete
only (diagnostics contained the entire response). The chunked/complete equivalence
test failed because it received only one preview callback. Implementing seeded
scalar chunk replay made both pass, including 32 seeds, no duplicate final preview,
unchanged state on cancellation and preservation of the observed raw prefix.
Logs use `/tmp/cyoa-step6-{errors,edges,nominal,cancel,chunks}-red.log` and
`/tmp/cyoa-step6-all-green.log`. No new dependency or real CLI invocation was needed.

## Step 7 — complete story and application rewind

Added tests in error → edge → nominal order. The new rewind command initially
returned success without changing state. The invalid-rewind test failed at its
error assertion. The first edge/nominal runs exposed a fixture omission
(`current_situation` is required); those runs are **not** behavioral red evidence.
After correcting the fixture, all three tests failed at their intended rewind
assertions: invalid count accepted, last turn retained, and two rewound turns
retained. Delegating to `GameState::rewind` made all three pass. No established
production behavior was disabled to obtain a failure.

The full scenario independently checks five successful turns, nine total requests,
preview then malformed output, explicit retry, preview then cancellation, same-name
NPC updates by repaired ID, unknown kind recovery, retitling, chapter bridge,
newest events, null/empty threads, unchanged state on failures and exact request
context after rewind. Fixture data is versioned and not regenerated from Python.
Logs: `/tmp/cyoa-step7-corrected-red.log` and `/tmp/cyoa-step7-green.log` (session
artifacts); the stable reproduction is `cargo test -p cyoa-infrastructure --test
phase_zero_acceptance --locked --offline`. Registered under ACCEPTANCE-001.
