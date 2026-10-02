# Claude adapter step 5 — Backend reconciliation over real children

Baseline `bf59ad1`. Adds `ClaudeCliBackend` (`backends/claude_cli/adapter.rs`), a
Claude argv mode in the fixture binary and `tests/claude_backend.rs`. No live call is
made in this step; fixture payloads are synthetic. Affected decisions: BACKENDS-001,
STREAM-001, TEXT-001 (statuses stay partial/enforced as before).

## Behavior

- `connect`: `auth status --json` via `process::run` with the generation executable
  and environment; passes only `loggedIn` + `claude.ai` + `firstParty`. A nonzero
  exit, unparseable output or any other value is `Unavailable` and no generation
  child ever launches. Never changes the login mode.
- `generate`: pre-cancel check → preparation (a preparation error, including the
  argv guard, is `Unavailable` and launches nothing) → one supervised child. The
  record callback feeds the codec and forwards each correlated preview fragment to
  `on_json` immediately; a codec rejection returns `ConsumerRejected`, so the
  supervisor kills and reaps the group.
- `reconcile` orders evidence like Codex: transport failure first (carrying the
  candidate), then observed cancellation, then the codec's outcome, then
  `GenerationResponse::from_json` on the exact span with transport diagnostics and
  observed provenance. If no preview was forwarded the payload is emitted once, after
  cleanup and cancellation checks; after previews it is not re-emitted (the engine
  appends any remainder from the final value). Cancellation is re-checked after
  emission.
- Fixture: `-p …` and `auth status --json` argv select vendor mode; the Codex
  behavior, including its `schema.json` report and cleanup obstruction, is
  unchanged (the 18 + 15 + 6 + 30 Codex/shared fixture tests pass unmodified).

## Findings worth keeping

- Previews are emitted **before** a later failure (nonzero exit, rejected result),
  unlike Codex's complete-only profile. The error still carries the candidate and
  full transport bytes; the engine never commits a preview. This is covered by
  `result_cannot_override_nonzero_exit_and_previews_are_tentative_evidence`.
- `transport_error`/`safe_transport_summary` are duplicated from the Codex adapter
  with Claude wording instead of extracted, to avoid editing the peer's file and
  breaking its mutation patches. A behavior-preserving extraction is a possible
  later cleanup.
- The auth preflight's failure diagnostics include the raw status stdout, which can
  name the account's email/org. It stays in local `TransportDiagnostics` (as Codex's
  does); do not paste it into evidence.

## TDD record

**Not red-first, and recorded as such.** The adapter was written before its tests, so
all 19 tests passed on first run; no failure was observed or manufactured. Their
ability to detect regressions is established separately by the persistent mutations
added in the mutation step. Existing coverage reused unchanged: codec and
preparation tests from steps 3–4.

## Registry

All 19 tests under BACKENDS-001; four under TEXT-001 (exact bytes, candidate
retention, application evidence); eight under STREAM-001 (cancellation, timeout,
preview semantics, cap, final-wins, complete-only).

## Verification and limits

fmt, warnings-denied Clippy and the full gate pass: `contracts.log`. Not claimed:
any authenticated behavior, composed `GenerationEngine`/`StoryUseCases` acceptance
(next step), or that a real `claude` exits as the fixture's children do.
