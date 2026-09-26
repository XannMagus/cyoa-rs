# Codex protocol profile discovery — 2026-09-26

Step 1 of `docs/plans/phase1-codex-adapter.md`, against clean HEAD `4ac538d`.
`git log`/`git diff 4ac538d..HEAD` found no subsequent commits or changes.
**Evidence only. No Codex Backend, production compatibility adapter, codec or
headless game is implemented here.** Claude remains a co-equal backend with its
own reference and later acceptance gate; no Claude live claim is changed.

## Method and provenance

Read PLAN, project contracts/required-test registry, Phase 0 acceptance, both
backend references, Phase 1 plans and supervisor repair evidence first. The installed
CLI is now **0.157.1**, not planning's 0.155.1. Recorded `version`, `help`, and
`auth` probes use the same explicit environment/home selection as generation:
only HOME, PATH and CODEX_HOME when present. Auth reports **Logged in using ChatGPT**.
No credentials, environment values, account IDs or auth files were copied. No
model was explicitly requested; the JSONL does not report one. Actual model is
**unknown**, not inferred from a CLI default or this agent's model.

`dump_requests.rs` renders real bundled `GenerationTemplates` via the existing
scripted use cases, using `phase0_story.json`. It records exact instructions,
prompt, original schema and a labelled discovery schema separately. The
independent [expected input state](expected-input-state.json) specifies the mason
protagonist, separate merchant/guard Ajax IDs, event cap 2 and the fixture opening
used by continuation. **These are independent requests from known fixture states,
not a live outline → live cast → live opening chain.** The live world describes
bronze ships called Ajax; that response is not the input to the cast/turn probes.
Model fiction is inspected as generated output, never used to invent expected state.

`probe.rs` is an opt-in recorder around the **existing** vendor-neutral supervisor.
It has no Backend implementation, JSONL acceptance machine, retries or game logic.
Each request gets a private RequestWorkspace, a 180-second deadline, 1 MiB stdout
and 256 KiB stderr caps; the supervisor handles process-group cleanup/reaping.
`invocation.json` records exact argv, executable, cwd, environment **names**, bounds
and requested-model absence. `stdin.txt` records exact sent bytes, and `schema.json`
is the exact supplied schema. `stdout.jsonl` and `stderr.txt` are raw, unredacted
synthetic-story probe captures, including failed attempts. `result.json` records
exit, elapsed time, capture completeness, workspace removal and per-record receipt
times. Timestamps measure receipt of complete lines, not token-generation timing.
Thread IDs are vendor thread identifiers, not authentication secrets.

The first `world-sandbox` attempt failed before generation because the outer
filesystem sandbox prevented app-server initialization. It has empty stdout,
exit 1 and the actual stderr. The explicit rerun and subsequent generation calls
used user-approved sandbox escalation. This does not change the child invocation's
`--sandbox read-only`. No login mode was changed and no API-key variable was passed.
Some independent probes overlapped; no production concurrency behavior is claimed.

