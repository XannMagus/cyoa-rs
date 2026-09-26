#!/usr/bin/env bash
# Offline artifact checks, NOT tests of a Codex adapter (none exists yet).
set -euo pipefail
review=$(cd -- "$(dirname -- "$0")" && pwd)
repo=$(cd -- "$review/../.." && pwd)
work=$(mktemp -d /tmp/cyoa-profile-check.XXXXXX)
trap 'rm -rf -- "$work"' EXIT
bash "$review/build_tools.sh"
/tmp/cyoa-profile-target/debug/dump "$work/requests" "$repo"
bash "$review/prepare_variants.sh" "$work/requests"
diff -r "$review/requests" "$work/requests"
/tmp/cyoa-profile-target/debug/inspect "$review"
for pair in 'world world' 'cast cast' 'opening-ref-union opening_ref_union' 'continuation-hardened continuation_ref_union' 'zero-npcs-hardened zero_npcs' 'null-hardened turn_null' 'empty-hardened turn_empty'; do
    read -r label kind <<< "$pair"
    tail -n 1 "$review/evidence/$label/stdin.txt" > "$work/envelope.json"
    jq -jr '.instructions' "$work/envelope.json" > "$work/instructions.txt"
    jq -jr '.prompt' "$work/envelope.json" > "$work/prompt.txt"
    cmp "$work/instructions.txt" "$review/requests/$kind/instructions.txt"
    cmp "$work/prompt.txt" "$review/requests/$kind/prompt.txt"
done
for label in world cast opening-ref-union continuation-hardened zero-npcs-hardened null-hardened empty-hardened canary canary-hardened tool-hardened; do
    jq -e '.exit_code == 0 and .status == "success" and .stdout_capture == "Complete" and .stderr_capture == "Complete" and .workspace_removed' "$review/evidence/$label/result.json" > /dev/null
done
jq -se '.[2].item.text | fromjson | .title == "CYOA_LOCAL_INSTRUCTION_CANARY"' "$review/evidence/canary/stdout.jsonl" > /dev/null
jq -se '.[2].item.text | fromjson | .title != "CYOA_LOCAL_INSTRUCTION_CANARY"' "$review/evidence/canary-hardened/stdout.jsonl" > /dev/null
jq -se '[.[] | select(.type == "item.completed" and .item.type == "command_execution")][0].item.aggregated_output == "CYOA_TOOL_READ_CANARY\n"' "$review/evidence/tool-hardened/stdout.jsonl" > /dev/null
# Independently specified source state, not generated from the live response.
for kind in opening_turn continuation_turn; do
    sed -n '/^{/,/^}$/p' "$review/requests/$kind/prompt.txt" > "$work/summary.json"
    phase=opening
    if [[ $kind == continuation_turn ]]; then phase=continuation; fi
    jq -e --arg phase "$phase" --slurpfile expected "$review/expected-input-state.json" '
      .world == $expected[0].outline.world_description and
      [.characters[].id] == $expected[0][$phase].character_ids and
      .major_events == $expected[0][$phase].major_events and
      .upcoming_events == $expected[0][$phase].upcoming_events and
      .current_situation == $expected[0][$phase].current_situation' "$work/summary.json" > /dev/null
done
# Regenerating synthetic bytes checks provenance/reproducibility, not acceptance.
cp -r "$review/synthetic" "$work/synthetic"
bash "$work/synthetic/make_fixtures.sh"
diff -r "$review/synthetic" "$work/synthetic"
printf '%s\n' 'PASS: bundled artifacts, explicit source state, live observations, wire/domain boundaries and synthetic reproduction (no codec implementation).'
