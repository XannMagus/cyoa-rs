#!/usr/bin/env bash
# Discovery-only derivatives; original bundled requests must already exist.
set -euo pipefail
requests=${1:?path to a request directory emitted by dump}
for pair in 'opening_turn opening_ref_union' 'continuation_turn continuation_ref_union'; do
    read -r source target <<< "$pair"
    mkdir -p "$requests/$target"
    cp "$requests/$source/instructions.txt" "$requests/$source/prompt.txt" "$requests/$target/"
    # These frozen schemas have only schema-bearing $ref objects. Production step
    # 2 must use a schema-node walk, not this discovery-only arbitrary JSON walk.
    jq 'walk(if type == "object" and has("$ref") and has("description") then .anyOf = [{"$ref": ."$ref"}] | del(."$ref") else . end)' \
        "$requests/$source/schema.codex-discovery.json" > "$requests/$target/schema.codex-discovery.json"
done
for kind in null empty; do
    mkdir -p "$requests/turn_$kind"
    cp "$requests/continuation_ref_union/"{instructions.txt,prompt.txt,schema.codex-discovery.json} "$requests/turn_$kind/"
done
cat >> "$requests/turn_null/prompt.txt" <<'TEXT'

DISCOVERY-ONLY RESPONSE-SHAPE PROBE: Keep the chapter title and upcoming threads unchanged: set chapter_title to null and summary_update.upcoming_events to null. Use empty strings in durable character fields that have not changed. The passage can be short for this shape probe.
TEXT
cat >> "$requests/turn_empty/prompt.txt" <<'TEXT'

DISCOVERY-ONLY RESPONSE-SHAPE PROBE: Resolve the existing Depart thread without introducing another open thread: set summary_update.upcoming_events to [] and chapter_title to null. Use empty strings in durable character fields that have not changed. The passage can be short for this shape probe.
TEXT
