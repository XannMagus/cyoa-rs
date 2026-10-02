# Claude adapter step 8 — live adapter acceptance and presentation handoff

Baseline `32fce53`. Scope: the Claude adapter plan's last step, before presentation
or headless play. One new opt-in harness (`examples/claude_adapter_acceptance.rs`);
no production, shared DTO, schema or prompt change. Affected decisions:
BACKENDS-001, STREAM-001, TEXT-001, ARCH-003 (statuses stay partial; the registry and
all expectations are unchanged).

## Reproduction and what the harness owns

```sh
cargo run -p cyoa-infrastructure --example claude_adapter_acceptance --locked --offline -- NEW_DIRECTORY
```

`StoryUseCases -> GenerationEngine -> ClaudeCliBackend` with bundled templates; a
recording `Backend` delegates every generation to the real adapter and implements no
codec, process runner or game engine. It refuses to overwrite a directory. Each call
has the production limits (180 s deadline, 4 MiB stdout, 256 KiB stderr). The adapter
checks authentication with the same executable/home as generation; **no model is
requested** (`--model` is never passed), so the account default applies, and no API-key
variable is passed. An observer reads only this process's direct children through
`/proc/self/task/*/children`, identifies the generation child by `--json-schema` in its
argv, and records its PID, argv and cwd; it never signals anything. Call 5 is cancelled
through the ordinary `CancellationSource` one second after the child is first seen;
the production supervisor terminates and reaps. Cleanup evidence is direct-PID and
workspace absence after return, not a claim about descendants or remote server work.

Installed CLI **2.1.286**; `auth status` modes only (`version.txt`,
`auth-status.json`: `claude.ai`, `firstParty`, team). `claude-help.txt` is unchanged
since step 1. No credential file or environment dump was collected, and a scan of
`live-attempt/` finds no email-like string. The nested calls ran inside this agent's
own sandbox without failing, so there is **no sandbox-attempt directory** and no
rerun: each call is a single attempt; there was no automatic retry anywhere.

The brief is the same synthetic harbour brief as the Codex live gate: 3–5 playable
characters with exactly two named `Ajax`.

## Observed live result — passed

| Call | Outcome | ms | Input total / cached / output | Previews (first at) | Messages |
|---|---|---:|---|---|:-:|
| 01 outline | accepted | 24940 | 2580 / 0 / 2082 | 589 (4245 ms) | 1 |
| 02 cast | accepted | 29501 | 5045 / 0 / 2808 | 759 (1671 ms) | 1 |
| 03 opening | accepted | 32923 | 27807 / 13065 / 3849 | 338 (19523 ms) | **2** |
| 04 continuation | accepted | 23836 | 14587 / 7255 / 2439 | 635 (2012 ms) | 1 |
| 05 controlled cancel | cancelled | 1024 | unreported | 0 | 0 |

Top-level `result.json`: `accepted: true`, five generation calls, two committed
turns, cancelled, full game state equal after cancellation. Independent recheck of the
raw artifacts (`analyze_live.sh` → `analysis.txt`, jq only): each success has exactly
one `result` last, empty stderr, `init.apiKeySource: "none"`, `init.tools:
["StructuredOutput"]`, model `claude-sonnet-5-5` (observed from `init`; none was
requested), `payload.json` **byte-equal** to the compact `result.structured_output`,
and previews equal to the payload when parsed as JSON (the CLI's compact payload and the
model-spaced previews differ in bytes, as in step 1). No `--bare` or `--model` in any
argv. Observed child PID and workspace are absent after every call, including the
cancelled one (cancellation-to-return about 15 ms, measured through observer join and
evidence writing, so conservative). The cancelled call had produced no preview, no
result and no usage when stopped.

Observed provenance on accepted calls: provider `firstParty`, model
`claude-sonnet-5-5`, and a **list-price estimate** (about $0.031, $0.048, $0.100,
$0.055); these are not subscription spending.

## Findings

1. **The enforce retry happened live through the whole adapter.** Call 03's stream
   is `M1: thinking, text` then `M2: StructuredOutput` after the CLI's
   `[structured-output-enforce]` user record. The first preview arrived 19.5 s in,
   from message 2's block only, and the committed turn came from the result. This is
   the second live observation of the shape that motivated (message ordinal, block
   index) keying, now in an adapter run rather than a probe.
2. **Zero advisor content blocks in all five calls**, so the local suppression held
   across the whole bundled sequence, not only on isolated probes. Two caveats: this
   is one account/org, and the first run of the harness **reported 2 advisor blocks for
   call 01**. That was a bug in the harness counter (a substring match that also hits
   the slash-command listings in `commands_changed`), not an advisor call; the
   independent structural count is 0. The harness now counts content blocks
   structurally (`advisor_blocks`). The recorded `live-attempt/01/result.json` keeps
   the original number as produced; `analysis.txt` is the authoritative recount.
3. **The live cast was the namesake case.** Exactly two playable characters, both with
   the name exactly `Ajax` and different descriptions and histories, plus seven NPCs
   with no duplicate names. (The brief asked for 3–5 playable; the model returned two,
   which is within the domain minimum.) The shared invariants held with no response
   edited and no extra model call.
4. **A live null thread update.** The continuation returned `upcoming_events: null`
   (keep); the opening proposed a chapter start (the domain still treats the first turn
   as not opening one); both returned all three quick-action kinds. Other null/empty
   and retitling cases keep their fixture evidence.

## Limits (not established)

One run, one call per kind; the account default model only; one org, whose advisor
policy may differ elsewhere. Not exercised live: `--model`, rate-limit/overload
shapes, a second `StructuredOutput` block, preview/final disagreement (fixture-only),
the `ping`/`error` stream events (documentation-only), signal exit codes, concurrency,
long-narrative scale. Cancellation evidence covers the direct child, not descendants
or server-side work. This is neither the headless acceptance gate (~six turns and a
chapter break) nor persistence/presentation acceptance.

## Verification

`contracts.log` records the passing shared gate (workspace tests, compile-fail
examples, registry, boundaries, all twenty-nine mutations); formatting and
warnings-denied Clippy pass. The harness is an observational example, so this is green
coverage plus live evidence, not a red/green production cycle.
