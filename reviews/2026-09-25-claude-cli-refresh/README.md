# Review: `claude -p` evidence refresh against the real bundled requests

Run on 2026-09-25 by Claude Sonnet 5, from a clean `main` after `b68fc3f`.
This is Phase 1 item 1 (Claude side) of
[`docs/plans/phase1-headless-backends.md`](../../docs/plans/phase1-headless-backends.md):
evidence-only, no adapter code written. `claude` 2.1.282, subscription auth
(`authMethod: claude.ai`, `apiProvider: firstParty`, `subscriptionType: team`),
no `ANTHROPIC_*` variables set. Full `--help` text is
[`claude-help-2.1.282.txt`](claude-help-2.1.282.txt); redacted auth status is
[`auth-status.json`](auth-status.json).

## Reproduction

```sh
# 1. Dump the real bundled requests (no model calls). In a scratch Cargo
#    package with path dependencies on cyoa-core, cyoa-application and
#    cyoa-infrastructure, plus serde_json = "1", edition 2024:
cp dump_requests.rs <scratch>/src/main.rs
cd <scratch> && cargo run --offline -- <out-dir> <path-to-cyoa-rs-repo>
# writes <out-dir>/{world,cast,opening_turn,continuation_turn}/{instructions.txt,
# prompt.txt,schema.json,schema.claude-adapted.json} — copy these over requests/
# to refresh them.

# 2. Live probes (each spends real model usage on the authenticated account):
./run_probes.sh world      p1-world
./run_probes.sh cast       p2-cast
./run_probes.sh opening_turn p3-opening
./run_probes.sh continuation_turn p4-continuation
./discriminator.sh          # advisor-tool env-stripping test, see below
./settings-override.sh      # advisor-tool --settings test, see below
./p8_kill.sh                 # SIGTERM-mid-stream test, see below
```

P6 (unadapted schema) and P7 (invalid model) are one-off invocations, shown
inline below rather than scripted, since they need no live-request scaffolding.

## `dump_requests.rs`: the real bundled requests (no network)

Replays the frozen `cyoa-infrastructure/tests/fixtures/phase0_story.json`
through `ScriptedBackend` and the real `GenerationTemplates`/`GenerationEngine`
— the exact same rendering path `phase_zero_acceptance.rs` exercises, capturing
the four `CapturedRequest`s instead of asserting on them. `requests/` holds the
output: `world/`, `cast/`, `opening_turn/`, `continuation_turn/`, each with
`instructions.txt`, `prompt.txt`, `schema.json` (generic) and
`schema.claude-adapted.json` (after `backend_compat::claude_cli::adapt_schema`).

Sizes (instructions + prompt + adapted schema, the three argv strings a live
call sends): world 1.2KB, cast 3.4KB, opening/continuation turn ~14.4KB each.
The per-argument limit on Linux is `MAX_ARG_STRLEN`, 128KiB — not the 2MB
`ARG_MAX` total-argv figure. At ~14KB for the largest single argument (the
turn instructions or schema), there is roughly 9x headroom before that limit;
worth re-checking if `Limits` configuration ever multiplies schema size
substantially (e.g. very high NPC caps).

## Confirmed findings

### The advisor server-tool call (the big one)

Every live probe below — world, cast, opening turn, continuation turn, and the
SIGTERM probe — included an unrequested `server_tool_use` block named
`advisor`, followed by an `advisor_tool_result` block, before the model
proceeded to the real `StructuredOutput` call. `usage.iterations` on the
`message_delta` event shows this is a genuine second model call, to
`claude-opus-5-5`, that the `claude -p` process makes internally. Observed
added latency: `p1-world` took 60.9s total; a plain single-call structured
request without advisor (see `01-claude-cli.md`'s original 2026-09-24 test #1)
took a few seconds.

**`--safe-mode --tools ""` does not suppress it.** The init event's `tools`
array lists only `["StructuredOutput"]` — `advisor` was never a client-side
tool `--tools ""` could have blocked. It is server-injected, independent of
the CLI's local tool permission system.

**Root cause, per `claude doctor`:** `Organization policy: Loaded from
api.anthropic.com`. This is an org-level policy enforced by Anthropic's API,
not a local CLI/settings feature. Three attempts to suppress it locally all
failed, each kept as its own reproducible script:

| Attempt | Script | Result |
|---|---|---|
| Strip `CLAUDE_CODE_*`/`CLAUDECODE`/all inherited env, keep only `HOME`/`PATH`/`USER`/`LANG`/`TERM` | `discriminator.sh` | Advisor still fires (`evidence/p1-world-clean-env.summary.json`) — rules out "nested inside another Claude Code session" as the cause |
| `--settings '{"advisorModel": null}'` | `settings-override.sh` | Advisor still fires (`evidence/p1-world-settings-override.summary.json`) |
| `CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL=0` with `--model sonnet` | (inline, see below) | Advisor still fires (`evidence/p1-world-envflag0.summary.json`) — this env var only gates advisor for a model *not* in the catalog (see P7 below), it is not a general disable switch |

**Implication for the Phase 1 plan:** this is a permanent characteristic of
`claude -p` structured-output calls on this account, not something the adapter
can configure away. Item 3's timeout defaults must budget for it (~60s+
baseline before the real generation even starts), and item 9's live-acceptance
cost accounting must include it. It does not change the codec/adapter
architecture — `advisor`/`advisor_tool_result` blocks are simply more
"unrelated tool blocks" the content-block-identity handling (see below) must
already ignore.

### Content-block identity (P3, opening turn)

Block order for the opening turn, by `content_block_start.index`:

| index | type | name |
|---|---|---|
| 0 | `thinking` | — |
| 1 | `text` | — |
| 2 | `server_tool_use` | `advisor` |
| 3 | `advisor_tool_result` | — |
| 4 | `thinking` | — |
| 5 | `tool_use` | `StructuredOutput` |

