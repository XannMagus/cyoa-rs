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
