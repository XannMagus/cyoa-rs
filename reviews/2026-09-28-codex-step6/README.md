# Codex adapter step 6: persistent regression mutations

Scope: step 6 of `docs/plans/phase1-codex-adapter.md`. This adds offline
regression protection; no live call was made and no production behavior changed.
Affected decisions: BACKENDS-001 and ARCH-003. The original eleven mutations,
including STREAM-001 and Phase 0 protections, remain required.

## What the gate checks

| Mutation | Exact registered regression |
| --- | --- |
| `accept-ineligible-message` | `codex_backend/ineligible_message_is_rejected_after_cleanup` |
| `accept-missing-terminal` | `codex_backend/candidate_without_terminal_is_rejected_after_cleanup` |
| `accept-conflicting-terminal` | `codex_backend/conflicting_terminal_is_rejected_after_cleanup` |
| `adapt-shared-schema` | `lib/generation::schema::tests::shared_turn_schema_keeps_optional_character_updates` |
| `adapt-peer-schema` | `lib/generation::backend_compat::claude_cli::tests::adapt_schema_strips_the_root_schema_key_and_nothing_else` |
| `discard-candidate-evidence` | `codex_backend/application_receives_complete_candidate_and_diagnostics_on_adapter_timeout` |
| `discard-diagnostic-evidence` | `codex_backend/application_receives_complete_candidate_and_diagnostics_on_adapter_timeout` |

The three protocol regressions exercise the actual adapter and supervisor with
frozen synthetic records, a three-second process bound, and a real fixture child.
They check the child's PID is gone and workspace is removed before the rejection
assertion. Unavailability, timeout and cleanup failures cannot satisfy the named
protocol assertion. Each mutant accepts a success-looking payload it should reject.

Schema mutations deliberately apply Codex's compatibility function in the shared
builder or Claude adapter, only in the isolated copy. Independent assertions check
optional SummaryUpdate character updates and exact Claude schema preservation.
Evidence mutations separately discard candidate text or diagnostics on timeout;
the application-facing test checks raw whitespace/Unicode and exact stdout/stderr,
with cleanup checked before the evidence assertions.

Each new manifest entry requires a nonempty behavioral assertion substring. The
runner requires the exact test, runtime-failure exit status, one failing test and
that substring. Compilation failures, missing tests, watchdog kills and unrelated
fixture/setup panics cannot count as detection. The outcome-checker self-test covers
these conditions. A passing baseline and restored test remain mandatory. Optional
`CYOA_MUTATION_EVIDENCE_DIR` retains all three logs, including failed attempts.

## Development evidence

These tests were green-first coverage of existing production behavior, followed
by deliberate mutation reds and restored greens, not a claim of production TDD.
The first full attempt rejected a surviving shared-schema mutant. The new test
had mistakenly inspected the root required list; character_updates belongs to
`$defs.SummaryUpdate`. The correction checks that definition and asserts the field
exists before checking optionality. The original survivor is retained in
[initial-shared-mutant.log](initial-shared-mutant.log) and the stopped gate in
[initial-survivor.log](initial-survivor.log). No production expectation was changed.
The protocol helper was also tightened before the final run to reject unrelated
transport failures without the behavioral marker.

Final validation: 243 passing workspace tests (including compile-fail examples),
all eighteen mutations detected with passing baseline/restored tests, formatting
clean, and warnings-denied Clippy clean. Final evidence is in
[contracts.log](contracts.log), [clippy.log](clippy.log),
and [mutations/](mutations/): each mutation has `baseline`, `mutant` and `restored`
logs. `baseline.log` records the initial fifteen adapter tests separately.

Reproduce:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
CYOA_MUTATION_EVIDENCE_DIR=/tmp/cyoa-step6-evidence bash scripts/check_contracts.sh
```

Step 7 remains the full authenticated adapter story/cancellation gate and peer
handoff. This record does not expand live evidence, Claude implementation status,
or playable headless/UI acceptance.
