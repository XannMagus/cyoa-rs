#!/usr/bin/env bash
# Standard Rust mutation testing, deliberately limited to the domain and its tests.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

if [[ $(cargo mutants --version) != 'cargo-mutants 27.1.0' ]] ||
   [[ $(cargo nextest --version | head -1) != cargo-nextest\ 0.9.132\ * ]]; then
  printf '%s\n' 'Install cargo-mutants 27.1.0 and cargo-nextest 0.9.132; see docs/testing/domain-mutations.md.' >&2
  exit 1
fi

# Always do a full domain sweep and a baseline. No diff filter or survivor allowance.
cargo mutants --package cyoa-core --test-package cyoa-core --test-workspace=false \
  --jobs "${CYOA_MUTATION_JOBS:-2}" --build-timeout 120

# Pin the report schema with the tool version. Build errors are unviable, not caught;
# a nextest invocation error or zero selected tests must never count as detection.
jq -e '
  .cargo_mutants_version == "27.1.0" and .success
  and .total_mutants > 0 and .caught > 0 and .missed == 0 and .timeout == 0
  and .total_mutants == (.caught + .unviable)
  and any(.outcomes[]; .scenario == "Baseline" and .summary == "Success")
  and all(.outcomes[] | select(.summary == "CaughtMutant");
    any(.phase_results[]; .phase == "Build" and .process_status == "Success")
    and any(.phase_results[]; .phase == "Test" and .process_status == {"Failure": 100}))
' mutants.out/outcomes.json > /dev/null
deadline_failures=0
while IFS= read -r log; do
  if grep -Eq '^ *TIMEOUT \[' "mutants.out/$log"; then
    deadline_failures=$((deadline_failures + 1))
  fi
done < <(jq -r '.outcomes[] | select(.summary == "CaughtMutant") | .log_path' mutants.out/outcomes.json)
printf 'Caught mutations with per-test nontermination failures: %s (included in cargo-mutants caught count).\n' "$deadline_failures"
printf '%s\n' 'Domain mutation gate passed. Unviable mutants are not test detections; review limits remain documented.'
