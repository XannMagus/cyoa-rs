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

## Step 8 — regression gate and mutation evidence

The outcome-checker shell test lists compiler/command/survival errors first,
zero/ignored/wrong tests and contradictory exit status next, then one true failure
and one true baseline success. Against the initial permissive checker it failed
with `Incorrectly accepted: compiler failure`. Implementing exact outcome/exit/name
checks made the suite pass (`/tmp/cyoa-step8-outcome-red.log`). The manifest guard
also rejected an orchestration test registered under another decision but not yet
under IDENTITY-001; registering the actual lifecycle under identity resolved it.

The isolated runner then demonstrated all four intended behavioral failures:

| Mutation | Registered test | Regression exposed |
|---|---|---|
| Name-only world deduplication | `generation_use_cases::scripted_lifecycle_uses_edited_outline_and_preserves_namesake_ids_without_extra_calls` | Two distinct playable Ajaxes no longer satisfy the minimum |
| Duplicate opening request | Same lifecycle | Responses consumed out of order and an unexpected extra request |
| Default rather than active limits | `generation_use_cases::restored_limits_reach_captured_prompt_schema_and_committed_memory` | Restored cap missing from the captured request |
| Commit before reporting generation failure | `phase_zero_acceptance::full_story_preserves_contracts_through_failure_retry_cancellation_and_rewind` | Game differs from its complete pre-error snapshot |

Every baseline passed; every mutant compiled and failed exactly one selected test.
Logs: `/tmp/cyoa-step8-mutations.log`; reproduce with
`bash scripts/check_contract_mutations.sh`. The committed patches are explicit
reviewable examples of prohibited regressions. They never touch the working tree.
The shared CI/local gate additionally compares documented and registered coverage.
Existing test-presence, ignore and architecture checks remain in force.

Commit trace: step 4 `04e6ad1`, step 5 `bf29c3a`, step 6 `36ade4f`, step 7
`2e9f166`; step 8 is the commit containing this section. User's semantic override
validator decision is recorded as pending PROMPTS-003 and public arbitrary
configuration remains unavailable. No live-backend claim was added by these steps.

Final verification: 118 runtime tests and 7 compile-fail doctests pass, along with
formatting, Clippy with warnings denied, registry/coverage checks and architecture
checks. The mutation runner also reruns each selected test after restoring the
original file, requiring it to pass again; restoration is checked, not assumed.
