# Next slice: a Claude CLI adapter with independently verified completion

Status: **planned, 2026-10-02.** Nothing below is implemented or live-verified yet.
Baseline: `efd0a53`. This expands item 5 of [Phase 1](phase1-headless-backends.md)
and consumes the [Claude adapter handoff](phase1-claude-adapter-handoff.md); it
does not replace that handoff or the [project contracts](../decisions/README.md).
The Codex slice ([plan](phase1-codex-adapter.md)) is the structural template only:
its protocol rules are not Claude's rules.

## Outcome and boundaries

Deliver `ClaudeCliBackend: Backend`, usable by the existing `GenerationEngine` and
`StoryUseCases`, for outline, cast, opening and continuation requests. A success
needs an authoritative `result.structured_output`, `is_error` false, successful
process completion, complete request delivery, no observed cancellation or I/O
failure, and successful cleanup. Domain validation still happens afterward.

Claude and Codex are co-equal. This session happens to have an authenticated
`claude`; that is why Claude goes next, not a preference. The slice ends before
presentation workers, headless play, TUI, persistence, arbitrary prompt overrides
or automatic retry. The opt-in live harness uses the real adapter and use cases and
is not a second engine or a shipped interface. Fixture tests never establish
authenticated behavior.

## Evidence questions to settle first

The [Claude reference](../../reference/01-claude-cli.md) records 2.1.282 probes
(2026-09-25). Installed now: 2.1.286; `--help` differs only by `--client-data-url`
and `--desktop`, neither used.

| Question | Probe / evidence | Consequence |
|---|---|---|
| Does the exact production invocation (empty cwd, env = HOME + PATH only, no `--model`) keep subscription auth and work? | Bounded live calls with that env, `auth status --json` mode fields only | Decides the env allowlist and whether `CLAUDE_CONFIG_DIR` is needed |
| Is more than one assistant message per stream ever seen? | Count `message_start` in every capture, including the advisor-suppressed ones | Decides how block indices are keyed and what a later-message `StructuredOutput` means |
| How does the `result` envelope relate byte-wise to `partial_json` and `structured_output`? | Compare the three for each capture | Bounds the TEXT-001 "raw span" claim: the CLI's serialization, not the model's bytes |
| Which `init` fields are stable evidence? (`apiKeySource`, `model`, `tools`) | Read `system/init` under subscription auth | Observed model and a metered-auth backstop |
| Which usage fields are present and how do they add up? | `usage` and `modelUsage` across captures | Input total/cached mapping through `normalize_input_tokens` |
| Does the advisor-suppression append hold across all four bundled requests? | Presence of `server_tool_use`/`advisor_tool_result` blocks | Confirms or qualifies the 2026-09-25 two-request result |
| Argv limits | Measure each argument | Single-argument `MAX_ARG_STRLEN` guard; no file-input flag exists |

Probes have finite deadlines and use synthetic story content. Record version, auth
mode only, invocation, request/schema artifacts, raw stdout/stderr, exit and elapsed
time; never credentials or environment values. Never change the login mode. If a
nested call fails for what looks like an environmental/sandbox reason, retain it as
a separate attempt and rerun outside the sandbox as one recorded diagnostic action.
No automatic retry anywhere.

## Architecture and type boundaries

```text
StoryUseCases -> StoryGenerator / GenerationEngine
                                  |
                               Backend
                                  |
                         ClaudeCliBackend
                          /            \
                Claude protocol       process::run
                state machine         (vendor blind)
```

All vendor types stay in infrastructure. The adapter owns a private protocol state
machine and a checked invocation configuration; the existing supervisor owns process
mechanics and must not learn block names, `result`, or Claude usage. The adapter
reconciles protocol state with the supervisor outcome.

