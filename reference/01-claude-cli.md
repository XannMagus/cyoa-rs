# `claude -p` as the inference backend — verified findings

Everything here was checked against the real, installed `claude` binary in
this session (not taken from documentation, which turned out to recommend a
flag — `--bare` — that would have broken the whole point of this project).
See PLAN.md's "The `claude -p` backend" section for the narrative version;
this file has the raw evidence.

## The invocation

```bash
claude -p \
  --safe-mode \
  --tools "" \
  --permission-prompts none \
  --no-session-persistence \
  --model sonnet \
  --system-prompt "<instructions>" \
  --json-schema '<json schema>' \
  --output-format stream-json --verbose --include-partial-messages
```

User prompt goes on **stdin**, not as an argv string (prompts run to many KB
and contain arbitrary player text — shell-arg escaping would be a real
hazard).

| Flag | Verified behavior |
|---|---|
| `--safe-mode` | Disables CLAUDE.md, skills, plugins, hooks, MCP servers, custom agents. Auth, model selection, permissions unaffected. |
| `--tools ""` | Disables all built-in tools. Structured output still worked in testing — see below, it's implemented as a forced tool call regardless. |
| `--permission-prompts none` | Anything that would prompt is auto-denied — process can never hang on a permission dialog. |
| `--no-session-persistence` | No transcript files accumulate per call. |
| `--json-schema '<schema>'` | Native structured output — confirmed working, see transcript below. |
| `--output-format stream-json --verbose --include-partial-messages` | All three required together for incremental deltas. |

## ⚠️ `--bare` is a trap — do not use it

`--bare`'s own `--help` text: *"Anthropic auth is strictly `ANTHROPIC_API_KEY`
or apiKeyHelper via `--settings` (OAuth and keychain are never read)."*

This would force metered API-key billing and defeat the entire reason this
project exists (riding the Claude Code subscription, not paying per token).
`--safe-mode` gives equivalent isolation (no CLAUDE.md/skills/plugins/hooks/
MCP) **without** touching auth.

## Verified test #1 — plain structured output (`--output-format json`)

Command:
```bash
echo 'Invent one fictional tavern. Reply as JSON only.' | claude -p \
  --safe-mode --tools "" --permission-prompts none --no-session-persistence \
  --model sonnet --output-format json \
  --system-prompt 'You are a terse fiction generator. Output only JSON.' \
  --json-schema '{"type":"object","properties":{"name":{"type":"string"},"description":{"type":"string"}},"required":["name","description"],"additionalProperties":false}'
```

Result envelope (trimmed):
```json
{
  "total_cost_usd": 0.00664,
  "usage": {"input_tokens": 1020, "output_tokens": 460, ...},
  "structured_output": {"name": "tavern", "description": "..."},
  "result": "{...same JSON as a string...}",
  "stop_reason": "tool_use",
  "num_turns": 2,
  "is_error": false,
  "session_id": "...",
  ...
}
```

**⚠️ Gotcha found here:** the system prompt said *"Output only JSON"* while
`--json-schema` was also set. The model double-encoded — it serialized an
entire nested JSON object as the *string value* of the `description` field,
because it was being told both "there's a schema, fill these fields" and
"output raw JSON text" at once. **Fix: with `--json-schema`, never mention
JSON/formatting in the system prompt — describe fields semantically only.**
This has a direct porting consequence: `prompts.toml` must have the
JSON-formatting fragments from calibre's prompt-based fallback path stripped
out (they exist there because calibre supports providers with no native
structured-output mode; `markdown_instructions()`, which governs the prose
*inside* `narrative`, is unrelated and must stay).

## Verified test #2 — streaming (`--output-format stream-json`)

Command (system prompt corrected per the finding above):
```bash
echo 'Write a two-sentence scene in a tavern.' | claude -p \
  --safe-mode --tools "" --permission-prompts none --no-session-persistence \
  --model sonnet --output-format stream-json --verbose --include-partial-messages \
  --system-prompt 'You are a novelist. narrative = the prose passage. mood = one word.' \
  --json-schema '{"type":"object","properties":{"narrative":{"type":"string"},"mood":{"type":"string"}},"required":["narrative","mood"],"additionalProperties":false}'
```

Observed event stream (newline-delimited JSON, condensed):
```
{"type":"system","subtype":"init", ...}
{"type":"system","subtype":"status", ...}
{"type":"stream_event","event":{"type":"message_start", ...}}
{"type":"stream_event","event":{"type":"content_block_start",
  "content_block":{"type":"tool_use","id":"toolu_...","name":"StructuredOutput","input":{}}}}
{"type":"stream_event","event":{"type":"content_block_delta",
  "delta":{"type":"input_json_delta","partial_json":""}}}
{"type":"stream_event","event":{"type":"content_block_delta",
  "delta":{"type":"input_json_delta","partial_json":"{\"narrative\": \"The firelight gu"}}}
{"type":"stream_event","event":{"type":"content_block_delta",
  "delta":{"type":"input_json_delta","partial_json":"ttered as the"}}}
... (many more small fragments) ...
{"type":"stream_event","event":{"type":"content_block_delta",
  "delta":{"type":"input_json_delta","partial_json":"\", \"mood\": \"t"}}}
{"type":"stream_event","event":{"type":"content_block_delta",
  "delta":{"type":"input_json_delta","partial_json":"ense"}}}
{"type":"stream_event","event":{"type":"content_block_delta",
  "delta":{"type":"input_json_delta","partial_json":"\"}"}}}
{"type":"assistant", ...}
{"type":"stream_event","event":{"type":"content_block_stop"}}
{"type":"user", ...}
{"type":"stream_event","event":{"type":"message_delta"}}
{"type":"result","subtype":"success","structured_output":{...},"total_cost_usd":...,"usage":{...}, ...}
```

**Key findings:**

1. Structured output is implemented as a **forced tool call** named
   `StructuredOutput` (`stop_reason: "tool_use"`, `num_turns: 2`). `--tools ""`
   does not block it — it's a distinct mechanism from user-facing tools.
2. The `partial_json` fragments in `content_block_delta` events **are exactly
   the raw-JSON-text stream** that `StreamingStringField` (see
   `00-engine-notes.md`) is designed to consume. No adaptation needed: feed
   each `partial_json` string into the scanner in order, and it yields
   `narrative`'s decoded characters as they arrive — assuming `narrative`
   stays the first property emitted, which depends on JSON key order (see
   PLAN.md's note on `serde_json/preserve_order`).
3. The final `{"type":"result"}` line carries a pre-parsed
   `structured_output` object — the engine should still validate it
   (`validated_turn` etc.), but doesn't need to re-parse accumulated JSON
   itself.

## Other verified/documented facts

- Baseline overhead ≈1000 input tokens per call even with `--safe-mode`.
- `total_cost_usd` uses `"costBasis":"list"` — **notional list pricing, not
  subscription billing.** Don't present it to the player as money spent;
  either omit it or label it clearly as an estimate.
- `--max-budget-usd` exists as an optional guard rail.
- Exit codes: `0` success, `1` failure (including rate limits), `2` partial
  (e.g. budget ceiling hit mid-run), `130`/`143` on signals.
- Also check `is_error` in the result envelope, not just the exit code.
- No automatic queueing on rate limit — a non-zero exit plus an error message.
  Matches calibre's "never auto-retry, let the player retry" posture exactly,
  so no adaptation needed in the engine's error posture.
- Full CLI help was read directly (`claude --help`) to confirm every flag
  above actually exists with the described behavior, rather than trusting
  secondhand documentation — worth doing again if the CLI's flags have
  changed by the time this is implemented.
