#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
source scripts/lib/test_outcome.sh
outcome_log=$(mktemp)
trap 'rm -f -- "$outcome_log"' EXIT
reject() {
  printf '%s\n' "$2" > "$outcome_log"
  if assert_test_outcome failed "$1" sentinel "$outcome_log"; then
    printf 'Incorrectly accepted: %s\n' "$3" >&2
    exit 1
  fi
}
# Error cases: failures of the harness/compiler cannot kill a behavioral mutant.
reject 101 'error[E0308]: mismatched types' 'compiler failure'
reject 127 'cargo: command not found' 'missing executable'
reject 0 $'test sentinel ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;' 'surviving mutant'
# Edge cases: filtered/ignored/wrong tests are not evidence.
reject 101 $'test result: FAILED. 0 passed; 0 failed; 0 ignored;' 'zero selected tests'
reject 101 $'test sentinel ... ignored\ntest result: FAILED. 0 passed; 1 failed; 1 ignored;' 'ignored selected test'
reject 101 $'test different ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;' 'different failing test'
reject 0 $'test sentinel ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;' 'inconsistent exit status'
# Nominal: one exact behavioral failure, and a baseline that really passes.
printf '%s\n' 'test sentinel ... FAILED' 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out;' > "$outcome_log"
assert_test_outcome failed 101 sentinel "$outcome_log"
printf '%s\n' 'test sentinel ... ok' 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out;' > "$outcome_log"
assert_test_outcome passed 0 sentinel "$outcome_log"
printf '%s\n' 'Mutation outcome validation verified.'
