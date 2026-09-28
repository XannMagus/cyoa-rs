# Synthetic Codex story acceptance fixture

`story.json` is a frozen derivative of `../phase0_story.json`, not live model
output. The merchant is renamed to Ajax the sailmaker by stable ID `ajax`, then
updated independently from namesake guard `ajax-2` on the next turn. The exact
NPC duplicate, distinct playable/NPC namesakes, unknown action kind, empty scene,
chapter opening/retitling, event sequence and null/empty thread updates are retained.

`codex_story_acceptance.rs` wraps these hand-authored payloads in the fixed
thread.started / turn.started / item.completed(agent_message) / turn.completed
JSONL shape, with CRLF framing and deliberately absent optional telemetry. Error
cases replace the terminal, exit status or payload explicitly. This helper does
not call the production codec and is not an oracle for story semantics. Assertions
spell out expected state, identities, events, prose and subprocess counts.

Some payload fields are omitted to exercise the established tolerant incoming
wire defaults; this is not a claim that a live schema-constrained model omits
required fields. Cast responses deliberately include NPC candidates even in the
zero-NPC test, to verify application filtering as well as the outgoing empty-list
instruction/schema. The existing Phase 0 fixture and its tests are unchanged.
