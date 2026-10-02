#!/usr/bin/env bash
# Cross-checks synthetic/expectations.json against the generated variants: every
# variant has an expectation and vice versa, and each located record really has
# the stated type. No model calls.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; S="$HERE/synthetic"; bad=0
for f in "$S"/*.jsonl; do n="$(basename "$f" .jsonl)"
  jq -e --arg n "$n" '.variants[$n]' "$S/expectations.json" >/dev/null || { echo "no expectation for $n"; bad=1; }; done
for n in $(jq -r '.variants|keys[]' "$S/expectations.json"); do
  f="$S/$n.jsonl"; [ -f "$f" ] || { echo "no file for $n"; bad=1; continue; }
  rec="$(jq -r --arg n "$n" '.variants[$n].error_record // empty' "$S/expectations.json")"
  want="$(jq -r --arg n "$n" '.variants[$n].record_type // empty' "$S/expectations.json")"
  [ -z "$rec" ] && continue
  if [ -n "$want" ]; then
    got="$(sed -n "${rec}p" "$f" | jq -r '.type + (if .type=="system" then "/"+.subtype elif .type=="stream_event" then "/"+.event.type else "" end)' 2>/dev/null)"
    [ "$got" = "$want" ] || { echo "$n: record $rec is '$got', expected '$want'"; bad=1; }
  fi
done
[ $bad = 0 ] && echo "synthetic expectations consistent ($(ls "$S"/*.jsonl | wc -l) variants)"; exit $bad