Official documentation consulted on 2026-09-26:
[non-interactive mode](https://learn.chatgpt.com/docs/non-interactive-mode)
documents stdin, JSONL, final schema output and saved authentication;
[AGENTS.md discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
documents the instruction byte limit. These support discovery choices, not claims
that all tools/global configuration are disabled or every transcript is supported.

## Observed runs

Every completed generation below had complete stdout/stderr captures and its
request directory removed. Exact durations/usage remain in the corresponding
artifacts; successful generation was about 26–55 seconds.

| Evidence directory | Request/schema | Observed result |
|---|---|---|
| `world-sandbox` | Original world | Local initialization failure, exit 1, no events |
| `world` | Original world | Exit 0; one agent message then turn.completed |
| `cast` | Original current cast | Exit 0; current schema already requires every property |
| `opening-original` | Original StoryTurn | Exit 1; invalid_json_schema, missing required character_updates |
| `opening` | All properties required only | Exit 1; `$ref` with sibling description rejected at QuickAction.kind |
| `opening-ref-union` | Required + single-branch anyOf wrapping annotated refs | Exit 0; 659 whitespace-separated narrative words, one complete message |
| `continuation-hardened` | Same adapted schema; instruction byte limit 0 | Exit 0; 629 words, chapter_title null; one complete message |
| `zero-npcs-hardened` | Bundled zero-NPC request, original schema | Exit 0; npcs is [] |
| `canary` | World plus local AGENTS.md | Exit 0; title is CYOA_LOCAL_INSTRUCTION_CANARY |
| `canary-hardened` | Same canary plus instruction byte limit 0 | Exit 0; title is The Harbour of Two Ajaxes |
| `tool-hardened` | World plus explicit harmless read request | Commentary agent message → command start/completion → another agent message → turn.completed |
| `null-hardened` | Bundled continuation plus labelled discovery directive | Exit 0; chapter_title null and upcoming_events null |
| `empty-hardened` | Bundled continuation plus labelled discovery directive | Exit 0; chapter_title null and upcoming_events [] |

`prepare_variants.sh` reproduces the ref-union schema and the explicitly labelled
null/empty discovery prompt suffixes. The original bundled inputs are preserved.
These extra suffixes are not product prompt overrides or production instructions.

## Findings and frozen implementation requirements

The normative, deliberately narrow profile is in
[reference/02-codex-cli.md](../../reference/02-codex-cli.md#supported-profile-01571-step-1-policy-not-yet-implemented).
It accepts one unambiguous completed agent message only after terminal completion
and successful transport reconciliation. The tool canary has **two agent_message
items with no phase/final discriminator**. Parsing only the JSON-looking message
would be a guess about authority. Preserve this transcript as a **live example
outside the supported profile**, not a successful adapter case. Neither the first
nor last parseable JSON policy is authorized. No tool data enters story payloads.

All seven inspected story success transcripts have exactly thread.started,
turn.started, item.completed(agent_message), turn.completed. No genuine partial
structured text appeared, including the two longer passages. Complete-only delivery
is the supported choice; this does not prove no Codex version/mode can stream.

Only two schema changes are supported by this discovery:

1. Require all properties of every object (including referenced definitions).
   Original current outline/cast/zero-NPC schemas already satisfy this.
2. Move an annotated `$ref` into a single-element `anyOf`, leaving its description
   on the containing schema. QuickAction.kind was the only such location in the
   current turn schema. Keep `$defs`, `$schema`, property order, descriptions,
   defaults, types, existing nullable unions and additionalProperties untouched.

The second change is a response to the captured rejection, not speculative cleanup.
A production walker must traverse schema-bearing nodes only and explicitly fail
unsupported shapes (step 2). No shared/Claude files changed here. Null and [] remain
different; defaulted strings remain strings, never broadened to nullable values.

Isolation is **partial**. The canary proves the old flags do not suppress local
AGENTS.md; byte limit 0 suppressed this observed canary. `--ignore-rules` refers to
execpolicy rules, not AGENTS.md. The tool canary proves shell tools remain available
and can run a read-only command even with the byte limit. Rejecting tool events in
the future codec prevents story acceptance; it does **not** retroactively prevent
a tool side effect. Full global/account skills/plugins/MCP/tool disablement remains
unverified. Do not claim that a clean cwd, explicit environment or no tool event
establishes that stronger guarantee. The normal envelope asks for direct generation
without tools, but this is a model instruction, not enforcement.

Every successful story transcript reports input_tokens, cached_input_tokens,
cache_write_input_tokens, output_tokens and reasoning_output_tokens. The tool probe
reports 14208 cached out of 28994 total input tokens, demonstrating a nonzero cached
count. Other counts include actual zeros; no monetary or observed-model field was
present. Errors in this session report top-level error followed by turn.failed and
exit 1, without usage. Recovery, rate limits and cancellation were not induced.

## Offline inspection, synthetic fixtures and remaining work

`inspect.rs` extracts the sole observed agent-message string **without trimming or
reserializing**, writes `payload.json`, and sends it through the existing wire and
domain constructors. All seven payloads pass. It explicitly asserts the observed
four-event sequence; it is an evidence checker, **not the future codec**.
[boundary-inspection.json](boundary-inspection.json) records results and usage.
`check_evidence.sh` regenerates and byte-compares requests, checks selected hand-built
state expectations, canary outcomes and synthetic reproducibility. These checks do
not establish live application orchestration, identities across generated turns,
preview behavior, cancellation, or adapter acceptance.

`synthetic/expectations.json` has hand-authored expected outcomes for 21 cases,
including terminal/order failures, multiple/ineligible messages, unknown events,
reconnect notice, missing/zero/invalid-cache usage, CRLF, exact whitespace/Unicode,
no trailing newline, malformed UTF-8 and truncation. `make_fixtures.sh` gives exact
provenance; **none is labelled live**. These are inputs for step 3's future tests,
not implemented regressions. The current required-test registry is unchanged and
BACKENDS-001 stays pending. The real tool-canary transcript separately supplies the
observed commentary/tool/multiple-message case.

Affected decisions: BACKENDS-001, ARCH-003, STREAM-001, TEXT-001, PROMPTS-002;
ARCH-001/002, TOOLING-001 and PROMPTS-003 boundaries preserved. No business behavior
or regression expectation changes. Full account isolation, recovered-error streams,
explicit model overrides, successful-adapter cancellation, all-platform cleanup,
and composed live story acceptance remain later work. Claude's own evidence and
acceptance obligations remain independent.

## Reproduction and verification

Offline only (Bash, jq, Rust; no Python or credentials):

```sh
bash reviews/2026-09-26-codex-profile/check_evidence.sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
bash scripts/check_contracts.sh
```

The small standalone tools build outside the workspace with the retained lockfile.
They are not shipped or added to the offline contract gate. The inspector rewrites
only deterministic derived inspection artifacts; original captures are never
rewritten. Build errors during tool setup (wrong Rust method/field names) were
corrected before probes; no error-first implementation/TDD claim is made here.

For an **explicit opt-in live rerun**, use a new evidence label (the recorder refuses
to overwrite one), the absolute installed Codex path, and the recorded request kind
and mode. Check auth with that same executable/environment first. Example:

```sh
/tmp/cyoa-profile-target/debug/probe /absolute/path/to/codex \
  /absolute/path/to/reviews/2026-09-26-codex-profile \
  new-auth none auth
/tmp/cyoa-profile-target/debug/probe /absolute/path/to/codex \
  /absolute/path/to/reviews/2026-09-26-codex-profile \
  new-continuation-hardened continuation_ref_union adapted
```

Labels containing `hardened` add only `-c project_doc_max_bytes=0`; the word is a
probe label, **not a full-isolation guarantee**. `original` uses the original schema;
`adapted` uses the labelled discovery copy. `canary` writes the retained local
instruction file; `tool` writes the harmless tool file and changes only the recorded
transport preface. `version`, `help`, `auth` do not call a model. Do not rerun the
whole probe table automatically or treat a rerun as deterministic model output.

Validation results are retained in `verification.txt`. The existing gate, including
its eleven real behavioral mutations, remains the completion requirement.
