#!/usr/bin/env bash
# Check inward workspace dependencies, including dev/build dependencies.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

errors=$(cargo metadata --format-version 1 --no-deps --locked | jq -r '
  {
    "cyoa-core": [],
    "cyoa-application": ["cyoa-core"],
    "cyoa-infrastructure": ["cyoa-core", "cyoa-application"],
    "cyoa-presentation": ["cyoa-core", "cyoa-application"],
    "cyoa-cli": ["cyoa-core", "cyoa-application", "cyoa-infrastructure", "cyoa-presentation"]
  } as $allowed
  | .workspace_members as $members
  | [.packages[] | select(.id as $id | $members | index($id))] as $packages
  | [$packages[].name] as $names
  | $packages[]
  | .name as $name
  | if ($allowed | has($name) | not) then
      "Assign \($name) an architectural layer before adding it."
    else
      .dependencies[].name as $target
      | if (($names | index($target)) != null and
            ($allowed[$name] | index($target)) == null) then
          "Forbidden dependency: \($name) -> \($target)"
        elif ((["cyoa-core", "cyoa-application"] | index($name)) != null and
              (["clap", "ratatui", "crossterm", "serde_json", "schemars", "toml", "minijinja"] | index($target)) != null) then
          "Outer-layer dependency in \($name): \($target)"
        else empty end
    end
')

if [[ -n "$errors" ]]; then
  printf '%s\n' "$errors" >&2
  exit 1
fi
printf '%s\n' 'Workspace dependency boundaries verified.'
