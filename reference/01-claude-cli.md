# `claude -p` as the inference backend — verified findings

See PLAN.md's "The `claude -p` backend" section for the narrative version;
this file has the raw evidence. Full reproduction, scripts and redacted
transcripts for the 2026-09-25 refresh are in
[`reviews/2026-09-25-claude-cli-refresh/`](../reviews/2026-09-25-claude-cli-refresh/README.md).

## Supported profile (CLI 2.1.286, 2026-10-02)

Frozen for the `ClaudeCliBackend` slice ([plan](../docs/plans/phase1-claude-adapter.md)).
Every statement here was observed in [`reviews/2026-10-02-claude-step1/`](../reviews/2026-10-02-claude-step1/README.md)
(subscription auth, `authMethod: claude.ai`, no `ANTHROPIC_*` variables in the child
environment); anything not observed is listed under "Not established" below, and the
older sections of this file remain the record of the 2026-09-25 observations.

**Production invocation.** Empty temporary cwd; environment limited to `HOME` and
`PATH` (the parent's `CLAUDE_CODE_*` variables are *not* passed); no `--model`;
user prompt on stdin, byte-exact:

```
claude -p --safe-mode --tools "" --permission-prompts none --no-session-persistence \
  --system-prompt <instructions> \
  --append-system-prompt "Do not consult the advisor tool for this task. Answer directly." \
  --json-schema <compact schema without root $schema> \
  --output-format stream-json --verbose --include-partial-messages
```

Observed with the real bundled outline, cast, opening and continuation requests
(templates byte-identical to the 2026-09-25 bundles): all four exit 0 with empty
stderr, in 18–39 s, stdout 140–298 KB. `HOME` + `PATH` alone preserve subscription
auth (`claude auth status --json` reports `loggedIn`, `claude.ai`, `firstParty`).
`init.apiKeySource` is `"none"`, `init.model` is `claude-sonnet-5-5` (the account
default, since no `--model` was passed) and `init.tools` is exactly
`["StructuredOutput"]`. Argument sizes: system prompt 622 B–6.4 KB, schema
438 B–6.9 KB, each far below Linux's 128 KiB `MAX_ARG_STRLEN`.

**Record vocabulary.** Top-level `type`/`subtype` values seen: `system/init`,
`system/status`, `system/thinking_tokens`, `system/commands_changed` (new in this
version; once it preceded `init`), `stream_event`, `assistant`, `user`,
`rate_limit_event`, `result/success`. `stream_event.event.type` values:
`message_start`, `content_block_start`/`_delta`/`_stop`, `message_delta`,
`message_stop`. Delta types: `input_json_delta`, `thinking_delta`, `signature_delta`,
`text_delta`. `result` was always the last record and appeared exactly once.

**Content-block indices restart per assistant message.** One of the four bundled-request
captures (`p3-opening`) contains **two** `message_start` records:
message 1 held `thinking` (index 0) and a plain `text` block (index 1) containing
prose, ended with `end_turn`; the CLI then injected a `user` record with text
`[structured-output-enforce] You MUST call the StructuredOutput tool to complete this
request. Call this tool now.`; message 2 called `StructuredOutput` at **index 0**.
`result.num_turns` was 3 and usage aggregates both messages. So a payload block is
identified by (message ordinal, block index), never by index alone, and prose in a
`text` block is not the payload even when it looks like the story. The other three
captures had one message each; the 2026-09-25 advisor captures also had one.

**Payload and preview.** The authoritative payload is `result.structured_output`.
Preview fragments are the `input_json_delta` events on the `StructuredOutput`
block (the first fragment was an empty string in every capture). Byte relations,
checked in all five successful captures by `analyze.sh`: the `result.result` string
is byte-equal to the compact serialization of `structured_output`; the concatenated
`partial_json` is **semantically equal but not byte-equal** (it contains the model's
spacing, e.g. `{"narrative": "…`, 3–11 bytes longer). The CLI therefore re-serializes
the payload compactly: an extracted `structured_output` span is "the CLI's
serialization of the payload", not the model's original bytes, and previews may
differ from it textually.

**Usage and cost.** `result.usage` has `input_tokens`, `cache_creation_input_tokens`,
`cache_read_input_tokens`, `output_tokens` (all present in every capture; cache reads
were nonzero in three of five). Input total = the sum of the first three; cached =
`cache_read_input_tokens`. Top-level `usage` covers the selected model only: in the
2026-09-25 advisor capture the advisor model's tokens appear only in
`modelUsage["claude-opus-5-5"]` and `usage.iterations[type=advisor_message]`.
`total_cost_usd` carries `modelUsage.*.costBasis: "list"` and, in the advisor
capture, includes the advisor model. It is a list-price estimate, not subscription
spending.

**Advisor suppression held.** Across the four bundled requests plus two canary
probes (six calls) there were zero `server_tool_use` / `advisor_tool_result`
blocks. The word "advisor" in stdout comes only from slash-command listings
(`init`, `commands_changed`).

**Isolation evidence (partial).** With a planted `CLAUDE.md` and `AGENTS.md`
canary in the cwd, the `--safe-mode` call produced no canary text; the same call
**without** `--safe-mode` (diagnostic positive control, not the production
invocation) leaked it. `init` still lists built-in plugins/skills/agents and an
empty `mcp_servers`; the canary result is the only evidence that user-level
instructions are not applied. Account-level (org policy) behavior is not isolated
by any flag, as recorded for the advisor.

**Failure shapes re-confirmed.** The unadapted schema (root `$schema` present) exits
1 in ~0.5 s with empty stdout and `Error: --json-schema is not a valid JSON Schema:
no schema with key or ref "https://json-schema.org/draft/2020-12/schema"` on stderr
(no model call). The 2026-09-25 bad-model capture remains the only evidence for an
`is_error: true` result with `subtype: "success"` (a lone `result` record, exit 1).

**Official documentation check (2026-10-02).** Read
[headless mode](https://code.claude.com/docs/en/headless) and the
[CLI reference](https://code.claude.com/docs/en/cli-reference). They document, and
this profile's observations agree with: `-p` with stdin; `--json-schema` (invalid
schema → `Error: --json-schema is not a valid JSON Schema` before the run);
`--output-format stream-json --verbose --include-partial-messages`;
`--permission-prompts none` (v2.1.259+); `--no-session-persistence`;
`--tools ""`; `--safe-mode` (authentication, model selection and permissions keep
working, unlike `--bare`; managed-settings policy still applies); `--bare` skipping
OAuth/keychain and requiring `ANTHROPIC_API_KEY`; SIGTERM → exit 143 with no result
record for the unfinished turn; `claude auth status` exiting 0 when logged in and 1
otherwise, with `authMethod` one of `none`, `claude.ai`, `oauth_token`, `api_key`,
`api_key_helper`, `third_party`; piped stdin capped at 10 MB. Documentation-only
facts, **not observed here**: `system/api_retry` events before a retry; `hook_*`/
`plugin_install` records before `init`; `--fallback-model` can switch models when the
primary is overloaded (so `init.model` is the starting model, and `modelUsage` keys
are the better record of models actually used); `--restricted` (v2.1.248+) as a
stronger harness-isolation flag; `--advisor <model>` makes the advisor tool opt-in
per session, which does not explain the org-level availability seen on 2026-09-25.
The two `*-system-prompt-file` flags are documented and recognized by 2.1.286
(`evidence/flag-recognition.txt`; they fail with "file not found" for a missing path,
where an unknown flag fails with "unknown option"), but were not run with a model.

**Live adapter acceptance (2026-10-02, `ClaudeCliBackend` on 2.1.286).** The real
adapter, `GenerationEngine` and `StoryUseCases` ran the bundled outline, cast, opening
and continuation requests plus one controlled cancellation against the authenticated
`claude`, with the shipped invocation (no `--model`). Evidence and independent recount:
[`reviews/2026-10-02-claude-step8/`](../reviews/2026-10-02-claude-step8/README.md).
Observed: all four accepted (24–33 s each, empty stderr, one `result` last, payload
byte-equal to the compact `structured_output`, previews equal to it as JSON, 338–759
preview fragments per call); the opening took the enforce-retry route live (message 1
`thinking`+`text`, message 2 `StructuredOutput`; first preview only after message 2
began, 19.5 s in); zero advisor content blocks in all five calls; usage summed across
both messages of the two-message call (input 27807, cached 13065); observed provenance
`claude-sonnet-5-5` / `firstParty` with a list-price estimate only; the live cast was
exactly two playable characters named exactly `Ajax` plus seven distinct NPCs; the
continuation returned `upcoming_events: null`. The controlled cancel (1 s after the
child was seen, before any preview) returned in about 15 ms, the direct child PID and
workspace were absent afterwards, and the whole game state was unchanged. Caveats: one
run, one account/org; direct-child cleanup only; the harness's first advisor counter
over-reported (substring match on slash-command listings; structural count is 0).

**Not established by this profile.** Exit codes on signal or budget ceiling; the shape
of rate-limit/overload failures; behavior of 2.1.286 under a bad `--model` (not
re-run); `--model` itself (never passed); streams with a rejected-then-retried
*payload* (a second `StructuredOutput` block) — the only two-message stream seen is
the enforce retry above; concurrent calls; whether org policy elsewhere changes the
advisor behavior; long-narrative streaming at scale.

## Confirmed: shipped live headless gate, 2.1.286, 2026-10-03

`target/debug/cyoa play --headless --backend claude` in a real PTY, bundled templates,
subscription auth (`claude.ai` / `firstParty` / team), no `--model`. Evidence:
[`reviews/2026-10-03-claude-headless/`](../reviews/2026-10-03-claude-headless/README.md).
Observed: outline, cast of four playables, twelve accepted turns (opening, `/action N`,
player prose, empty-line continuation, explicit retry); chapter 0 retitled once
("The Silent Tower" → "The Empty Yoke", seen by turn 3), a
chapter break opened chapter 1 on turn 11 after the scenario was extended; a same-person
rename kept its ID through the break. A real `/action` was cancelled about 1 s after its
child appeared; canonical inspection was byte-equal before and after, and `/retry`
committed the turn. **Incremental previews arrived on every accepted turn** (first
fragment mid-sentence). On two turns the committed final narrative differed in wording
from the streamed preview and the presentation replaced it with the authoritative final;
the wire stream was not retained, so whether this was a second `StructuredOutput`
payload in one result remains unobserved. All sixteen direct children observed (one at
startup, consistent with the auth check; outline, cast, twelve turns, the cancelled
request) and their workspaces were
absent after exit; exit code 0. The model never sent `consolidated_major_events`, so the
faithful 30-event cap truncated the earliest major events from turn 9 on. One run, one
account/org; direct-child cleanup only.

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
| `--system-prompt <text>` / `--append-system-prompt <text>` | Inline string. **Correction (2026-10-02):** `--system-prompt-file`/`--append-system-prompt-file` are absent from `claude --help` but are documented in the official CLI reference, and 2.1.286 recognizes them (see the profile section); the original "do not exist" conclusion from `--help` alone was wrong. They have never been exercised with a model call. |
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

*Update 2026-10-02:* the invocation builder now exists
(`backends::claude_cli::PreparedClaudeRequest`, constant
`backend_compat::claude_cli::ADVISOR_SUPPRESSION`), and the suppression held in six
further live calls (see the profile section above). Both facts are offline/probe
evidence; the adapter has not yet been run end to end.

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
  killed child's exit** — not observed live against Claude. The supervisor has
  implemented group-kill since Phase 1 item 3, and it is covered offline by
  `a_descendant_retaining_the_inherited_pipe_does_not_hang_the_supervisor`; live
  runs confirmed only direct-child cleanup.
