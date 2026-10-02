# Claude adapter review repairs — 2026-10-02

Two P2 findings from an independent (Codex) review of the pushed
`phase1-claude-adapter` branch, both confirmed against the code before any change.
Affected decisions: BACKENDS-001 (and STREAM-001/TEXT-001 for the codec tests).
Each repair is its own commit with its own tests and mutation protection.

## Repair 1 — malformed result metadata discarded the candidate

**Finding.** `result()` validated `is_error` and `subtype` before extracting
`structured_output`, so a result with a valid payload but missing `is_error` was
rejected with `candidate: None`. Transport bytes survived in the diagnostics, but the
application's separate candidate field was lost.

**Fix.** Extract and retain the exact span first; validate the metadata afterwards with
rejections that carry it. A result without an object payload still has no candidate.

**TDD.** Two tests were written first and observed red on the unfixed code: a codec
unit test (`left: None`, expected the exact span, for missing/non-bool `is_error` and
missing/blank `subtype`) and a real-child test asserting the application-visible
`raw_response` (`""` instead of the payload). Both pass after the fix. The existing
`ignore-is-error-result` mutation patch no longer applied (its context moved) and was
regenerated with the same one-line edit; a new mutation,
`drop-candidate-on-malformed-metadata`, restores the old behavior at the `is_error`
check and is detected by the real-child test.

**A mistake found along the way, in my own earlier test.** The step-4 test
`result_payload_is_the_exact_span_not_a_reserialization` was meant to include a
six-character JSON unicode escape (backslash, `u`, `00e9`) that a compact serializer
would rewrite to the single character it denotes. The escape as I typed it was
decoded into that literal character by the tool layer that wrote the file (confirmed
from the committed bytes: the two-byte UTF-8 sequence, not six ASCII bytes), so that
part of the test never ran; the odd spacing in the same span still
proved exactness and the `reserialize-structured-output` mutant was genuinely
detected, but the comment overstated what was tested. Both spans now build the
backslash from a char (`'\\'`) with an assertion that the escape is really present.
