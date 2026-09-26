#!/usr/bin/env bash
# Handcrafted expectations, never represented as live vendor observations.
set -euo pipefail
cd -- "$(dirname -- "$0")"
cat > success.jsonl <<'JSONL'
{"type":"thread.started","thread_id":"synthetic-thread"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":" \r\n{\"title\":\"Harbour\",\"world_description\":\"A sheltered harbour — 灯\"}\n"}}
{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5}}
JSONL
jq -c 'select(.type != "turn.completed")' success.jsonl > missing-terminal.jsonl
jq -sc '. + [.[-1]] | .[]' success.jsonl > duplicate-terminal.jsonl
jq -sc '. + [{type:"turn.failed",error:{message:"Synthetic late failure"}}] | .[]' success.jsonl > conflicting-terminal.jsonl
jq -sc '.[0:3] + [{type:"turn.failed",error:{message:"Synthetic rate limit; not induced live"}}] | .[]' success.jsonl > failed-after-candidate.jsonl
jq -sc '.[0:3] + [ (.[2] | .item.id = "item_1") ] + .[3:] | .[]' success.jsonl > ambiguous.jsonl
jq -sc '.[0:2] + [{type:"item.completed",item:{id:"comment",type:"agent_message",text:"Working on it"}}] + .[2:] | .[]' success.jsonl > commentary-then-json.jsonl
jq -c 'if .type == "item.completed" then .item.type = "reasoning" else . end' success.jsonl > ineligible.jsonl
jq -sc '.[0:2] + [.[3], .[2]] | .[]' success.jsonl > wrong-order.jsonl
jq -sc '.[0:2] + [{type:"future.completion",value:"unknown"}] + .[2:] | .[]' success.jsonl > unknown-event.jsonl
jq -sc '.[0:2] + [{type:"error",message:"Reconnecting... synthetic notice"}] + .[2:] | .[]' success.jsonl > reconnect-notice.jsonl
jq -c 'if .type == "item.completed" then del(.item.text) else . end' success.jsonl > missing-text.jsonl
jq -c 'if .type == "turn.completed" then del(.usage) else . end' success.jsonl > absent-usage.jsonl
jq -c 'if .type == "turn.completed" then .usage = {input_tokens:0,cached_input_tokens:0,output_tokens:0} else . end' success.jsonl > zero-usage.jsonl
jq -c 'if .type == "turn.completed" then .usage.cached_input_tokens = 11 else . end' success.jsonl > invalid-cache.jsonl
jq -c '. + {future_metadata:"harmless envelope addition"}' success.jsonl > extra-metadata.jsonl
head -c -1 success.jsonl > no-final-newline.jsonl
sed 's/$/\r/' success.jsonl > crlf.jsonl
head -c -12 success.jsonl > truncated.jsonl
printf '\377\n' > invalid-utf8.jsonl
cat > expectations.json <<'JSON'
{
  "provenance": "Entirely synthetic; success payload and expectations hand-authored. Not results of an implemented codec.",
  "success_payload": " \r\n{\"title\":\"Harbour\",\"world_description\":\"A sheltered harbour — 灯\"}\n",
  "cases": [
    {"file":"success.jsonl","exit":0,"expected":"accept_exact_payload"},
    {"file":"success.jsonl","exit":1,"expected":"reject_process_failure_retain_candidate"},
    {"file":"missing-terminal.jsonl","exit":0,"expected":"reject_missing_terminal_retain_candidate"},
    {"file":"duplicate-terminal.jsonl","exit":0,"expected":"reject_duplicate_terminal"},
    {"file":"conflicting-terminal.jsonl","exit":0,"expected":"reject_conflicting_terminal"},
    {"file":"failed-after-candidate.jsonl","exit":0,"expected":"reject_terminal_failure_retain_candidate"},
    {"file":"ambiguous.jsonl","exit":0,"expected":"reject_multiple_agent_messages_even_identical_payloads"},
    {"file":"commentary-then-json.jsonl","exit":0,"expected":"reject_multiple_agent_messages_never_first_parseable"},
    {"file":"ineligible.jsonl","exit":0,"expected":"reject_no_agent_candidate"},
    {"file":"wrong-order.jsonl","exit":0,"expected":"reject_completion_before_candidate"},
    {"file":"unknown-event.jsonl","exit":0,"expected":"reject_unsupported_event"},
    {"file":"reconnect-notice.jsonl","exit":0,"expected":"reject_unsupported_recovery_even_if_cli_completes"},
    {"file":"missing-text.jsonl","exit":0,"expected":"reject_malformed_message"},
    {"file":"absent-usage.jsonl","exit":0,"expected":"accept_unknown_usage"},
    {"file":"zero-usage.jsonl","exit":0,"expected":"accept_known_zero_usage"},
    {"file":"invalid-cache.jsonl","exit":0,"expected":"accept_payload_normalize_cache_to_unknown"},
    {"file":"extra-metadata.jsonl","exit":0,"expected":"accept_ignore_extra_envelope_fields"},
    {"file":"no-final-newline.jsonl","exit":0,"expected":"accept_complete_json_record_without_newline"},
    {"file":"crlf.jsonl","exit":0,"expected":"accept_preserve_diagnostics_and_payload"},
    {"file":"truncated.jsonl","exit":0,"expected":"reject_invalid_final_record"},
    {"file":"invalid-utf8.jsonl","exit":0,"expected":"reject_invalid_protocol_utf8"}
  ]
}
JSON
