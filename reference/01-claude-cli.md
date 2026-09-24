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

## Verified test #3 — `$defs`/`$ref` in `--json-schema` (2026-09-24)

Re-checked `claude --help` first (version `2.1.281`): every flag below still
exists with the described behavior.

Cast doubt on before this test: whether `--json-schema` accepts a schema
whose nested types are expressed as `$ref`s into a `$defs` map (schemars'
default output shape), as opposed to needing everything inlined. Resolved:
**it does, and it correctly uses `$ref`'d nested field descriptions to guide
generation.**

**Sub-finding first — a root `"$schema"` key is rejected outright**, not
merely ignored:

```bash
$ echo '...' | claude -p --json-schema '{"$schema":"https://json-schema.org/draft/2020-12/schema", ...}' ...
Error: --json-schema is not a valid JSON Schema: no schema with key or ref "https://json-schema.org/draft/2020-12/schema"
```

Removing the `"$schema"` key (leaving `$defs`/`$ref` otherwise untouched)
fixed this immediately. This is `claude -p`'s own tolerance limit, not a
defect in the schema or something every backend need care about — don't
disable `"$schema"` generation globally. `cyoa-infrastructure`'s
`generation::schema` module keeps its generic builders fully
standards-compliant (`"$schema"` included) and strips it only in a
`claude_cli`-specific submodule, applied immediately before a `claude -p
--json-schema` call and nowhere else.

**Canary test for `$ref` resolution:** a schema referencing `#/$defs/Detail`,
whose *only* copy of a made-up, arbitrary formatting rule lived in the
`$defs` entry's field `description` (never restated in the system/user
prompt):

```json
{"type":"object","properties":{"narrative":{"type":"string","description":"One short sentence of story text."},"detail":{"$ref":"#/$defs/Detail"}},"required":["narrative","detail"],"additionalProperties":false,"$defs":{"Detail":{"type":"object","properties":{"code":{"type":"string","description":"A short tracking code. It MUST begin with the exact literal prefix 'ZQ7-' followed by exactly three digits, e.g. 'ZQ7-482'. This exact rule is stated nowhere else."}},"required":["code"],"additionalProperties":false}}}
```

System prompt said only *"narrative = one short sentence. detail = the
requested nested object"* — no mention of the `ZQ7-` rule anywhere outside
the `$ref`'d description. Result:
```json
{"narrative":"The lighthouse beam swept across the churning night sea.","detail":{"code":"ZQ7-114"}}
```
Repeated with a different, more unusual constraint (a `PLK-` prefix, "two
lowercase Greek letters joined by an underscore", with a deliberately stale
numeric example still present in the same description text) to rule out
coincidence:
```json
{"narrative":"...","detail":{"code":"PLK-alpha_beta"}}
```
The model followed the *stated rule* over the stale example — evidence this
is genuine comprehension of the nested description, not pattern-matching an
example string.

**End-to-end confirmation against this project's actual generated schema:**
`generation::schema::generated_cast_schema` (with `"$schema"` now removed at
the source) was sent as-is, with the real `cast_generation_prompt` instructions/
prompt text for a test world ("The Mist City"). Result: a valid `GeneratedCast`
— 4 distinct playable characters and 7 NPCs, every required field populated,
`relationships` (a `$ref`'d, optional-with-default field two levels of
nesting deep) filled in coherently for every NPC. `is_error: false`,
`stop_reason: "tool_use"`.

**Conclusion:** `$defs`/`$ref` schemas work with `claude -p --json-schema`,
including nested field descriptions actually being read and followed. The
only adaptation required — dropping the root `"$schema"` key — is scoped to
`claude -p` alone, in `generation::backend_compat::claude_cli::adapt_schema`;
the generic schema this project generates for any backend keeps that key
(`generic_schemas_declare_a_root_schema_key`,
`adapt_schema_strips_the_root_schema_key_and_nothing_else`).
