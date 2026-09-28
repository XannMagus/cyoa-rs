# `codex exec` — adapter implemented; composed/live acceptance incomplete

Co-equal companion to `01-claude-cli.md`. Step 1 of the Codex adapter plan is
complete: **0.157.1, 2026-09-26**, authenticated using ChatGPT. Bundled outline,
cast, opening, continuation and zero-NPC requests were probed, plus explicit
null/empty and isolation canaries. [Full evidence and reproduction](../reviews/2026-09-26-codex-profile/README.md).
This freezes a narrow protocol. The subsequent step-4 evidence below exercises
the actual Backend for one outline; complete acceptance remains pending.
Claude's implementation/live gates remain independent.

Implementation status, 2026-09-27: steps 1–4 now include request preparation,
private protocol decoding and real Backend reconciliation. The supported-profile
policy below is implemented; offline replay is distinct from live evidence.

## Confirmed: actual adapter outline smoke, 0.157.1, 2026-09-27

The opt-in `codex_adapter_smoke` example used bundled outline templates and the
actual CodexCliBackend with a 180-second deadline and 1 MiB/256 KiB capture caps.
It confirmed ChatGPT subscription auth through the same selected home/environment,
then issued one generation call. The first sandboxed attempt failed before model
generation with empty stdout and an app-server filesystem initialization error.
An explicitly approved rerun outside the outer sandbox succeeded in 27.161 seconds.
Both attempts are retained in [step-4 evidence](../reviews/2026-09-27-codex-step4/README.md).

The accepted outline passed the existing wire/domain constructors. One complete
payload emission matched the final payload exactly. Input 14419, cached input 0,
output 778; no model override was requested, no model identity or cost was observed.
Success came through supervisor exit/reap/workspace-cleanup checks. Real PID and
workspace assertions are separately exercised by fixture children, not inferred
from this smoke. No real cancellation or composed cast/turn story was exercised.

## Confirmed: 0.157.1 discovery, 2026-09-26

- Exact help/version/auth captures and each argv/stdin/schema/stdout/stderr/exit,
  finite bounds and receipt timings are in the linked evidence directory. Only
  HOME, PATH and CODEX_HOME (when present) were inherited. No metered-auth override
  was passed. No explicit model was selected, and events reported no model name.
- Outline and current cast schemas work unchanged. Original StoryTurn fails with
  `invalid_json_schema` (missing required character_updates). Making all properties
  required exposes another rejection: a `$ref` with sibling description at
  QuickAction.kind. Moving that ref into a single-branch anyOf while retaining
  its description succeeds. Keep root `$schema`, definitions/refs, ordering,
  defaults, descriptions, types, nullability and additionalProperties.
- Seven inspected story successes have thread.started → turn.started → exactly
  one item.completed(agent_message with string text) → turn.completed. Opening
  and continuation narratives contain 659 and 629 whitespace-separated words;
  neither emitted partial text. Complete-only is supported; universal absence of
  streaming is not proven.
- Bundled instructions and user prompt were delivered together on stdin, as
  separately JSON-escaped `instructions` and `prompt` fields with an explicit
  transport preface. The unmodified strings round-trip; this is a combined user
  message, **not a privileged system/developer channel**. All large text stays
  off argv. The exact preface is frozen in each `stdin.txt` and `probe.rs`.
- Zero-NPC output is `[]`. Labelled discovery suffixes exercised chapter_title
  null and upcoming_events null versus []; defaulted durable delta fields were
  returned as empty strings. Existing wire/domain constructors accept the seven
  extracted payloads. They were not played as a composed live story.
- `--ignore-user-config --ignore-rules` does **not** disable AGENTS.md: a local
  canary changed the title. Adding `-c project_doc_max_bytes=0` suppressed that
  canary. A separate harmless read canary proves shell tools remain usable.
  That stream includes an ordinary commentary agent_message, command_execution
  start/completion, then another agent_message. Neither message has a final/phase
  discriminator. Do not pick the first/last parseable JSON to resolve ambiguity.
