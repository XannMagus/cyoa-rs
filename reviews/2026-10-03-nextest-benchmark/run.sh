#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
evidence=reviews/2026-10-03-nextest-benchmark
rustc "$evidence/measure.rs" -o /tmp/cyoa-benchmark-timer
measure() {
 local phase=$1 iteration=$2 runner=$3 threads=$4 scope=$5
 local selection=(--workspace)
 if [[ $scope == domain ]]; then selection=(-p cyoa-core); fi
 if [[ $runner == cargo ]]; then
  /tmp/cyoa-benchmark-timer "$evidence" "$phase" "$iteration" "$runner" cargo test "${selection[@]}" --lib --bins --tests --locked --offline --color never -- --test-threads "$threads"
 else
  /tmp/cyoa-benchmark-timer "$evidence" "$phase" "$iteration" "$runner" cargo nextest run "${selection[@]}" --lib --bins --tests --locked --offline --color never --test-threads "$threads"
 fi
}
for threads in 24 8; do
 for runner in cargo nextest; do measure "warmup-$threads" 0 "$runner" "$threads" workspace; done
 for iteration in {1..7}; do
  runners=(cargo nextest)
  if (( iteration % 2 == 0 )); then runners=(nextest cargo); fi
  for runner in "${runners[@]}"; do measure "workspace-$threads" "$iteration" "$runner" "$threads" workspace; done
 done
done
for iteration in {1..7}; do
 runners=(cargo nextest)
 if (( iteration % 2 == 0 )); then runners=(nextest cargo); fi
 for runner in "${runners[@]}"; do measure domain "$iteration" "$runner" 24 domain; done
done
/tmp/cyoa-benchmark-timer "$evidence" doctests 1 cargo cargo test --workspace --doc --locked --offline --color never
# Choose new, empty targets for each cold-build pair; do not reuse these paths.
cold_root=$(mktemp -d /tmp/cyoa-benchmark-cold.XXXXXX)
CARGO_TARGET_DIR="$cold_root/cargo" /tmp/cyoa-benchmark-timer "$evidence" cold-build 1 cargo cargo test --workspace --lib --bins --tests --no-run --locked --offline --color never
CARGO_TARGET_DIR="$cold_root/nextest" /tmp/cyoa-benchmark-timer "$evidence" cold-build 1 nextest cargo nextest run --workspace --lib --bins --tests --no-run --locked --offline --color never
