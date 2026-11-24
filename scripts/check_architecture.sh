#!/usr/bin/env bash
# Check inward workspace dependencies, including dev/build dependencies, and keep
# backend-specific code out of shared, backend-agnostic modules (ARCH-003).
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

# Domain and application are allowlisted, not denylisted: any new external crate
# (serde, a terminal or filesystem library, ...) must be an explicit decision.
errors=$(cargo metadata --format-version 1 --no-deps --locked | jq -r '
  {
    "cyoa-core": [],
    "cyoa-application": ["cyoa-core"],
    "cyoa-infrastructure": ["cyoa-core", "cyoa-application"],
    "cyoa-presentation": ["cyoa-core", "cyoa-application"],
    "cyoa-cli": ["cyoa-core", "cyoa-application", "cyoa-infrastructure", "cyoa-presentation"]
  } as $allowed
  | {
    "cyoa-core": ["indexmap", "thiserror"],
    "cyoa-application": ["thiserror"]
  } as $external
  | .workspace_members as $members
  | [.packages[] | select(.id as $id | $members | index($id))] as $packages
  | [$packages[].name] as $names
  | $packages[]
  | .name as $name
  | if ($allowed | has($name) | not) then
      "Assign \($name) an architectural layer before adding it."
    else
      .dependencies[].name as $target
      | if ($names | index($target)) != null then
          # A crate may dev-depend on itself to enable its own test-only features.
          if $target != $name and ($allowed[$name] | index($target)) == null then
            "Forbidden dependency: \($name) -> \($target)"
          else empty end
        elif ($external | has($name)) and ($external[$name] | index($target)) == null then
          "Unapproved external dependency in \($name): \($target)"
        else empty end
    end
')

# Shared generation code must never special-case a vendor. Each backend's quirks
# live in its own adapter; the only shared mention allowed is the one-line module
# registration in backend_compat/mod.rs. Comments may cite reference files.
execution=$(grep --recursive --line-number --extended-regexp \
  'JoinHandle|thread::(spawn|scope)|thread::Builder' \
  cyoa-core/src cyoa-application/src cyoa-presentation/src \
  | grep --invert-match --extended-regexp '^[^:]+:[0-9]+:[[:space:]]*(//|#)' || true)
if [[ -n "$execution" ]]; then
  errors+=${errors:+$'\n'}"Execution resources must be owned by infrastructure (ARCH-001):"$'\n'"$execution"
fi
presentation_policy=$(grep --line-number --extended-regexp \
  '^pub mod (session|runtime|worker|storage|persistence);' cyoa-presentation/src/lib.rs || true)
if [[ -n "$presentation_policy" ]]; then
  errors+=${errors:+$'\n'}"Session/execution policy must not return to presentation (ARCH-001)."
fi
shared=(
  cyoa-core/src
  cyoa-application/src
  cyoa-infrastructure/src/backend.rs
  cyoa-infrastructure/src/json.rs
  cyoa-infrastructure/src/generation
)
vendor=$(grep --recursive --line-number --ignore-case --binary-files=without-match \
  --extended-regexp '\b(claude|codex)' "${shared[@]}" \
  | grep --invert-match --extended-regexp \
    '^cyoa-infrastructure/src/generation/backend_compat/(claude_cli|codex_cli|mod)\.rs:' \
  | grep --invert-match --extended-regexp '^[^:]+:[0-9]+:[[:space:]]*(//|#)' || true)
if [[ -n "$vendor" ]]; then
  errors+=${errors:+$'\n'}"Backend-specific code in a shared module (ARCH-003):"$'\n'"$vendor"
fi
registrations=$(grep --count --ignore-case --extended-regexp '\b(claude|codex)' \
  cyoa-infrastructure/src/generation/backend_compat/mod.rs || true)
if [[ "$registrations" != 2 ]]; then
  errors+=${errors:+$'\n'}"backend_compat/mod.rs must only register the two backend modules."
fi

if [[ -n "$errors" ]]; then
  printf '%s\n' "$errors" >&2
  exit 1
fi
printf '%s\n' 'Workspace dependency and backend placement boundaries verified.'
