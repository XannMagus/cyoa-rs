#!/usr/bin/env bash
# Project contracts override Python parity. Keep this gate shared by CI and local work.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
registry=docs/decisions/required-tests.json
contract=docs/decisions/README.md

jq -e '
  type == "array" and length > 0
  and ([.[].id] | length == (unique | length))
  and all(.[];
    (.id | test("^[A-Z]+-[0-9]{3}$"))
    and (.coverage == "enforced" or .coverage == "partial" or .coverage == "pending")
    and (.checks | type == "array")
    and (if .coverage == "enforced" then (.checks | length > 0) else true end)
    and (if .coverage == "pending" then (.checks | length == 0) else true end)
    and all(.checks[];
      (.package | test("^cyoa-[a-z]+$"))
      and (.target | test("^[a-z_]+$"))
      and (.tests | type == "array" and length > 0)
      and all(.tests[]; type == "string" and test("^[A-Za-z_][A-Za-z_0-9:]*$"))))
' "$registry" > /dev/null

documented=$(sed -n 's/^### \([A-Z]*-[0-9][0-9][0-9]\) .*/\1/p' "$contract" | sort)
registered=$(jq -r '.[].id' "$registry" | sort)
if [[ "$documented" != "$registered" ]]; then
  printf '%s\n' 'Decision headings and required-test registry disagree.' >&2
  exit 1
fi

while IFS=$'\t' read -r package target; do
  args=(-p "$package" --locked --offline)
  case "$target" in
    lib) args+=(--lib) ;;
    doc) args+=(--doc) ;;
    *) args+=(--test "$target") ;;
  esac
  listed=$(cargo test "${args[@]}" -- --list --format terse)
  ignored=$(cargo test "${args[@]}" -- --ignored --list --format terse)
  if [[ "$target" == doc ]]; then
    # Rustdoc locations move during refactors; the qualified item's name is stable.
    listed=$(sed -E 's/^.* - //; s/ \(line [0-9]+\): test$/: test/' <<< "$listed")
    ignored=$(sed -E 's/^.* - //; s/ \(line [0-9]+\): test$/: test/' <<< "$ignored")
  fi
  while IFS=$'\t' read -r decision test_name; do
    if ! grep -Fxq -- "$test_name: test" <<< "$listed"; then
      printf '%s\n' "$decision: required test missing: $package/$target/$test_name" >&2
      exit 1
    fi
    if grep -Fxq -- "$test_name: test" <<< "$ignored"; then
      printf '%s\n' "$decision: required test ignored: $package/$target/$test_name" >&2
      exit 1
    fi
  done < <(jq -r --arg package "$package" --arg target "$target" '
    .[] | .id as $id | .checks[] | select(.package == $package and .target == $target)
    | .tests[] | [$id, .] | @tsv' "$registry")
done < <(jq -r '[.[].checks[] | [.package, .target]] | unique[] | @tsv' "$registry")

bash scripts/check_architecture.sh
cargo test --workspace --locked --offline
printf '%s\n' 'Project contracts verified. Pending feature obligations remain listed in docs/decisions.'