- Success usage includes input_tokens, cached_input_tokens,
  cache_write_input_tokens, output_tokens and reasoning_output_tokens. Cached
  input is zero in story probes and nonzero (14208 of 28994) in the tool probe.
  No monetary field or model identity appeared. Schema errors emit error then
  turn.failed, exit 1, no usage. The initial outer-sandbox initialization failure
  instead had empty stdout and stderr only; its explicit rerun is retained.

## Supported profile 0.157.1 (step 1 policy, not yet implemented)

This is an intentionally conservative **adapter acceptance policy**, not a claim
that every valid Codex run has this shape. Steps 2–6 must implement and test it.

Invocation: resolved absolute executable, private request cwd, explicit auth-home
environment above, subscription-mode preflight using the same selection, and:

```text
codex exec -c project_doc_max_bytes=0 --json --ephemeral --sandbox read-only
  --ignore-user-config --ignore-rules --skip-git-repo-check --color never
  --output-schema schema.json -
```

Use the recorded combined stdin envelope. Prepare a Codex-owned schema copy using
the two observed transformations above in `generation::backend_compat::codex_cli`.
Do not import Claude's root-key removal, alter shared DTOs/templates/schemas, add
nullability, or expose arbitrary config/argv overrides. Explicit model overrides
are not exercised by this profile; configured is never synonymous with observed.

Protocol rules to implement:

1. One thread.started with nonempty thread_id, then one turn.started, then one
   item.completed whose item has nonempty id, type agent_message and string text,
   then one turn.completed. Empty/invalid structured text cannot become success.
   Preserve the decoded text's exact UTF-8 bytes; validate JSON separately.
2. An agent message is only a candidate. Require successful process completion,
   full request delivery, no observed cancellation/I/O/cap/cleanup failure, and
   successful reaping/workspace cleanup before emitting **one complete payload**.
   No simulated streaming, concatenation or duplicate final emission.
3. Reject duplicate or conflicting terminals, messages before turn start/after
   completion, missing candidate/terminal, multiple agent_message items (including
   commentary and identical candidates), and reused/invalid item IDs. No
   first/last-message or first/last-parseable-JSON heuristic.
4. In this initial profile, ignore only extra fields on known valid envelopes;
   no other event/item kinds are classified harmless. Reject reasoning, tools,
   unknown events and partial item events as unsupported, keeping diagnostics.
   This narrow choice can reject legitimate CLI runs; future tolerance needs its
   own evidence and regression. In particular, tool output never supplies JSON.
5. Any turn.failed rejects the call and invalidates earlier candidate success.
   Any top-level error also makes this initial profile unsupported. Historical
   reconnect notices are **not necessarily terminal vendor failures**, but no
   successful recovery was observed here; conservatively reject rather than
   claim them harmless. Keep candidate and diagnostics separately on failure.
6. Reject malformed required event fields, invalid protocol UTF-8/JSON and
   truncated JSON records. Accept a complete final JSON record without newline,
   and CRLF framing without changing retained transcript or decoded payload bytes.
7. Usage is optional telemetry, never the payload authority. Missing means unknown;
   explicit zero means zero. Map total/cached input through existing
   normalize_input_tokens (cached greater than total becomes unknown). Keep output
   total without adding reasoning or cache counts again. Missing/malformed usage
   does not invent counts or invalidate otherwise valid fiction; malformed
   telemetry is unknown. Ignore documented auxiliary counts at the normalized
   boundary; raw diagnostics retain them. Observed model and monetary cost stay
   absent for these transcripts.

The [synthetic fixture manifest](../reviews/2026-09-26-codex-profile/synthetic/expectations.json)
specifies future acceptance outcomes independently of a codec. These fixtures
are not live observations or existing implementation tests. The real tool-canary
transcript is an explicit unsupported-profile example despite its CLI exit 0.

## Open questions after step 1

- Full account/global instruction, skill/plugin/MCP and tool isolation is **not
  verified**. The byte-limit canary covers local AGENTS.md only. Read-only does
  not disable reads/tools; rejecting their events cannot prevent earlier effects.
- Other CLI versions, explicit model choices, longer/different-mode streaming,
  recovery after error notices and a reliable discriminator among multiple agent
  messages remain outside this profile until independently exercised.
- Rate-limit/unsatisfiable-schema/sandbox-denial outcomes and failure/cancellation
  usage are unexercised. Do not exhaust quota to manufacture evidence.
