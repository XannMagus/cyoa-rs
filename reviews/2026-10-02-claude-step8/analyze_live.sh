#!/usr/bin/env bash
# Independent re-check of live-attempt/ from the raw artifacts (jq only; no model
# calls, nothing taken from the Rust harness's own result.json counters).
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; L="${1:-$HERE/live-attempt}"
for d in 01 02 03 04 05; do
  f="$L/$d/stdout.jsonl"; echo "== call $d"
  echo "message_start: $(jq -c 'select(.type=="stream_event" and .event.type=="message_start")' "$f" | wc -l); results: $(jq -c 'select(.type=="result")' "$f" | wc -l); stderr bytes: $(wc -c < "$L/$d/stderr.bin")"
  echo "blocks: $(jq -rs '[.[]|select(.type=="stream_event")|.event] | reduce .[] as $e ({m:0,out:[]}; if $e.type=="message_start" then .m+=1 elif $e.type=="content_block_start" then .out += ["M\(.m):\($e.index)=\($e.content_block.type)\(if $e.content_block.name then ":"+$e.content_block.name else "" end)"] else . end) | .out | join(" ")' "$f")"
  echo "real advisor content blocks: $(jq -c 'select(.type=="stream_event" and .event.type=="content_block_start" and (.event.content_block.name=="advisor" or .event.content_block.type=="advisor_tool_result" or .event.content_block.type=="server_tool_use"))' "$f" | wc -l)"
  jq -c 'select(.type=="system" and .subtype=="init") | {model, apiKeySource, tools}' "$f" | sed 's/^/init: /'
  [ "$d" = 05 ] && continue
  tail -1 "$f" | jq -cj '.structured_output' > "$L/$d/.span"
  cmp -s "$L/$d/.span" "$L/$d/payload.json" && echo "payload.json == result.structured_output (compact span): byte-equal" || echo "payload.json != compact structured_output: DIFFER"
  rm -f "$L/$d/.span"
  tail -1 "$f" | jq -c '{is_error, subtype, terminal_reason, num_turns, total_cost_usd}' | sed 's/^/result: /'
  echo "forbidden flags in argv (bare or model): $(tr '\0' '\n' < "$L/$d/argv.bin" | rg -c '^--(bare|model)$' || echo 0)"
done
echo "== argv of call 03 (values elided except flags)"
tr '\0' '\n' < "$L/03/argv.bin" | awk 'NR<=12 && length($0)<80 {print} NR<=12 && length($0)>=80 {print "<" length($0) " bytes>"}'
echo "== auth/identity leakage check: email-like strings in artifacts"
rg -l '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[a-z]{2,}' "$L" || echo none
