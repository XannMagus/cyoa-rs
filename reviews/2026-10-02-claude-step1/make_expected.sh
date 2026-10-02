#!/usr/bin/env bash
# Derives expected/<name>.{payload.json,preview.txt} from the frozen live captures
# with jq only (never from the Rust codec under test): the payload is the result's
# structured_output in the CLI's compact serialization, the preview is the exact
# concatenation of input_json_delta fragments of the (first) StructuredOutput block.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; OUT="$HERE/expected"; mkdir -p "$OUT"
R25="$HERE/../2026-09-25-claude-cli-refresh/evidence"
emit() { # name file
  local name="$1" f="$2"
  tail -1 "$f" | jq -cj '.structured_output' > "$OUT/$name.payload.json"
  # first StructuredOutput block: its message ordinal and index
  jq -rs '[.[]|select(.type=="stream_event")|.event] | reduce .[] as $e ({m:0,hit:null}; if $e.type=="message_start" then .m+=1 elif $e.type=="content_block_start" and $e.content_block.name=="StructuredOutput" and .hit==null then .hit={m:.m,i:$e.index} else . end) | .hit | "\(.m) \(.i)"' "$f" > "$OUT/$name.block"
  read -r m i < "$OUT/$name.block"
  jq -j --argjson m "$m" --argjson i "$i" -n '[inputs|select(.type=="stream_event")|.event] | reduce .[] as $e ({m:0,out:""}; if $e.type=="message_start" then .m+=1 elif $e.type=="content_block_delta" and $e.delta.type=="input_json_delta" and .m==$m and $e.index==$i then .out+=$e.delta.partial_json else . end) | .out' "$f" > "$OUT/$name.preview.txt"
}
for n in p1-world p2-cast p3-opening p4-continuation p5-canary-world; do emit "$n" "$HERE/evidence/$n/stdout.jsonl"; done
emit 0925-p3-opening-advisor "$R25/p3-opening.jsonl"
emit 0925-p3-opening-append "$R25/p3-opening-append.jsonl"
rm -f "$OUT"/*.block
ls "$OUT" | wc -l