- Complete CodexCliBackend live acceptance, live cancellation,
  composed identities/limits/story behavior and the headless gate remain pending.
  Candidate retention and transport reconciliation now have fixture evidence.
  Supervisor Linux tests and payload boundary inspection are different evidence.

## Historical observations (0.155.1, retained with original scope)

## Initial minimal verification recipe

```bash
npx @openai/codex@latest login          # interactive, OAuth — do this in a real terminal
codex login status                       # confirm subscription (not API-key) auth
echo 'Write a two-sentence tavern scene.' | codex exec \
  --json --ephemeral --sandbox read-only --skip-git-repo-check \
  --output-schema /tmp/schema.json
```
with `/tmp/schema.json` holding e.g.
`{"type":"object","properties":{"narrative":{"type":"string"},"mood":{"type":"string"}},"required":["narrative","mood"],"additionalProperties":false}`.
Then read the JSONL stream the same way `01-claude-cli.md`'s test #2 did, and update
this file's "Confirmed" section with what actually comes back.

## Confirmed: the command exists and takes these flags

```
codex exec [OPTIONS] [PROMPT]
```

Prompt comes from the positional arg, or from stdin when no arg is given (if both are
given, stdin is appended as a `<stdin>` block) — same shape as `claude -p`.

Relevant flags from `codex exec --help` (0.155.1):

| Flag | What `--help` says |
|---|---|
| `--json` | Print events to stdout as JSONL |
| `--output-schema <FILE>` | Path to a JSON Schema file describing the model's final response shape — **a file path, not inline JSON** like `claude`'s `--json-schema`, so the Rust backend must write a temp schema file per call (or once, if the schema is static per call site) |
| `--ephemeral` | Run without persisting session files to disk |
| `-s, --sandbox <read-only\|workspace-write\|danger-full-access>` | Sandbox policy for model-issued shell commands |
| `--ignore-user-config` | Do not load `$CODEX_HOME/config.toml`; auth still uses `CODEX_HOME` |
| `--ignore-rules` | Do not load user or project execpolicy `.rules` files |
| `--skip-git-repo-check` | Allow running Codex outside a Git repository |
| `-o, --output-last-message <FILE>` | Write the final agent message to a file |
| `-m, --model <MODEL>` | Model to use (e.g. `gpt-5.2-codex`) |
| `--color <auto\|always\|never>` | Analogous to nothing in the Claude backend; keep `never` for clean piping |

No flag equivalent to `claude`'s `--tools ""` (full tool disablement) was found —
`--sandbox read-only` constrains what a shell tool call can *do*, it doesn't prevent
the model from attempting one. Unverified how that surfaces in the JSON event stream.

## Confirmed: auth mirrors the `--bare` trap exactly

```
codex login                              # ChatGPT/subscription OAuth — default, what we want
codex login --with-api-key               # reads OPENAI_API_KEY from stdin — metered billing
codex login --with-access-token          # reads a raw access token from stdin
codex login status                       # check which mode is active
```

`codex login`'s own `--help` confirms `--with-api-key` is the opt-in metered path;
default interactive login is the ChatGPT plan. This is the same shape as Claude's
`--safe-mode` (subscription-preserving) vs. `--bare` (forces `ANTHROPIC_API_KEY`) —
just inverted, in that Codex's trap is a login *mode* rather than an *exec* flag, so
there's nothing on the `codex exec` invocation itself to audit — `cyoa doctor` needs
to shell out to `codex login status` and parse it.

## Confirmed: unauthenticated failure shape

Running `codex exec --json ...` with no valid login produces this JSONL shape
(observed directly, request failed at the transport layer before reaching a model):

```json
{"type":"thread.started","thread_id":"..."}
{"type":"turn.started"}
{"type":"error","message":"Reconnecting... 1/5 (unexpected status 401 Unauthorized: ...)"}
...
{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Falling back from WebSockets to HTTPS transport. ..."}}
{"type":"error","message":"..."}
{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized: ..."}}
```

This at least confirms the event *envelope* (`thread.started` / `turn.started` /
`item.completed` / `turn.failed`) is real JSONL with a `type` discriminant, matching
the general shape `01-claude-cli.md` describes for Claude's stream — but says nothing
about what a successful `item.completed` for an agent message or a structured
response looks like. The later authenticated run below supplies that evidence.

