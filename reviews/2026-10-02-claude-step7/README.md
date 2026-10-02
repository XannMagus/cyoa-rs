# Claude adapter step 7 — persistent behavioral mutations

Baseline `60bdd15`. Adds eleven mutation patches (29 total; the 18 existing ones are
untouched and still detected), their manifest entries, the updated required-id list
and message requirement in `scripts/check_contract_mutations.sh`, and a few
distinguishing assertion messages in existing Claude tests. No production behavior
changed. Affected decisions: STREAM-001, BACKENDS-001, TEXT-001, ARCH-003.

## The mutants

| Mutant | Decision | Exact test | Must fail with |
|---|---|---|---|
| forward-uncorrelated-preview | STREAM-001 | lib `…protocol::tests::frozen_completed_variants_emit_expected_previews_payload_usage_and_provenance` | `correlated preview only` |
| share-block-indices-across-messages | STREAM-001 | same test | `enforced-payload-in-second-message` |
| ignore-is-error-result | BACKENDS-001 | `claude_backend::is_error_result_is_rejected_after_cleanup` | `protocol acceptance regression` |
| accept-stream-without-result | BACKENDS-001 | `claude_backend::stream_without_a_result_is_rejected_after_cleanup` | same |
| accept-duplicate-result | BACKENDS-001 | `claude_backend::duplicate_result_is_rejected_after_cleanup` | same |
| result-overrides-process-failure | BACKENDS-001 | `claude_backend::result_cannot_override_nonzero_exit_and_previews_are_tentative_evidence` | `process failure override regression` |
| accept-api-key-auth | BACKENDS-001 | `claude_backend::preflight_rejects_non_subscription_unknown_and_conflicting_auth_without_generation` | `auth acceptance regression` |
| discard-claude-candidate-evidence | BACKENDS-001 | `claude_backend::application_receives_candidate_and_diagnostics_on_adapter_timeout` | `candidate evidence regression` |
| discard-claude-diagnostic-evidence | BACKENDS-001 | same test | `diagnostic evidence regression` |
| reserialize-structured-output | TEXT-001 | lib `…protocol::tests::result_payload_is_the_exact_span_not_a_reserialization` | `exact span regression` |
| leak-advisor-into-shared-instructions | ARCH-003 | `claude_story_acceptance::composed_story_preserves_namesakes_repaired_ids_retitling_and_rewind_context` | `shared instructions leak regression` |

Protocol-acceptance mutants (the three `protocol acceptance regression` rows) check
child/workspace cleanup before the behavioral assertion, so a surviving mutant cannot
hide a leaked child. Each patch was produced by applying one targeted edit to the
tree, capturing `git diff` and restoring the file; each applies with `git apply
--check`.

## A mutant survived, and what it showed

The first full run detected ten mutants and **missed
`leak-advisor-into-shared-instructions`**: the advisor-locality unit test passed with
the suppression text appended to shared instructions. Cause: the patched line builds
only *turn* instructions, but the test inspected the world request (plus the turn
schema). That was a real gap in the test, not a bad mutant. Fix: the unit test now
also checks the cast request, and the mutant is bound to the composed acceptance test,
which now asserts that no instructions or prompt of any captured request (outline,
cast, opening, continuations) contain advisor text. Re-running the gate detects it.

## Why this step matters for steps 5 and 6

The adapter and composed tests were written after the implementation, so they had no
red phase. These mutants are the evidence that the tests are sensitive: each
regression a Claude-specific acceptance mistake could cause is caught by the named
test, including the incidental one (index sharing) that the live enforce-retry
capture motivated.

## Verification

`CYOA_MUTATION_EVIDENCE_DIR` retained baseline/mutant/restored logs for every mutant
in `mutation-evidence/`. fmt, warnings-denied Clippy and the full gate pass:
`contracts.log`. Offline adapter protections only; no live-backend claim.
