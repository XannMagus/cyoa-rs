# Claude protocol profile discovery — 2026-10-02

Step 1 of `docs/plans/phase1-claude-adapter.md`, against clean HEAD `f001e0b`
(the plan commit) on branch `phase1-claude-adapter`. **Evidence only. No Claude
Backend, codec, preparation code or headless game is implemented here.** Codex
status is unchanged. Affected decisions: BACKENDS-001 and STREAM-001 (evidence
for their later Claude parts), TEXT-001 (scoped raw-span claim), ARCH-003 (no
change). No production code, registry entry or mutation changed.

## Method and provenance

Installed CLI: **2.1.286** (the 2026-09-25 evidence is 2.1.282; `claude --help`
differs only by `--client-data-url` and `--desktop`). `claude auth status --json`
with only HOME and PATH reports `loggedIn: true`, `authMethod: claude.ai`,
`apiProvider: firstParty`, `subscriptionType: team`; only those four fields are
kept in `evidence/auth-status.json` (the raw output also names an email, org and
directories, which are not copied). No `ANTHROPIC_*` variable reached any child.

Every live call uses the **production invocation planned for the adapter**
(`run_probe.sh`): an empty temporary cwd, `env -i HOME PATH` (so this session's
own `CLAUDE_CODE_*` variables are not inherited), **no `--model`**, the
advisor-suppression `--append-system-prompt`, compact adapted schema in
`--json-schema`, prompt on stdin byte-exact, `timeout 180s`. The four bundled
requests were re-rendered with `dump_requests.rs` (unchanged since 2026-09-25;
byte-identical, so `requests/` is a copy of that bundle). These are four
**independent requests from fixture states**, not a live outline → cast → opening
chain; model fiction is output, never expected state.

The nested calls worked in this session's sandbox on the first attempt, so there
is no sandbox-attempt directory. No call was retried. Calls: `p1-world`,
`p2-cast`, `p3-opening`, `p4-continuation`, `p5-canary-world` (planted
`CLAUDE.md`/`AGENTS.md` canary in the cwd), `p5-canary-world-without-safe-mode`
(diagnostic positive control; **not** the production invocation, tools still
disabled) and `p6-unadapted-schema` (fails before any model call). One batch
launch failed on a shell word-splitting mistake of mine before any `claude`
process started (`$2: unbound variable`) and was rerun; it left no artifacts.

Official documentation read the same day: the
[headless page](https://code.claude.com/docs/en/headless) and the
[CLI reference](https://code.claude.com/docs/en/cli-reference); findings are in
`reference/01-claude-cli.md`'s profile section. They reveal that the
`--system-prompt-file` flags *do* exist; the earlier claim that they do not was
drawn from `--help` alone and is corrected in the reference.

## Results (all four bundled requests, exit 0, empty stderr)

| Call | Elapsed ms | stdout bytes | Messages | Payload block | Input total / cached / output |
|---|---:|---:|:-:|:-:|---|
| world | 24327 | 250452 | 1 | M1:0 | 2473 / 0 / 2189 |
| cast | 38677 | 297549 | 1 | M1:1 | 3182 / 0 / 3942 |
| opening | 29277 | 223829 | **2** | **M2:0** | 14625 / 6647 / 3334 |
| continuation | 18102 | 139965 | 1 | M1:0 | 6583 / 5084 / 1687 |

(Input total = `input_tokens` + `cache_creation_input_tokens` + `cache_read_input_tokens`.)
Zero advisor blocks in all six model calls. Default model reported by `init`:
`claude-sonnet-5-5`; `costBasis: list` throughout. The canary stayed out of the
safe-mode outputs and leaked (4 mentions) without `--safe-mode`.

## The findings that change the codec design

1. **A second assistant message is real.** In `p3-opening` the model first wrote
   the opening as plain prose in a `text` block (message 1, `end_turn`); the CLI
   appended `[structured-output-enforce] You MUST call the StructuredOutput tool…`
   and message 2 called `StructuredOutput` at block **index 0**, the same index
   message 1 used for `thinking`. A parser correlating by bare block index would
   have collided; the plan's (message ordinal, index) key is justified by live
   evidence. The prose in message 1 is not the payload.
2. **Payload bytes.** `result.result` is byte-equal to the compact
   `structured_output`; the concatenated `partial_json` is semantically equal but
   3–11 bytes different (extra spaces). An extracted span is the CLI's
   serialization, not the model's original bytes. Previews may differ textually
   from the final payload even on success; the first `partial_json` fragment is
   always the empty string.
3. **New record kinds.** `system/commands_changed` (once before `init`), a second
   `system/status`, a second `rate_limit_event`, and a `user` record with `text`
   content (the enforce prompt) all occur; `result` is always last and unique.
4. **Usage** covers the selected model only; advisor tokens (when present) are in
   `modelUsage`/`iterations`. The 2026-09-25 advisor capture is retained as a
   shape fixture.

## Artifacts

- `evidence/` — per-call `stdout.jsonl`, `stderr.txt`, `stdin.txt`, exact `argv-*`
  values, `meta.json`; `analysis.txt` (output of `analyze.sh`); version, help,
  redacted auth status, `flag-recognition.txt`.
- `requests/` — the bundled request bundle (instructions, prompt, schemas).
- `synthetic/` — 29 **synthetic** stream variants generated by `make_synthetic.sh`
  from literal records whose shapes were copied from the live captures, with
  `expectations.json` as the hand-written specification for the step-4 codec
  tests. They are not live output and the tests they will drive are not yet
  registered. `check_synthetic.sh` cross-checks every referenced record number.
- `expected/` — payload and preview files derived with jq (never from the Rust
  codec) for this review's captures and two 2026-09-25 ones, plus
  `live-expectations.json` (usage typed by hand from `analysis.txt`).
- `check_evidence.sh` — offline consistency checks over all of the above.

## Reproduction

```sh
./run_probe.sh world p1-world         # likewise cast / opening_turn / continuation_turn
PROBE_CANARY=1 ./run_probe.sh world p5-canary-world
PROBE_CANARY=1 PROBE_OMIT_SAFE_MODE=1 ./run_probe.sh world p5-canary-world-without-safe-mode
./analyze.sh > evidence/analysis.txt
./make_synthetic.sh && ./make_expected.sh && ./check_evidence.sh
```

Live calls spend subscription usage. `dump_requests.rs` is run from a scratch
Cargo package with path dependencies on the three workspace crates, as described in
the 2026-09-25 review.

## Verification and limits

`check_evidence.sh` passes; the shared contract gate result is in `contracts.log`.
This is evidence, not a red/green implementation cycle. Not established: exit codes
on signal/budget, rate-limit and overload shapes, `--model` and fallback-model
behavior, a second `StructuredOutput` block (only the enforce retry was seen),
concurrency, long-narrative scale, the file-based prompt flags with a real call,
and anything about other accounts or organizations.
