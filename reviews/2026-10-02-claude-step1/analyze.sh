#!/usr/bin/env bash
# Summarizes each live capture against the step-1 evidence questions. Pure jq/cmp;
# no model calls. Usage: analyze.sh [evidence-dir]  (default: ./evidence)
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EVID="${1:-$HERE/evidence}"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
for dir in "$EVID"/p[1-4]-*/ "$EVID"/p5-*/; do
  [ -f "$dir/stdout.jsonl" ] || continue
  f="$dir/stdout.jsonl"; name="$(basename "$dir")"
  echo "== $name"
  jq -r '.exit as $e | "exit=\($e) elapsed_ms=\(.elapsed_ms) stdout_bytes=\(.stdout_bytes) stderr_bytes=\(.stderr_bytes)"' "$dir/meta.json"
  echo "message_start records: $(jq -c 'select(.type=="stream_event" and .event.type=="message_start")' "$f" | wc -l)"
  echo "result records: $(jq -c 'select(.type=="result")' "$f" | wc -l); last record is result: $([ "$(tail -1 "$f" | jq -r .type)" = result ] && echo yes || echo no)"
  echo "top-level types: $(jq -r '.type + (if .subtype then "/"+.subtype else "" end)' "$f" | sort | uniq -c | tr '\n' ';')"
  echo "content block types: $(jq -r 'select(.type=="stream_event" and .event.type=="content_block_start") | .event.content_block.type + (if .event.content_block.name then ":"+.event.content_block.name else "" end)' "$f" | sort | uniq -c | tr '\n' ';')"
  echo "per-message blocks: $(jq -rs '[.[] | select(.type=="stream_event") | .event] | reduce .[] as $e ({m:0,out:[]}; if $e.type=="message_start" then .m+=1 elif $e.type=="content_block_start" then .out += ["M\(.m):\($e.index)=\($e.content_block.type)\(if $e.content_block.name then ":"+$e.content_block.name else "" end)"] else . end) | .out | join(" ")' "$f")"
  echo "user record kinds: $(jq -r 'select(.type=="user") | .message.content[0] | .type + (if .type=="text" then ":"+(.text|.[0:48]) else "" end)' "$f" | tr "\n" ";")"
  echo "advisor content blocks: $(jq -c 'select(.type=="stream_event" and .event.type=="content_block_start" and (.event.content_block.name=="advisor" or .event.content_block.type=="advisor_tool_result"))' "$f" | wc -l)"
  jq -c 'select(.type=="system" and .subtype=="init") | {model, apiKeySource, tools, claude_code_version}' "$f" | sed 's/^/init: /'
  idx="$(jq -r 'select(.type=="stream_event" and .event.type=="content_block_start" and .event.content_block.name=="StructuredOutput") | .event.index' "$f" | head -1)"
  echo "StructuredOutput block index: $idx (count: $(jq -c 'select(.type=="stream_event" and .event.type=="content_block_start" and .event.content_block.name=="StructuredOutput")' "$f" | wc -l))"
  jq -j --argjson i "${idx:-0}" 'select(.type=="stream_event" and .event.delta.type=="input_json_delta" and .event.index==$i) | .event.delta.partial_json' "$f" > "$T/partial"
  tail -1 "$f" | jq -j '.result' > "$T/result_string"
  tail -1 "$f" | jq -cj '.structured_output' > "$T/so_compact"
  echo "bytes: partial_json=$(wc -c < "$T/partial") result_string=$(wc -c < "$T/result_string") structured_output(compact)=$(wc -c < "$T/so_compact")"
  cmp -s "$T/result_string" "$T/so_compact" && echo "result string == compact structured_output: byte-equal" || echo "result string vs compact structured_output: DIFFER"
  cmp -s "$T/partial" "$T/so_compact" && echo "partial_json == compact structured_output: byte-equal" || {
    jq -S . "$T/partial" > "$T/a" 2>/dev/null; jq -S . "$T/so_compact" > "$T/b"
    cmp -s "$T/a" "$T/b" && echo "partial_json vs structured_output: semantically equal, bytes differ" || echo "partial_json vs structured_output: SEMANTICALLY DIFFER"; }
  tail -1 "$f" | jq -c '{is_error, subtype, terminal_reason, stop_reason, num_turns, total_cost_usd, usage: (.usage|{input_tokens,cache_creation_input_tokens,cache_read_input_tokens,output_tokens}), models: (.modelUsage|to_entries|map({model:.key, costBasis:.value.costBasis, provider:.value.provider}))}' | sed 's/^/result: /'
  echo "canary mentions: $(rg -c "PINEAPPLE-CANARY|Zorblax" "$f" || echo 0)"
done
