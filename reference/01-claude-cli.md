# `claude -p` as the inference backend — verified findings

See PLAN.md's "The `claude -p` backend" section for the narrative version;
this file has the raw evidence. Full reproduction, scripts and redacted
transcripts for the 2026-09-25 refresh are in
[`reviews/2026-09-25-claude-cli-refresh/`](../reviews/2026-09-25-claude-cli-refresh/README.md).

## Confirmed (CLI 2.1.282, 2026-09-25)

Checked against the real, installed `claude` binary, subscription auth
(`authMethod: claude.ai`, `apiProvider: firstParty`, team subscription), no
`ANTHROPIC_*` variables set. Live probes replayed the real bundled
world/cast/opening-turn/continuation-turn requests (rendered by the actual
`GenerationTemplates`, not hand-written test JSON) — see the linked review for
exact commands, full transcripts and reproduction steps.

### The invocation

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

User prompt goes on **stdin**, not as an argv string.

| Flag | Confirmed behavior |
|---|---|
| `--safe-mode` | Disables CLAUDE.md, skills, plugins, hooks, MCP servers, custom agents. Does **not** disable the account-level advisor policy — see below. |
| `--tools ""` | Disables client-declared tools (the init event's `tools` array). Does **not** block the server-side `advisor` tool, which never appears in that array. |
| `--permission-prompts none` | Anything that would prompt is auto-denied. |
| `--no-session-persistence` | No transcript files accumulate per call. |
| `--json-schema '<schema>'` | Native structured output, implemented as a forced `StructuredOutput` tool call. Rejects a root `"$schema"` key outright (see below); accepts `$defs`/`$ref`. Takes inline JSON only — **no file-path variant exists** in 2.1.282. |
| `--output-format stream-json --verbose --include-partial-messages` | All three required together for incremental deltas. |
| `--system-prompt <text>` / `--append-system-prompt <text>` | Inline string only. **`--system-prompt-file`/`--append-system-prompt-file` do not exist** in 2.1.282's flag list, despite being referenced by name in the `--safe-mode` help text — checked directly against `claude --help`, not assumed. For calls near the argv size limit (see below), there is no file-based escape hatch. |
| `--settings <file-or-json>` | Merges additional settings. Confirmed it does **not** suppress the advisor policy (see below) — org-level policy overrides it. |

### ⚠️ `--bare` is a trap — do not use it

`--bare`'s own `--help` text: *"Anthropic auth is strictly `ANTHROPIC_API_KEY`
or apiKeyHelper via `--settings` (OAuth and keychain are never read)."* This
would force metered API-key billing. `--safe-mode` gives equivalent isolation
without touching auth.

### ⚠️ The advisor server-tool tax — permanent, org-level, unavoidable locally

**Every** live structured-output call observed this session — world, cast,
opening turn, continuation turn, and the SIGTERM probe — included an
unrequested `server_tool_use` block named `advisor`, followed by an
`advisor_tool_result` block, before the real `StructuredOutput` call. This is
a genuine second model call (to `claude-opus-5-5`, visible in
`usage.iterations`), adding real latency (the world-request probe took 60.9s
total) and its own token cost.

Three independent *availability* attempts all failed — stripped environment,
`--settings` override, and the experimental-advisor env var set to `0`. Root
cause per `claude doctor`: `Organization policy: Loaded from
api.anthropic.com` — the tool's *availability* is enforced server-side by
Anthropic for the authenticated org, not controllable via any local
flag/setting/environment. See the linked review's "Confirmed findings"
section for each attempt and its evidence file.

**A fourth lever, suppressing the model's *choice* to invoke it rather than
its availability, works:**

```
--append-system-prompt "Do not consult the advisor tool for this task. Answer directly."
```

Confirmed on both the world request and the opening-turn request (repeated
independently, not the same call): zero `server_tool_use`/`advisor_tool_result`
blocks in either transcript, `is_error: false`, valid structured output, no
stray `text` block ahead of `StructuredOutput` either (content blocks went
straight `thinking` → `tool_use`). No bleed-through of the appended
instruction's wording into generated narrative/description text (checked for
"advisor" in both). Duration dropped from the 60.9s advisor-inclusive baseline
to 29–31s, consistent with the advisor round-trip being the source of the
extra time, not a coincidence. Evidence:
`reviews/2026-09-25-claude-cli-refresh/evidence/{p1-world-append,p3-opening-append}.jsonl`.

**Architectural placement:** this is unambiguously a `claude -p`-specific
tolerance adjustment — Codex has no advisor tool, so this instruction can
never be shared/generic. Per ARCH-003, it belongs in
`generation::backend_compat::claude_cli` alongside `adapt_schema`, appended to
the invocation only when building a `ClaudeCliBackend` call — never mixed into
`GenerationTemplates`'s backend-agnostic `instructions` string that the Codex
adapter also consumes. Not yet implemented (the invocation builder itself is
Phase 1 item 3/4 work); recorded here as a confirmed requirement for that
item.

**Consequence for this project:** with the appended instruction in place at
implementation time, the advisor tax should not apply in practice — but the
underlying org-level *availability* remains permanent and account-specific
(see above), so item 2's timeout defaults should still budget generously
rather than assume the suppression instruction is bulletproof against every
possible request shape, and item 9's live-acceptance run should confirm it
holds across the full turn sequence, not just these two isolated probes.

### Content-block identity: text and tool blocks that are not the payload

A single opening-turn call produced, in order: `thinking`, **`text`**,
`server_tool_use` (`advisor`), `advisor_tool_result`, `thinking`,
`tool_use` (`StructuredOutput`). The `text` block's content was the model's
own meta-commentary about the task, not narrative — a live, observed instance
of the Phase 1 plan's existing warning that "ordinary intermediate messages
must not be concatenated into a pretend final structured response." The
payload must be identified by `content_block_start.content_block.name ==
"StructuredOutput"`, and only that block's `index`'s `input_json_delta`
events collected — never "the first text-shaped content," never all
`content_block_delta` events unconditionally.

