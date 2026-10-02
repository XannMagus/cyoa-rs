#!/usr/bin/env bash
# Offline artifact checks (no model calls, no Rust). These verify that the frozen
# evidence is internally consistent and reproducible; they are NOT tests of a
# Claude adapter. Run from anywhere.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT

# 1. Every live success capture: exit 0, one success result last, is_error false,
#    prompt and argv artifacts equal the committed request bundle.
for n in p1-world:world p2-cast:cast p3-opening:opening_turn p4-continuation:continuation_turn p5-canary-world:world; do
  dir="$here/evidence/${n%%:*}"; req="$here/requests/${n##*:}"
  jq -e '.exit == 0 and .stderr_bytes == 0' "$dir/meta.json" >/dev/null
  tail -1 "$dir/stdout.jsonl" | jq -e '.type=="result" and .subtype=="success" and .is_error==false and (.structured_output|type)=="object"' >/dev/null
  [ "$(jq -c 'select(.type=="result")' "$dir/stdout.jsonl" | wc -l)" = 1 ]
  cmp "$dir/stdin.txt" "$req/prompt.txt"
  cmp "$dir/argv-system-prompt.txt" "$req/instructions.txt"
  jq -c . "$req/schema.claude-adapted.json" | tr -d '\n' | cmp - "$dir/argv-json-schema.json"
  [ "$(jq -r 'select(.type=="system" and .subtype=="init")|.apiKeySource' "$dir/stdout.jsonl")" = none ]
  jq -e 'select(.type=="system" and .subtype=="init")|.tools==["StructuredOutput"]' "$dir/stdout.jsonl" >/dev/null
  [ "$(jq -c 'select(.type=="stream_event" and .event.content_block.name=="advisor")' "$dir/stdout.jsonl" | wc -l)" = 0 ]
done
# 2. Positive control leaks the canary; every safe-mode capture does not.
! rg -q 'PINEAPPLE-CANARY|Zorblax' "$here"/evidence/p[1-5]-*/stdout.jsonl --glob '!*without-safe-mode*'
rg -q 'PINEAPPLE-CANARY|Zorblax' "$here/evidence/p5-canary-world-without-safe-mode/stdout.jsonl"
# 3. The unadapted schema is rejected before any model call.
[ ! -s "$here/evidence/p6-unadapted-schema/stdout.jsonl" ]
rg -q 'not a valid JSON Schema' "$here/evidence/p6-unadapted-schema/stderr.txt"
# 4. Derived artifacts reproduce byte-for-byte.
mkdir -p "$work/reviews"; cp -r "$here" "$work/reviews/review"; ln -s "$here/../2026-09-25-claude-cli-refresh" "$work/reviews/2026-09-25-claude-cli-refresh"
rm -rf "$work/reviews/review/synthetic" "$work/reviews/review/expected"
bash "$work/reviews/review/make_synthetic.sh" >/dev/null; bash "$work/reviews/review/make_expected.sh" >/dev/null
work_review="$work/reviews/review"
diff -r "$here/synthetic" "$work_review/synthetic" -x expectations.json
diff -r "$here/expected" "$work_review/expected" -x live-expectations.json
bash "$here/check_synthetic.sh"
# 5. The auth artifact holds mode fields only.
jq -e 'keys == ["apiProvider","authMethod","loggedIn","subscriptionType"]' "$here/evidence/auth-status.json" >/dev/null
echo "evidence checks passed"