## Confirmed: authenticated cast and schema compatibility, 2026-09-24

Installed `codex --version`: **0.155.1**. `codex login status`: **Logged in using
ChatGPT**. Used `exec --json --ephemeral --sandbox read-only --ignore-user-config
--ignore-rules --skip-git-repo-check --color never --output-schema <file>` in a
temporary directory outside the repository, with a 50-second timeout. Neither run
hit that timeout. Exact commands, schemas, and JSONL transcripts are retained in
[the generation-boundary review](../reviews/2026-09-24-generation-boundary/README.md#live-codex-evidence-and-backend-handoff).

- The project's unadapted generated cast schema failed, exit **1**, with HTTP 400
  `invalid_json_schema`. The error required every property to appear in `required`
  and specifically named missing `relationships`. Events were `thread.started`,
  `turn.started`, `error`, and `turn.failed`.
- A temporary copy making every object's properties required succeeded, exit **0**.
  The root `$schema` and `$defs`/`$ref` remained present. Thus this cast schema works
  with those features; Claude's root-key removal is not needed for this example.
- The success stream contained `thread.started`, `turn.started`, one
  `item.completed` with `item.type = "agent_message"` and a JSON **string** in
  `item.text`, then `turn.completed`. No pre-parsed structured-output object or
  partial text event appeared in this transcript.
- `turn.completed.usage` reported `input_tokens: 13893`, `cached_input_tokens: 0`,
  `cache_write_input_tokens: 0`, `output_tokens: 356`, and
  `reasoning_output_tokens: 31`. There was no monetary-cost field in this stream.
- No tool-call event appeared. This is not proof of tool disablement or complete
  configuration isolation. No explicit model was pinned in these calls.

Per ARCH-003, production schema adaptation belongs in a future
`generation::backend_compat::codex_cli` file. The temporary cast transformation
does not settle optional/null behavior for all DTOs and is not a shipped adapter.
Shared wire requiredness must separately follow the business/source contract.

## Historical open questions (2026-09-24; superseded by the current sections above)

1. **Does `--output-schema` stream the structured fields incrementally?** Claude's
   backend gets this via a forced `StructuredOutput` tool call whose `input_json_delta`
   fragments are exactly the raw JSON text `StreamingStringField` wants. It is
   still unverified whether Codex's `--output-schema` (a) can stream the final message's raw
   JSON text token-by-token as ordinary `item` deltas (which would still work with the
   same scanner, just fed from a different event field), or (b) only validates/attaches
   the parsed object once the turn completes, with no partial text available at all. If
   (b), `narrative`'s incremental reveal doesn't work for this backend in v1 — either
   accept spinner-then-reveal for Codex, or find another signal to stream from.
   The short cast run above emitted only the complete message; exercise a long
   narrative before settling the backend's incremental-progress contract.
2. **Remaining response/schema shapes** — the successful cast message shape is
   confirmed above. Exercise StoryTurn, nested optional/null fields, longer
   responses, and any other item kinds the adapter must handle. Do not infer
   universal protocol behavior from one cast response.
3. **Does `--ignore-user-config` + `--ignore-rules` add up to full isolation**, or can
   configured MCP servers / plugins still activate during `codex exec`? Claude's
   `--safe-mode` explicitly disables MCP/plugins/hooks/skills in one flag; Codex has no
   single documented equivalent in `--help`.
4. **Exit codes and error conventions** on failure modes that matter for this project:
   rate limit, sandbox-denied action, other malformed/unsatisfiable schemas. The
   missing-required schema rejection is confirmed above. Needed to
   reproduce Claude backend's "never auto-retry, surface as `Result::Err`" posture.
5. **Remaining usage/cost behavior** — successful cast token reporting is confirmed
   above. Failure/cancellation usage and monetary reporting, if available, remain
   unverified. Do not treat token counts as a subscription charge.
6. Whether `--model gpt-5.2-codex`-style model names need pinning the way `--model
   sonnet` does for Claude, or whether a config default is sufficient.

Update this file's "Confirmed" section with real output before implementation, the
same way `01-claude-cli.md` was written from an actual transcript rather than
`--help` text alone.
