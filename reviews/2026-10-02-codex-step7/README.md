# Codex adapter step 7 — live acceptance and peer handoff

Baseline: `99067ab`. Scope: Codex adapter plan step 7, before presentation or
headless play. No production behavior, shared DTO, schema or prompt changes.
Affected decisions: BACKENDS-001, STREAM-001, TEXT-001 and ARCH-003.
The registry and all existing behavioral expectations remain unchanged.

## Reproduction and evidence ownership

`cyoa-infrastructure/examples/codex_adapter_acceptance.rs` is an opt-in Linux
harness using `StoryUseCases -> GenerationEngine -> CodexCliBackend` and bundled
templates. Its recording Backend delegates every generation to the real adapter;
it does not implement a substitute codec, process runner or game engine.
It refuses to overwrite an existing attempt directory.

```sh
cargo run -p cyoa-infrastructure --example codex_adapter_acceptance --locked --offline -- NEW_DIRECTORY
```

Each generation has the production 180-second deadline, 1 MiB stdout cap and
256 KiB stderr cap. Authentication is checked by the adapter with the same
selected executable/home environment used for generation. No model override is
selected and no API-key environment override is passed. Help/version and auth-mode
captures are adjacent to this file; credential files and environment dumps are not
collected. Installed version: **0.159.3**; status: **Logged in using ChatGPT**.

Calls are outline, cast, opening, continuation and a deliberately cancelled third
turn. The synthetic harbour brief requests two distinct playable Ajaxes; no
response is edited to create a desired result. An observer reads only the harness's
direct children via `/proc/self/task/*/children`, identifies the schema-bearing
invocation, and captures its PID, argv, cwd and adapted schema. It never signals
a child itself. For call 5 it cancels through the ordinary CancellationSource one
second after observing that child. The production supervisor handles termination
and reaping. Result artifacts check direct PID and workspace absence after return;
they do not assert that all possible descendants or remote server work stopped.

Per-call artifacts retain shared instructions/prompt/schema, observed argv and
adapted schema, raw JSONL stdout, separate stderr bytes, exact decoded payload,
normalized token counts and complete callback text/timing. Success through the
adapter establishes zero exit and cleanup; the adapter does not expose a raw exit
code on cancellation, so none is fabricated. Requested model and observed model
are distinct; absent provenance is retained as absent. Debug state snapshots are
inspection evidence, not a save format. Application narrative progress is recorded
separately from the low-level complete-JSON emission.

## Attempts

`sandbox-attempt/`: authentication preflight passed, but the first generation
failed after 46 ms with empty stdout. Stderr reports in-process app-server
initialization on a read-only filesystem. This is an outer execution-sandbox
failure, not a schema rejection or completed model request.

`live-attempt/`: explicitly approved rerun outside the outer sandbox. Each call
is a separate one-shot generation. There is no automatic retry in the harness or
adapter; the sandbox rerun is retained as a separate diagnostic attempt.

## Verification

`contracts.log` records the passing shared contract gate: workspace tests,
compile-fail examples, required-test registry, dependency boundaries and all
eighteen isolated behavioral mutations. Formatting and warnings-denied Clippy
also pass. This is green coverage of unchanged production code plus an opt-in
observational harness, not a claimed red/green production implementation cycle.

The [Claude handoff](../../docs/plans/phase1-claude-adapter-handoff.md) specifies
peer-owned schema/invocation changes, StructuredOutput block correlation,
`is_error` precedence, exact embedded-payload extraction and separate fixture/live
gates. Claude has not been run this session. Neither this harness nor the earlier
fixture story establishes playable headless acceptance, persistence, full account/
tool isolation, all models/versions, or arbitrary future model compliance.

## Observed live result

**Passed.** `live-attempt/result.json` records five generation calls, two committed
turns, a cancellation outcome and exact full-state equality on cancellation.

| Call | Outcome | Elapsed ms | Input / cached / output tokens |
| --- | --- | ---: | --- |
| 01 outline | accepted | 46356 | 14842 / 0 / 827 |
| 02 cast | accepted | 124380 | 15669 / 0 / 2591 |
| 03 opening | accepted | 81850 | 21452 / 0 / 1845 |
| 04 continuation | accepted | 82717 | 22412 / 0 / 1838 |
| 05 cancellation | cancelled | 1016 | unreported |

All four successful calls have thread.started → turn.started → one
item.completed(agent_message) → turn.completed, zero stderr bytes, and exactly
one complete emission equal to the final decoded payload. Observed child PID
and workspace are absent after each call. The cancelled call contains only
thread.started/turn.started, no payload, no usage and no prose emission. The
reported cancellation-to-return interval is approximately 10 ms (measured through
observer join/evidence writing, so a conservative local interval).

The outline describes Greyhaven's missing bell, two lighthouse stations and tidal
passages. The opening introduces Nessa delivering the anonymous measurement letter;
the continuation picks up at the speaking tube and reaches the seven-step market
stair. Both offer cautious, bold and social actions and update `protagonist` and
`nessa-pike` without changing their durable fields. The opening proposes a chapter
start; continuation supplies `starts_new_chapter: false` and a null title. This is
not a later chapter-boundary or retitling probe. Upcoming events are populated
replacement lists in both turns, not null/empty cases.

The cast model interpreted the brief as four decorated Ajax player alternatives
and one conditional “Ajax of the Other Lantern” NPC. No exact equal-name records
or ID collisions were generated, and the prose-dependent NPC description is a
fiction-quality limitation, not a domain/adapter failure. The shared invariants
were preserved; no response was edited or extra model call made to force coverage.
The existing composed fixture tests remain the evidence for exact namesakes,
collision repair, restored limits, chapter transitions and null/empty threads.

Step 7 is complete for the bounded adapter scope. Next is the peer adapter slice,
with its independent live status, followed by presentation/headless acceptance.
