# Claude story acceptance fixture

The Claude composed acceptance test (`claude_story_acceptance.rs`) deliberately
reuses `../codex/story.json`: the story, identities, limits and expected state are
backend-agnostic project decisions, so one frozen scenario protects both adapters.
That file is a synthetic derivative of `../phase0_story.json`, not model output;
see `../codex/README.md` for what it retains.

Only the transport wrapper differs. `claude_story_acceptance.rs` wraps each
hand-authored payload in a Claude-shaped stream (init, status, message_start, a
thinking block, the `StructuredOutput` block streamed as small `input_json_delta`
fragments, a trailing `result` with usage and list-price cost), with CRLF framing.
Error cases replace the terminal explicitly (`is_error`, truncated record, missing
result) or the exit status; two cases stream a preview that disagrees with the final
payload and a live-shaped enforce retry (prose in a text block of message 1, the
payload in message 2). The helper does not call the production codec and is not an
oracle for story semantics; assertions spell out expected state, identities, events,
prose, provenance and subprocess counts. The `result` payload is spliced as text so
the span the adapter sees is exactly the authored payload.
