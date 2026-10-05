# Repository review repairs — 2026-10-05

Repairs for all three findings on reviewed baseline `e3da6b7`. Affected decisions:
BACKENDS-001, ARCH-002, ARCH-003, PRODUCT-001, STREAM-001 and TEXT-001 for retained
evidence. No domain expectation or shared wire/schema/prompt behavior changes.

## R1: ambiguous Codex control records

The Codex-private codec checks unique decoded keys in the record and its immediate
`item`, `error` and `usage` control objects before interpreting any event fields.
Repeated identical values and escaped spellings of the same key also reject with
a located `DuplicateField` error. Unknown nested metadata and the opaque model
payload string remain outside this check. A previous candidate survives rejection;
an ambiguous item never creates a candidate. No emission authorizes success.

`duplicate_control_keys_reject_without_emission_and_preserve_prior_candidate`
drives real fixture children through the actual adapter. Eight cases cover
conflicting/identical terminal types, usage/error keys, duplicate item types/IDs/
text, and escaped keys. It checks rejection, zero emissions, exact candidate and
stdout/stderr bytes, scratch-directory removal and kernel PID disappearance.

## R2: ambiguous Claude authentication

The Claude-private preflight uses a typed status DTO. Serde rejects duplicate
known `loggedIn`, `authMethod` and `apiProvider` fields instead of silently keeping
the last value. Missing/mistyped values and non-subscription status remain rejected;
unrelated identity/configuration fields remain tolerated. No generation launches
without unambiguous subscription evidence.

`duplicate_auth_keys_fail_closed_before_generation_and_retain_status` drives
seven conflicting, identical and escaped duplicate-key statuses through real
auth children. It checks Unavailable, exact status bytes and an auth-only launch
log. This is synthetic admission evidence; Claude was not exercised live here.

## R3: input line limits and read-ahead

Terminal input bounds the first complete line before consuming it, and bounds
the whole buffer only when no LF exists. The 64 KiB limit counts each line's bytes
including CR/LF delimiters when present, independently of later buffered lines or read
alignment. Buffered lines and EOF tails pass through the same complete-line check.

`input_line_bound_ignores_read_ahead_and_rejects_oversized_lines` runs the public
demo binary with a regular input file: a preceding short `/help`, a padded near-
or exact-limit command, and trailing `/inspect` and `/quit`. It checks exact command
counts, successful close and inspection. Complete lines of 65,536 bytes succeed;
65,537-byte lines reject, both with LF and as EOF tails. No vendor access.

## Regression and mutation evidence

All three new tests were run before the fixes and failed their intended behavioral
assertions: ambiguous Codex control accepted, ambiguous Claude authentication
accepted, and valid 65,530-byte input rejected due to read-ahead. After repairs,
the complete Codex/Claude backend targets passed (16 and 22 tests), and the new
public input regression passed all five cases.

The required-test registry binds R1/R2 to BACKENDS-001 and ARCH-002; R3 to
PRODUCT-001 and STREAM-001. Three persistent handwritten mutations restore these
defects and require those exact assertions, bringing the shared gate to 45.
Existing mutations, strict compiler/setup/missing-test failure policy, workspace
Nextest profile, doctests and the separate domain-only mutation scope remain intact.

The first full gate stopped on the stale `accept-api-key-auth` patch after the
DTO change. Its patch now removes the same subscription-method check from the
typed status predicate; its original registered behavioral test and required
assertion are unchanged. Stale-patch failure was not counted as detection. The
complete gate was then restarted against the final implementation and tests.

The final `bash scripts/check_contracts.sh` passed:

- Required-test presence/coverage and inward dependency checks passed.
- 431 workspace runtime tests passed, none skipped; seven doctest/compile-fail
  examples passed.
- All 45 handwritten mutations were detected by their exact registered tests,
  including the three new repairs and the updated API-key-admission patch.
- All 179 domain mutants completed: 100 caught (98 assertion/test failures and
  two bounded per-test nontermination failures), 79 compile-invalid/unviable,
  zero survivors or outer timeouts. Unviable cases are not detections.
- `cargo fmt --all --check`, workspace/all-target Clippy with warnings denied,
  `git diff --check` and independent applicability checks for every handwritten
  mutation patch passed.

`repair-gate-evidence.txt` retains the final summaries and every handwritten
mutation result. The full local gate log is
`/tmp/cyoa-review-repair-final-contracts.log`; `repair-mutations/` retains the
baseline, mutant and restored logs for each new repair and the updated existing
auth mutation. `repair-domain-summary.json` retains the sweep totals and baseline
status. These are distinct from the live evidence below.

Documentation also labels historical PLAN checkpoints and replaces README's stale “Next is”
headless handoff with the completed status. Phase 3's TUI play screen is next.

## Live Codex verification

```sh
cargo run -p cyoa-infrastructure --example codex_adapter_smoke --locked --offline -- /tmp/cyoa-review-repairs-live
```

The bounded actual-adapter smoke ran outside the outer sandbox (the review had
already established app-server initialization fails there), with installed
`codex-cli 0.160.0`, bundled outline configuration, subscription preflight and no
model override. It accepted “The Bell Beneath the Tide” in 46,340 ms with one
byte-exact payload emission, empty stderr, a valid domain outline, reported input
14,443/cached 0/output 831, and no observed model/cost. Captures are retained in
`repair-live-codex-0.160.0/`.

This is one live outline smoke through the repaired codec; malformed duplicate
records remain synthetic evidence. It does not expand the full-story/headless,
global instruction/tool isolation, model override, rate-limit or cancellation
claims. Independent historical Claude live evidence remains unchanged.
