#!/usr/bin/env bash
# Generates the SYNTHETIC Claude stream variants in synthetic/ from small literal
# records whose shapes were copied from the live captures (evidence/p1-world,
# 2026-09-25 evidence). Nothing here is live output: these specify protocol
# edge/error behaviour that a real run should not be induced to produce.
# Expected outcomes are written independently in synthetic/expectations.json.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/synthetic"; mkdir -p "$OUT"

SID='"session_id":"00000000-0000-0000-0000-000000000000"'
init()   { printf '{"type":"system","subtype":"init","apiKeySource":"%s","model":"claude-sonnet-5-5","tools":["StructuredOutput"],"claude_code_version":"2.1.286",%s}\n' "${1:-none}" "$SID"; }
status() { printf '{"type":"system","subtype":"status","status":"requesting",%s}\n' "$SID"; }
mstart() { printf '{"type":"stream_event","event":{"type":"message_start","message":{"model":"claude-sonnet-5-5","id":"msg_%s","type":"message","role":"assistant","content":[]}},%s}\n' "$1" "$SID"; }
bstart() { # index type [name]
  if [ -n "${3:-}" ]; then printf '{"type":"stream_event","event":{"type":"content_block_start","index":%s,"content_block":{"type":"%s","id":"toolu_%s","name":"%s","input":{}}},%s}\n' "$1" "$2" "$1" "$3" "$SID"
  else printf '{"type":"stream_event","event":{"type":"content_block_start","index":%s,"content_block":{"type":"%s"}},%s}\n' "$1" "$2" "$SID"; fi; }
jdelta() { # index partial_json-string(raw JSON string literal content)
  printf '{"type":"stream_event","event":{"type":"content_block_delta","index":%s,"delta":{"type":"input_json_delta","partial_json":%s}},%s}\n' "$1" "$2" "$SID"; }
tdelta() { printf '{"type":"stream_event","event":{"type":"content_block_delta","index":%s,"delta":{"type":"text_delta","text":%s}},%s}\n' "$1" "$2" "$SID"; }
bstop()  { printf '{"type":"stream_event","event":{"type":"content_block_stop","index":%s},%s}\n' "$1" "$SID"; }
mstop()  { printf '{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"%s"},"usage":{}},%s}\n{"type":"stream_event","event":{"type":"message_stop"},%s}\n' "${1:-tool_use}" "$SID" "$SID"; }
user()   { printf '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Structured output provided successfully"}]},%s}\n' "$SID"; }
ratelimit() { printf '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"},%s}\n' "$SID"; }
USAGE='{"input_tokens":2,"cache_creation_input_tokens":10,"cache_read_input_tokens":5,"output_tokens":7}'
result() { # structured_output-json-or-"" is_error subtype [usage]
  local so=""; [ -n "$1" ] && so="\"structured_output\":$1,"
  printf '{"type":"result","subtype":"%s","is_error":%s,"api_error_status":null,"terminal_reason":"completed",%s"total_cost_usd":0.0125,"usage":%s,"modelUsage":{"claude-sonnet-5-5":{"costBasis":"list","provider":"firstParty"}},"result":"ignored by the codec",%s}\n' "$3" "$2" "$so" "${4:-$USAGE}" "$SID"; }

PAYLOAD='{"narrative":"A \"quoted\" é雪","n":1}'
# Preview fragments deliberately differ in bytes (extra spaces, empty first
# fragment) from the compact result payload, as in the live captures.
payload_block() { # index
  bstart "$1" tool_use StructuredOutput
  jdelta "$1" '""'
  jdelta "$1" '"{\"narrative\": \"A \\\"quo"'
  jdelta "$1" '"ted\\\" é雪\", \"n\": 1}"'
  bstop "$1"; }
head_ok() { init; status; mstart 1; }
tail_ok() { mstop; user; ratelimit; result "$PAYLOAD" false success; }

