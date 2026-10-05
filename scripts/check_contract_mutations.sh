#!/usr/bin/env bash
# Run intentional regressions only in an isolated copy of the current source.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
source scripts/lib/test_outcome.sh
bash scripts/test_mutation_outcome.sh
manifest=scripts/mutations/manifest.json
registry=docs/decisions/required-tests.json
jq -e --slurpfile registry "$registry" '
  type == "array" and length >= 45
  and ([.[].id] | length == (unique | length))
  and ([.[].id] as $ids | all([
    "name-only-deduplication", "duplicate-opening-request",
    "default-instead-of-active-limits", "commit-before-reporting-failure",
    "capture-past-limit", "accept-incomplete-input", "ignore-cancelled-token",
    "accept-failed-process", "swallow-reader-error", "disable-cancellation-wake",
    "discard-cleanup-cause", "accept-ineligible-message", "accept-missing-terminal",
    "accept-conflicting-terminal", "adapt-shared-schema", "adapt-peer-schema",
    "discard-candidate-evidence", "discard-diagnostic-evidence",
    "forward-uncorrelated-preview", "share-block-indices-across-messages",
    "ignore-is-error-result", "accept-stream-without-result", "accept-duplicate-result",
    "reserialize-structured-output", "result-overrides-process-failure",
    "accept-api-key-auth", "discard-claude-candidate-evidence",
    "discard-claude-diagnostic-evidence", "leak-advisor-into-shared-instructions",
    "drop-candidate-on-malformed-metadata", "accept-last-duplicate-field",
    "accept-stale-worker-result", "accept-cancelled-worker-success", "commit-worker-turn-twice", "ignore-headless-interrupt", "block-final-error-report", "advance-demo-on-cancel",
    "accept-stale-storage-receipt", "autosave-unaccepted-generation",
    "rotate-corrupt-primary", "mark-post-rename-failure-clean",
    "retry-generation-on-save-failure", "accept-codex-duplicate-control",
    "accept-claude-duplicate-auth", "count-input-read-ahead"
  ][]; . as $id | $ids | index($id) != null))
  and all(.[];
    (if (.id | test("^(accept-codex-duplicate-control|accept-claude-duplicate-auth|count-input-read-ahead)$")) then
      (.failure_contains | type == "string" and length > 0) else true end)
    and
    (if (.id | test("^(accept-stale-storage-receipt|autosave-unaccepted-generation|rotate-corrupt-primary|mark-post-rename-failure-clean|retry-generation-on-save-failure)$")) then
      (.failure_contains | type == "string" and length > 0) else true end)
    and
    (if (.id | test("^(accept-stale-worker-result|accept-cancelled-worker-success|commit-worker-turn-twice|ignore-headless-interrupt|block-final-error-report|advance-demo-on-cancel)$")) then
      (.failure_contains | type == "string" and length > 0) else true end)
    and (.id | test("^[a-z-]+$"))
    and (if (.id | test("^(accept-ineligible-message|accept-missing-terminal|accept-conflicting-terminal|adapt-shared-schema|adapt-peer-schema|discard-candidate-evidence|discard-diagnostic-evidence|forward-uncorrelated-preview|share-block-indices-across-messages|ignore-is-error-result|accept-stream-without-result|accept-duplicate-result|reserialize-structured-output|result-overrides-process-failure|accept-api-key-auth|discard-claude-candidate-evidence|discard-claude-diagnostic-evidence|leak-advisor-into-shared-instructions|drop-candidate-on-malformed-metadata|accept-last-duplicate-field)$")) then
      (.failure_contains | type == "string" and length > 0) else true end)
    and (.file | test("^cyoa-[a-z]+/src/[a-z_/]+\\.rs$"))
    and (. as $mutation | any($registry[0][];
      .id == $mutation.decision and any(.checks[];
        .package == $mutation.package and .target == $mutation.target
        and (.tests | index($mutation.test) != null)))))
' "$manifest" > /dev/null
mutation_root=$(mktemp -d)
trap 'rm -rf -- "$mutation_root"' EXIT
mkdir "$mutation_root/work"
# Include unstaged and nonignored new source, never .git or build artifacts.
git ls-files -z --cached --others --exclude-standard > "$mutation_root/files"
tar -cf "$mutation_root/source.tar" --null -T "$mutation_root/files"
tar -xf "$mutation_root/source.tar" -C "$mutation_root/work"
export CARGO_TARGET_DIR="$mutation_root/target"
# Optional retained evidence; use an absolute path because tests run in a copy.
evidence_dir=${CYOA_MUTATION_EVIDENCE_DIR:-}
if [[ -n "$evidence_dir" ]]; then
  mkdir -p -- "$evidence_dir"
  evidence_dir=$(cd -- "$evidence_dir" && pwd)
fi

run_test() {
  local expected=$1 package=$2 target=$3 name=$4 log=$5 failure_contains=${6:-} status=0
  local -a target_args=(--test "$target")
  if [[ "$target" == lib ]]; then target_args=(--lib); fi
  (cd "$mutation_root/work" && cargo test -p "$package" "${target_args[@]}" \
    --locked --offline --color never -- "$name" --exact --test-threads=1) > "$log" 2>&1 || status=$?
  if [[ -n "$evidence_dir" ]]; then
    cp -- "$log" "$evidence_dir/$id-$(basename -- "$log")"
  fi
  if ! assert_test_outcome "$expected" "$status" "$name" "$log" "$failure_contains"; then
    cat "$log" >&2
    printf 'Expected one exact %s test: %s/%s/%s (exit %s).\n' "$expected" "$package" "$target" "$name" "$status" >&2
    return 1
  fi
}
while IFS=$'\t' read -r id package target name file failure_contains; do
  printf 'Checking mutation: %s\n' "$id"
  run_test passed "$package" "$target" "$name" "$mutation_root/baseline.log"
  cp "$mutation_root/work/$file" "$mutation_root/original"
  patch="scripts/mutations/$id.patch"
  (cd "$mutation_root/work" && git apply --check "$patch" && git apply "$patch")
  run_test failed "$package" "$target" "$name" "$mutation_root/mutant.log" "$failure_contains"
  cp "$mutation_root/original" "$mutation_root/work/$file"
  run_test passed "$package" "$target" "$name" "$mutation_root/restored.log"
  printf 'Detected mutation: %s (%s/%s)\n' "$id" "$target" "$name"
done < <(jq -r '.[] | [.id, .package, .target, .test, .file, (.failure_contains // "")] | @tsv' "$manifest")
printf '%s\n' 'All required behavioral mutations detected.'
