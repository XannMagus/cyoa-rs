# Presentation and headless handoff after the Claude adapter slice

Written 2026-10-02 after the Claude adapter slice (steps 0–8) landed on
`phase1-claude-adapter`. Scope: Phase 1 items 6–9 of
[the Phase 1 plan](phase1-headless-backends.md) — presentation lifecycle and worker
ownership, headless/demo commands, the extended contract gate and the live headless
gate. **Controller/worker item 6 is implemented offline; headless items 7–9 remain
pending.** Both backends are co-equal; neither adapter's live
adapter gate is the headless gate. Read PLAN.md, the project contracts and the
required-test registry first, as always.

The next slice now has a detailed
[controller and worker plan](phase1-controller-and-worker.md), written after the
Claude review repairs (`9801577`, `d22db81`). Those repairs raise the retained
mutation baseline from 29 to 31. The controller/worker adds three more (34 total);
see its [implementation record](../../reviews/2026-10-02-controller-worker/README.md).

## What exists to build on

- `StoryUseCases<GenerationEngine<B>>` with `B: Backend` for `CodexCliBackend` or
  `ClaudeCliBackend`: synchronous `generate_outline`, `generate_world`, `take_turn`,
  `rewind`. Both backends are `Send` and `&mut self` per call, so a worker thread can
  own the use cases plus a snapshot, as item 6 specifies. Failed generation never
  commits; retry is the player's decision and launches exactly one new child.
- `connect` (both adapters) runs a blocking auth preflight and fails closed
  (`BackendError::Unavailable`); it takes a cancellation token. Run it on the worker or
  before the UI starts, not on the input thread.
- `CancellationSource::cancel` stops the actual child: observed live for both
  backends (Claude: ~15 ms to return, direct PID and workspace gone, game unchanged).
- Per-process bounds are fixed in the adapters' configs (180 s deadline). Executable
  resolution is a shared vendor-blind module (`backends::executable`).

## Behavior differences the controller must not paper over

- **Progress shape.** Codex is complete-only: one narrative emission after the process
  is reconciled. Claude streams: hundreds of narrative fragments per turn, forwarded
  *as they arrive*, so they can precede a failure, a cancellation or a final payload
  that disagrees with them. Treat all preview text as transient and replace it with the
  committed turn; never persist it.
- **Latency.** Claude's first fragment arrived after 1.7–4.2 s on three calls but 19.5 s
  on the opening, where the model first wrote prose as plain text and the CLI forced a
  second message. Show a spinner, not an assumption that streaming starts promptly.
- **Failure evidence.** `GenerationFailure` carries the candidate payload (when one
  existed) and the exact transport bytes separately; a result that was rejected or
  outlived by a nonzero exit keeps its payload, a truncated or missing result has none.
- **Cost/provenance.** Claude reports observed model, provider and a *list-price
  estimate*; never present it as subscription spending. Codex reports neither. No model
  is requested by default for either.
- **Auth.** Claude accepts only `claude.ai` + `firstParty`; Codex only a ChatGPT login.
  A `cyoa doctor` should report both CLIs and warn on anything the adapters refuse; it
  must never change a login mode.

## Obligations that remain

1. Items 7–8: memory-only headless and demo commands, input/output/signal shutdown
   and binary acceptance; extend the gate while retaining all 34 current mutations.
   Item 6's typed IDs/revisions, canonical ownership, joining and cancellation are
   implemented; they do not establish stoppable terminal input.
2. Item 9, **once per backend**: roughly six turns through a chapter break with real
   player input, a real cancel then explicit retry, and honest reporting of what was
   not observed. A fixture or an adapter-gate run does not substitute.
3. Not yet exercised live for Claude: `--model` (never passed), rate-limit/overload
   shapes, a rejected-then-retried payload block, preview/final disagreement, the
   `ping`/`error` stream events (documentation-only), signal exit codes and
   concurrency. Not exercised live for either: long narratives at scale.
4. Optional cleanup: the supervisor-error → `BackendError` mapping is duplicated
   between the two adapters (deliberately, so neither slice edited the other's file);
   a behavior-preserving extraction would need its mutation patches refreshed.

Do not reuse one backend's terminal semantics for the other, loosen a domain invariant
because a model produced an inconvenient story, or add automatic retry.