w() { cat > "$OUT/$1.jsonl"; }
head_ok | { cat; payload_block 0; tail_ok; } | w success-minimal
{ head_ok; bstart 0 thinking; bstop 0; bstart 1 text; tdelta 1 '"meta commentary, not narrative"'; bstop 1
  bstart 2 server_tool_use advisor; jdelta 2 '"{\"question\":\"x\"}"'; bstop 2; bstart 3 advisor_tool_result; bstop 3
  bstart 4 thinking; bstop 4; payload_block 5; tail_ok; } | w advisor-and-text-before-payload
{ head_ok; payload_block 0; tail_ok; } | sed 's/$/\r/' | w crlf-records
{ head_ok; payload_block 0; tail_ok; } | head -c -1 | w no-trailing-newline
{ head_ok; payload_block 0; mstop; user; result "$PAYLOAD" false success '{"input_tokens":2,"output_tokens":7}'; } | w usage-partial
{ head_ok; payload_block 0; mstop; user; result "$PAYLOAD" false success '{}'; } | w usage-missing
{ head_ok; payload_block 0; mstop; user; result "$PAYLOAD" false success '{"input_tokens":2,"cache_creation_input_tokens":"x","cache_read_input_tokens":5,"output_tokens":7}'; } | w usage-malformed-component
# Second assistant message: the first attempt's payload block was abandoned; the
# result is authoritative and preview must stop at the first block's stop.
{ init; status; mstart 1; payload_block 0; mstop; user; mstart 2; bstart 0 tool_use StructuredOutput
  jdelta 0 '"{\"narrative\": \"SECOND"'; bstop 0; mstop; ratelimit; result "$PAYLOAD" false success; } | w payload-block-in-second-message
# Live-shaped (reviews/2026-10-02-claude-step1/evidence/p3-opening): the model wrote prose as a
# plain text block, the CLI injected an enforce prompt, and message 2 called StructuredOutput
# at index 0 again. Indices restart per message.
user_text() { printf '{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[structured-output-enforce] You MUST call the StructuredOutput tool to complete this request. Call this tool now."}]},%s}\n' "$SID"; }
{ init; status; mstart 1; bstart 0 thinking; bstop 0; bstart 1 text; tdelta 1 '"prose written as plain text"'; bstop 1; mstop end_turn
  ratelimit; user_text; status; mstart 2; payload_block 0; mstop; user; ratelimit; result "$PAYLOAD" false success; } | w enforced-payload-in-second-message
{ printf '{"type":"system","subtype":"commands_changed","commands":[],%s}\n' "$SID"; head_ok; payload_block 0; tail_ok; } | w commands-changed-before-init
# --- errors / order ---
{ head_ok; payload_block 0; mstop; user; ratelimit; result "$PAYLOAD" true success; } | w is-error-true-with-payload
{ head_ok; payload_block 0; mstop; user; ratelimit; result "$PAYLOAD" false error_max_turns; } | w subtype-not-success
{ head_ok; payload_block 0; mstop; user; ratelimit; } | w missing-result
{ head_ok; payload_block 0; tail_ok; result "$PAYLOAD" false success; } | w duplicate-result-identical
{ head_ok; payload_block 0; tail_ok; ratelimit; } | w record-after-result
{ head_ok; payload_block 0; mstop; user; result "" false success; } | w result-without-structured-output
{ head_ok; payload_block 0; mstop; user; result '"a string"' false success; } | w structured-output-string
{ head_ok; payload_block 0; mstop; user; result '[1,2]' false success; } | w structured-output-array
{ head_ok; payload_block 0; mstop; user; result 'null' false success; } | w structured-output-null
{ head_ok; payload_block 0; mstop; user; result '{"narrative":"a","narrative":"b"}' false success; } | w structured-output-duplicate-keys
{ head_ok; payload_block 0; payload_block 1; tail_ok; } | w two-payload-blocks-same-message
{ head_ok; jdelta 0 '"{\"narrative\": \"x"'; tail_ok; } | w delta-for-unstarted-index
{ head_ok; payload_block 0; printf '{"type":"mystery","detail":1,%s}\n' "$SID"; tail_ok; } | w unknown-top-level-type
{ init ANTHROPIC_API_KEY; status; mstart 1; payload_block 0; tail_ok; } | w metered-api-key-source
{ status; mstart 1; payload_block 0; tail_ok; } | w missing-init
{ head_ok; payload_block 0; tail_ok; } | head -c -12 | w truncated-final-record
{ head_ok; payload_block 0; printf '\xff\xfe not utf8\n'; tail_ok; } | w invalid-utf8-record
{ head_ok; payload_block 0; printf '[1,2,3]\n'; tail_ok; } | w non-object-record
{ result "" true success '{}' | sed 's/"result":"ignored by the codec"/"result":"There is an issue with the selected model (x)."/;s/"terminal_reason":"completed"/"terminal_reason":"api_error"/'; } | w result-only-api-error
ls "$OUT" | wc -l
