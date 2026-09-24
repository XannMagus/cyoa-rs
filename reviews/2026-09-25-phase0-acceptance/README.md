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