The index-1 `text` block's content is plan-relevant prose about the story
task itself (paraphrased: *"The task asks me to write the opening scene...
Let me consult the advisor before committing to my approach"*) — a real,
observed case of exactly the failure the Phase 1 plan already warns about:
"ordinary intermediate messages must not be concatenated into a pretend final
structured response." Only `index: 5`'s `input_json_delta` events are the
real payload stream.

**Verified:** the concatenated `partial_json` for index 5 is byte-identical
(after key-order-insensitive JSON comparison) to the final `result.
structured_output` object. Confirms TEXT-001's raw-span extraction premise
holds for `claude -p`.

### Required fields after R5 (P2, cast)

All 8 generated NPCs had non-empty `relationships` populated — consistent
with `wire.rs`'s post-R5 required (non-`#[serde(default)]`) field. No
omission observed in this run; still only n=1.

### Continuation turn shape (P4)

`starts_new_chapter: false`, `chapter_title: null`, non-null `upcoming_events`
array with 2 entries — nominal continuation, no chapter-break or
null-vs-empty-thread edge case observed in this run.

### Unadapted (root `$schema`) rejection — P6, no model call

```sh
claude -p --safe-mode --tools "" --permission-prompts none --no-session-persistence \
  --model sonnet --output-format json \
  --system-prompt "$(cat requests/world/instructions.txt)" \
  --json-schema "$(cat requests/world/schema.json)" \
  <<< "$(cat requests/world/prompt.txt)"
```

Exit 1, empty stdout, all output on stderr:
`Error: --json-schema is not a valid JSON Schema: no schema with key or ref
"https://json-schema.org/draft/2020-12/schema"`. No result envelope, no cost.
Confirms the 2026-09-24 finding still holds on 2.1.282, and that failure here
is fast/cheap (no advisor call — never reaches model invocation).

### Invalid `--model` — P7

```sh
claude -p ... --model not-a-real-model-xyz --output-format json ...
```

Exit 1, `stdout`: a `result` envelope with `is_error: true`,
`api_error_status: 404`, `terminal_reason: "api_error"`,
`subtype: "success"` (misleading field name — check `is_error`, not
`subtype`, as the original reference doc already advised). `stderr` carries
the actually informative line:

```
Warning: Advisor disabled — base model 'not-a-real-model-xyz' has no advisor
rank in the model catalog. Switch to a public model alias (opus, sonnet,
fable) or set CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL=1.
```

This is what led to testing the env var above (envflag0) — disproved as a
general disable, since it only gates advisor for models the catalog doesn't
recognize (which also fail the call entirely), not for `sonnet`/`opus`.

### SIGTERM mid-stream — P8

`p8_kill.sh`: launched the opening-turn request under `setsid` (PGID == the
`claude` process's own PID after `bash -c` execs it), waited for the first
`input_json_delta`, sent `SIGTERM` to the `claude` PID alone (not the group).

**Result: inconclusive on timing, but instructive.** The process was killed
mid-way through the `advisor` server-tool-use block (see
`evidence/p8-sigterm.jsonl`, 54 lines, no terminal `result` line ever
written). No stray processes remained afterward (checked separately after the
fact), but the script's own `wait` on the process did not return within its
~196s remaining budget before an outer `timeout 200s` killed the whole
harness — so a single-PID `SIGTERM` was *not* observed to produce prompt
(sub-few-second) termination in this run. This is one observation, not a
confirmed hang duration, but it's directly consistent with the Phase 1 plan's
existing requirement to use `killpg`-style whole-group signaling plus an
explicit reap/timeout in the supervisor (item 3), rather than relying on
`SIGTERM` to a single leaf PID and hoping the process tree unwinds promptly.

## What's still open (not exercised this session)

- Rate limit / budget-exceeded shapes (would require an actual rate limit or
  `--max-budget-usd`, not induced here — labeled synthetic-only if fixture'd
  in item 4).
- Exit codes 130/143 specifically (P8 used SIGTERM/exit-code inspection but
  didn't cleanly observe a terminal exit code before the outer timeout fired).
- Long-narrative genuine streaming over many small deltas at scale (P3/P4 show
  incremental deltas but on relatively short generated turns).
- `--system-prompt-file` / `--append-system-prompt-file`: **do not exist** in
  2.1.282's flag list (checked directly against `claude --help`, not assumed).
  Only inline `--system-prompt`/`--append-system-prompt` strings exist. This
  removes the "large-prompt alternative" the original Phase 1 plan considered
  testing (originally P5) — there isn't one; argv is the only path, hence the
  `MAX_ARG_STRLEN` headroom check above matters more than previously assumed.
- Whether the advisor tax applies identically to other accounts/orgs without
  this `advisorModel` policy — this session can only speak for the
  authenticated org.

## Redaction note

All committed `evidence/*.jsonl` files had `encrypted_content` values (from
`advisor_tool_result` blocks) replaced with a length placeholder — those
blobs base64-decode to include the org UUID in their protobuf framing, so a
literal-UUID grep alone would have missed it. Scratch paths and the org UUID/
name were scrubbed from all committed files; `auth-status.json` keeps only
`loggedIn`/`authMethod`/`apiProvider`/`subscriptionType`, dropping
email/orgId/orgName/projectsDirectory/configDirectory. Three of the four
advisor-investigation reruns (`clean-env`, `settings-override`, `envflag0`)
are committed as small `.summary.json` extracts (content-block types + error
flag) rather than their full ~250KB transcripts, since their only evidentiary
value is "did advisor still appear" — the full baseline transcript
(`p1-world.jsonl`) is kept in full as the primary codec-evidence artifact.