Confirmed: the concatenated `partial_json` for the `StructuredOutput` block's
index is byte-identical (order-insensitive JSON comparison) to the final
`result.structured_output` object. TEXT-001's raw-span-extraction premise
holds for `claude -p`.

### `$defs`/`$ref` and the root `$schema` rejection

A root `"$schema"` key is rejected outright, not merely ignored:

```
Error: --json-schema is not a valid JSON Schema: no schema with key or ref "https://json-schema.org/draft/2020-12/schema"
```

Removing only that key (leaving `$defs`/`$ref` untouched) fixes it
immediately, with no model call made at all (fails before invocation — no
advisor tax, no generation cost). `$defs`/`$ref` schemas otherwise work
correctly, including nested field descriptions actually being read and
followed rather than pattern-matched from examples (canary-tested against a
made-up field rule stated only in a `$ref`'d description). This is `claude
-p`'s own tolerance limit, scoped to
`generation::backend_compat::claude_cli::adapt_schema` alone; the generic
schema builders keep the key for every other consumer.

### Required fields after the R5 repair

All 8 NPCs generated in the cast-request probe had non-empty `relationships`
populated, consistent with `wire.rs`'s current required (no
`#[serde(default)]`) field. (Prior version of this file said `relationships`
was "optional-with-default" — that was stale; R5 made it required. n=1 this
session, no omission observed.)

### Argv size

The three argv strings a live call sends (system prompt, schema, and the user
prompt on stdin is *not* argv) measured 1.2KB (world) up to ~14.4KB
(opening/continuation turn instructions+schema) using the project's actual
bundled requests. The relevant OS limit on Linux is `MAX_ARG_STRLEN`
(128KiB **per single argument**), not the 2MB `ARG_MAX` total-argv figure.
~9x headroom at current sizes; re-check if `Limits` configuration (very high
NPC caps, etc.) ever multiplies schema size substantially. No file-based
system-prompt/schema alternative exists to fall back on if this limit is
ever approached (see above).

### Error shapes

- **Malformed/rejected schema** (root `$schema` present): exit 1, empty
  stdout, error text on stderr only, no result envelope, no model call.
- **Invalid `--model`**: exit 1, but *does* emit a `result` envelope on
  stdout with `is_error: true`, `api_error_status: 404`,
  `terminal_reason: "api_error"` — note `subtype: "success"` is present
  despite the error; **check `is_error`, not `subtype`**. The actually
  informative line is on stderr: `Warning: Advisor disabled — base model
  '<model>' has no advisor rank in the model catalog. Switch to a public
  model alias (opus, sonnet, fable) or set
  CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL=1.` — this env var only
  affects models outside the recognized catalog; confirmed it does not
  disable advisor for `sonnet`/`opus`.

### SIGTERM mid-stream — inconclusive on timing, but a real data point

Sending `SIGTERM` to the `claude` process alone (not its process group), mid
in-flight during the advisor call, did not produce observably prompt
termination — the harness's own wait did not return within ~196s before an
outer watchdog fired. No processes were found running afterward, but exact
timing wasn't captured. Treat as one data point supporting, not proving, the
Phase 1 plan's existing requirement for group-level (`killpg`) signaling plus
an explicit reap/timeout in the process supervisor (item 3), rather than a
single-PID `SIGTERM` with an assumed-prompt exit. Full transcript and script:
see the linked review.

## Historical (CLI 2.1.281, 2026-09-24) — not re-run this session

- Baseline overhead ≈1000 input tokens per call even with `--safe-mode`.
- `total_cost_usd` uses `"costBasis":"list"` — **notional list pricing, not
  subscription billing.** Don't present it to the player as money spent;
  either omit it or label it clearly as an estimate.
- `--max-budget-usd` exists as an optional guard rail (not re-tested this
  session).
- A system prompt that mentions JSON/formatting *alongside* `--json-schema`
  causes double-encoding (the model serializes a nested object as a string
  value). Fix: with `--json-schema`, describe fields semantically only, never
  mention JSON/formatting in the system prompt. Direct porting consequence:
  `prompts.toml`'s JSON-formatting fragments from calibre's prompt-based
  fallback path must stay stripped (already true as of the R-series repairs);
  `markdown_instructions()` (governs prose *inside* `narrative`) is unrelated
  and must stay.
- No automatic queueing on rate limit — a non-zero exit plus an error
  message. Matches calibre's "never auto-retry, let the player retry"
  posture.

## Open questions (documented or partially observed, not fully exercised)

- **Exit codes 130/143 on signal** — not cleanly observed this session (the
  SIGTERM probe's timing was inconclusive; see above). `2` on budget-ceiling
  partial completion — not tested this session, from 2026-09-24 documentation
  reading of `--help`, not a live observation.
- **Rate-limit / budget-exceeded response shape** — not induced (would waste
  a real rate limit); any future fixture for this must be labeled synthetic.
- **Long-narrative genuine incremental streaming at scale** — P3/P4 showed
  real incremental deltas, but on relatively short generated turns; not
  stress-tested with a long narrative.
- **Whether the advisor tax is universal or specific to this org's policy** —
  this session can only speak for the authenticated org
  (`subscriptionType: team`). A different account/org may not carry it, or
  may carry a different one.
- **Account/tool isolation under concurrent calls** — not tested.
- **Process-group cleanup of a descendant that retains a pipe past the
  killed child's exit** — not directly observed; the SIGTERM probe tested
  single-PID signaling only, not the group-kill approach item 3 will
  implement.
