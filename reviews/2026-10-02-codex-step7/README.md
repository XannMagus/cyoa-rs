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

The cast contains four distinct playable characters with decorated Ajax names
and one conditional “Ajax of the Other Lantern” NPC. The original brief did not
say **exactly** two Ajaxes, while the bundled prompt requested 3–5 playable
characters. Four distinct playable Ajaxes therefore did not violate an exact-two
constraint: no such constraint was supplied. No exact equal-name records or ID
collisions were generated. The shared invariants were preserved; no response was
edited or extra model call made to force coverage.
The existing composed fixture tests remain the evidence for exact namesakes,
collision repair, restored limits, chapter transitions and null/empty threads.

Step 7 is complete for the bounded adapter scope. Next is the peer adapter slice,
with its independent live status, followed by presentation/headless acceptance.

## Brief clarification after the live run

Following user clarification, the opt-in harness now requests 3–5 distinct playable
characters with exactly two Ajaxes across the entire cast, both playable and each
using the exact name `Ajax`. Their different histories distinguish them without
decorating their name fields. This targets the intended equal-name scenario while
respecting the bundled cast-size request. The preserved attempts above used the
original brief; the subsequent revised run is recorded below. This change affects
only the synthetic harness brief, not bundled generation prompts or game rules.

## Revised brief run, 2026-10-02

The revised prompt was then run live in a fresh `revised-live-attempt-outside/`
directory. The first sandbox attempt is retained separately in
`revised-live-attempt/`; Codex initialization failed on the read-only filesystem.
The outside-sandbox run used the existing ChatGPT login, Codex 0.159.3, the real
adapter and the same bounded requests/cancellation sequence.

The cast response passed the stated constraint exactly:

```text
Playable names: Ajax, Ajax, Mara Venn, Silas Rook
Exact "Ajax" playable count: 2
Exact "Ajax" NPC count: 0
```

Both Ajaxes were distinct people with different histories and descriptions. The
two other playables and all six NPCs had other names. No generated playable had a
surname, station label or epithet in its name field. The cast had four playables,
within the bundled 3–5 request. This directly exercises a real cast response
containing two equal names. Selection keeps the chosen playable and the NPCs;
the two playable Ajaxes were not both present in the subsequent story summary.
The harness's later turn requests succeeded. Independent updates to two same-name
characters in one summary remain covered by the composed fixture tests.

Outline, cast, opening and continuation all succeeded in 41.893, 127.834, 73.444
and 72.648 seconds. Each emitted one exact complete payload, reported no model
identity, and returned after observed child and workspace cleanup. Controlled
cancellation returned in about 10 ms with no output, absent PID/workspace and
unchanged state. The top-level harness recorded `accepted: true`, five generation
calls and two committed turns. Token usage and exact payloads are retained in that
attempt directory. This run confirms the clarified cast constraint for this model
response; it does not guarantee future model calls obey every instruction.
