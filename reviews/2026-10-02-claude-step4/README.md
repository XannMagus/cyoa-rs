# Claude adapter step 4 — private stream-json codec

Baseline `918c330`. Adds `backends::claude_cli::protocol` (private, offline) and
enables serde_json's `raw_value` feature (workspace `Cargo.toml`; no lockfile
change) so the payload can be taken as its exact span. No `Backend`, no process
interaction, no live claim. Affected decisions: STREAM-001, TEXT-001, BACKENDS-001,
ARCH-002 (telemetry policy). Statuses stay partial. The module is
`#[cfg_attr(not(test), allow(dead_code))]` until the adapter consumes it next step.

## Policy as implemented (and where it came from)

- **Block correlation by (message ordinal, index).** Live evidence: the opening
  capture's second assistant message restarts indices at 0 after the CLI's enforce
  prompt. A second `StructuredOutput` block in the *same* message is
  `AmbiguousPayload`; one in a *later* message is tolerated (the `result` is
  authoritative) but never previews, and preview ends at the first payload block's
  stop or its message's end, so an abandoned attempt cannot splice into the scanner.
  The later-message case is **synthetic only**: no live capture produced it.
- Only the correlated block's `input_json_delta` is a preview; empty fragments are
  not forwarded (every live capture starts with `""`). Text, thinking, advisor and
  other tool blocks, and an advisor's own `input_json_delta`, are ignored.
- One `result`, last: duplicate (even identical) → `DuplicateResult`; anything after
  it → `Order`; none → `Incomplete`. `is_error` beats `subtype: "success"` and beats
  a present payload (the candidate is still retained as audit evidence). A
  non-object or null `structured_output` is `MissingPayload`; duplicate keys inside
  it are `InvalidPayload`.
- `init` is required before stream events and a success result. `system` subtypes
  other than `init` are tolerated before or after it (`commands_changed` precedes
  `init` in a live capture). An unknown top-level type or stream event is
  `Unsupported`.
- `apiKeySource != "none"` → `MeteredAuth`: a backstop (the request is already
  under way); the real guards are the preflight and the environment allowlist.
- Usage: input total = `input_tokens` + cache creation + cache read, only if all
  three are present (else unknown); cached = cache read; output independent.
  Provenance: observed model from `init.model`, provider as reported by `modelUsage`
  for it (`firstParty`, an observed string, not an invented brand), and a USD
  list-price cost only when every `modelUsage` entry reports `costBasis: "list"`.
- **UNVERIFIED, documentation-only:** a stream `ping` event is tolerated and a stream
  `error` event is a `VendorError`, from Anthropic's published streaming docs; neither
  appeared in any live capture. They are covered by one inline test, labelled so.

## TDD record

A skeleton codec (every record accepted, end of stream = `Incomplete`) was written
first and all ten tests were run: **ten runtime reds**, each on a meaningful
assertion (e.g. the live bad-model result classified `Incomplete` instead of
`VendorError`; a preview fragment expected at the delta record was `None`). The real
codec was then implemented. One test then failed from **my test's bug** (an empty
fragment produced `{},` and so invalid JSON), which was fixed in the test, not the
codec. All ten pass. Tests are driven by the step-1 expectations (hand-written, not
produced by this codec): 19 rejected and 10 completed synthetic variants, five live
captures of this review, two 2026-09-25 captures (one with advisor blocks and a
leading text block) and the two 2026-09-25 failure captures (bad-model, SIGTERM).

## Registry

Ten tests registered under STREAM-001 (all), TEXT-001 (exact span; sticky failure
keeps the candidate), BACKENDS-001 (error/completed/live-failure replays) and ARCH-002
(usage/provenance policy).

## Verification and limits

fmt, warnings-denied Clippy and the full contract gate pass: `contracts.log`.
Not established: how the supervisor splits records on a real child (covered by the
Codex real-child tests and the next step), and live behavior beyond the captures.