| Home | Responsibility |
|---|---|
| `backends/executable.rs` | Vendor-neutral executable resolution extracted from `CodexExecutable` (behavior-preserving) |
| `backends/claude_cli.rs` + private `adapter`, `protocol` modules | Config, preparation, codec, reconciliation |
| `generation/backend_compat/claude_cli.rs` | Existing `adapt_schema` (untouched) plus the advisor-suppression constant |
| `bin/subprocess_fixture.rs` | A Claude vendor mode beside the unchanged Codex mode |
| `tests/fixtures`, `reviews/…-claude-step*/` | Frozen live/synthetic transcripts with provenance |
| `tests/claude_backend.rs`, `tests/claude_story_acceptance.rs` | Real adapter + real supervisor + real fixture children |
| `examples/claude_adapter_acceptance.rs` | Opt-in authenticated harness |

Do not edit shared wire/schema/prompt/template code. The advisor instruction is
appended locally to this invocation only. Do not use `--bare`. Keep Codex tests,
names and assertions unchanged through the executable extraction.

Protocol state is a private runtime enum with data in each variant; the correlated
payload block is keyed by (message ordinal, block index) so an index never travels
without its message. A candidate is not a `GenerationResponse`; only reconciliation
constructs success.

## Proposed protocol policy (frozen against evidence in step 1)

- Preview fragments come only from `input_json_delta` on the block whose start is
  `tool_use` named `StructuredOutput`. Text, thinking, advisor and other tool blocks
  are never narrative; unknown content-block types are non-payload.
- Two `StructuredOutput` blocks in one message are ambiguous. A later message's
  block is tolerated because `result` is authoritative, but preview stops for good
  at the first payload block's `content_block_stop`.
- Exactly one `result`, last. Missing, duplicate (even identical) and trailing
  records are distinct errors. `is_error: true` is a failure whatever `subtype` says.
- Payload is `result.structured_output`'s exact span (serde_json raw values), never
  re-serialized; duplicate keys rejected. Preview may differ; the result wins.
- Listed metadata and unknown `system` subtypes are tolerated; an unknown top-level
  record type is a protocol failure, not a generic ignore.
- `init.apiKeySource` other than the subscription value fails closed, as a backstop
  only; the real guards are the auth preflight and the environment allowlist.
- Usage may be unknown; cache counts use `normalize_input_tokens`. Selected model
  and observed model are separate. `total_cost_usd` is a list-price estimate only.
- A result candidate never overrides nonzero exit, incomplete stdin, observed
  cancellation, deadline, output cap, protocol or cleanup failure.

## Atomic implementation sequence

Each item is one commit with implementation, tests, registry updates and evidence
together; its review record lives under `reviews/2026-10-02-claude-stepN/`.

0. This plan.
1. Freeze the supported protocol profile with bounded live evidence (evidence only).
2. Extract vendor-neutral executable resolution (no behavior change).
3. Isolated invocation preparation, argv-size guard and advisor suppression.
4. Private codec, offline, replaying frozen captures.
5. `ClaudeCliBackend`: auth preflight, reconciliation, real-child fixture tests.
6. Composed story acceptance through the real adapter.
7. Persistent behavioral mutations for Claude-specific acceptance mistakes.
8. Live adapter gate, Claude reference update and presentation handoff.

## Per-commit TDD and completion checklist

Error tests first, observed meaningful runtime red, minimum implementation, then
edges, then nominal. Compile/fixture errors are not red. A test that already passes
is recorded as existing coverage; never damage correct code or change expectations
to manufacture history. Mutation evidence is reported separately.

Every commit records affected contracts (BACKENDS-001, ARCH-001/002/003, TEXT-001,
STREAM-001 and the identity/prompt/limit/chapter contracts exercised), evidence
paths and remaining uncertainty, then runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
bash scripts/check_contracts.sh
```

All eighteen existing mutations remain; patches that target files this slice edits
(notably `backend_compat/claude_cli.rs`) must keep applying. Ordinary tests need no
vendor executable, credentials or network. Broad contract statuses stay partial.
