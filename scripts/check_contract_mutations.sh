#!/usr/bin/env bash
# Run intentional regressions only in an isolated copy of the current source.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
source scripts/lib/test_outcome.sh
bash scripts/test_mutation_outcome.sh
manifest=scripts/mutations/manifest.json
registry=docs/decisions/required-tests.json
jq -e --slurpfile registry "$registry" '
  type == "array" and length >= 4
  and ([.[].id] | length == (unique | length))
  and ([.[].id] as $ids | all([
    "name-only-deduplication", "duplicate-opening-request",
    "default-instead-of-active-limits", "commit-before-reporting-failure"
  ][]; . as $id | $ids | index($id) != null))
  and all(.[];
    (.id | test("^[a-z-]+$"))
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

run_test() {
  local expected=$1 package=$2 target=$3 name=$4 log=$5 status=0
  (cd "$mutation_root/work" && cargo test -p "$package" --test "$target" \
    --locked --offline --color never -- "$name" --exact --test-threads=1) > "$log" 2>&1 || status=$?
  if ! assert_test_outcome "$expected" "$status" "$name" "$log"; then
    cat "$log" >&2
    printf 'Expected one exact %s test: %s/%s/%s (exit %s).\n' "$expected" "$package" "$target" "$name" "$status" >&2
    return 1
  fi
}
while IFS=$'\t' read -r id package target name file; do
  printf 'Checking mutation: %s\n' "$id"
  run_test passed "$package" "$target" "$name" "$mutation_root/baseline.log"
  cp "$mutation_root/work/$file" "$mutation_root/original"
  patch="scripts/mutations/$id.patch"
  (cd "$mutation_root/work" && git apply --check "$patch" && git apply "$patch")
  run_test failed "$package" "$target" "$name" "$mutation_root/mutant.log"
  cp "$mutation_root/original" "$mutation_root/work/$file"
  run_test passed "$package" "$target" "$name" "$mutation_root/restored.log"
  printf 'Detected mutation: %s (%s/%s)\n' "$id" "$target" "$name"
done < <(jq -r '.[] | [.id, .package, .target, .test, .file] | @tsv' "$manifest")
printf '%s\n' 'All required behavioral mutations detected.'
