# Claude adapter handoff after Codex step 7

Scope: Phase 1 item 5, independent of the later presentation/headless gate.
Claude and Codex remain co-equal. Claude's existing live observations are in
`reference/01-claude-cli.md`; this handoff adds no Claude live verification.

## Starting point

Reuse `backends::process::run`, `RequestWorkspace`, checked `ProcessBounds`,
explicit environment selection, cancellation and structured failure evidence.
Codex's adapter shows how to reconcile a private codec with transport cleanup,
but its protocol rules are not Claude rules. Keep the existing eighteen mutation
checks and composed acceptance tests. Do not create a second process runner.

Before implementation, read the full plan, contracts/required-test registry,
Phase 0 acceptance and both backend references. Recheck the installed Claude
version/help and current official Claude documentation. Run bounded authenticated
probes where Claude auth exists; otherwise label documentation-based scaffolding
and synthetic tests as live-unverified. Never change authentication modes.

## Claude-owned preparation

Create `backends/claude_cli.rs` and private codec modules. Keep compatibility in
`generation/backend_compat/claude_cli.rs`; preserve the existing root `$schema`
removal and its exact regression. Do not require Codex's all-properties-required
or annotated-reference adaptation. Leave shared wire/schema/template code intact.

Use the independently verified flag surface from the Claude reference:
`-p --safe-mode --tools "" --permission-prompts none --no-session-persistence`,
`--system-prompt`, `--json-schema`, and
`--output-format stream-json --verbose --include-partial-messages`.
Append `--append-system-prompt "Do not consult the advisor tool for this task. Answer directly."`
locally to this invocation, never to shared instructions. User text goes on stdin.
Check argv size/launch failure; do not invent unsupported file-input flags.
Do not use `--bare`, which bypasses subscription authentication.

## Private protocol and reconciliation

Use Claude's frozen stream evidence from the dated reference/review links.
Correlate `input_json_delta` with the index of the block whose start names
`StructuredOutput`. Text, thinking, advisor and unrelated tool blocks are not
narrative. Define supported metadata and ordering from Claude evidence; do not
copy Codex's reject-all-other-items policy blindly.

Final `result.structured_output` is authoritative. Extract its original JSON span
(e.g. serde raw-value support), rather than reserializing a Value and claiming
byte preservation. Preview fragments are tentative and may differ from the final
payload. Preserve transport bytes separately. Check `is_error` even when subtype
says success. A result candidate cannot override nonzero exit, incomplete stdin,
observed cancellation, deadline, output cap, protocol or cleanup failure.

Missing/duplicate/conflicting results and ambiguous payload blocks need explicit
error policies and tests. Usage may be unknown; cached counts use the existing
normalizer. Report selected versus observed model separately. A list-price cost
estimate is not subscription spending. Preserve candidate plus diagnostics on
cancellation/timeout through the application boundary.

## Acceptance order

1. Freeze documented/live invocation and supported protocol with provenance.
2. Error → edge → nominal tests for preparation and pure codec; observe meaningful
   red before implementation, without manufacturing failures in existing behavior.
3. Implement `Backend` with actual fixture children, transport reconciliation and
   cleanup checks. Include complete-only, correlated previews, final disagreement,
   `is_error`, nonzero-after-result, missing result and idle cancellation.
4. Compose `GenerationEngine` and `StoryUseCases`: edited outline, cast/selection,
   opening/continuation, identities, active limits, chapter/rewind context, unchanged
   full state on failure and one child for an explicit retry. Reuse scenarios, not
   Codex envelopes. Register actual tests under the owning contracts.
5. Add persistent behavioral mutations for Claude-specific acceptance mistakes.
6. Run the real adapter using bundled outline/cast/opening/continuation requests
   and controlled cancellation with actual cleanup evidence. Record Claude's own
   results in its reference. Fixture tests never establish authenticated behavior.

Run formatting, clippy and `bash scripts/check_contracts.sh`. Affected decisions:
BACKENDS-001, ARCH-001/002/003, TEXT-001, STREAM-001 and the identity/prompt/limit/
chapter contracts exercised by composed acceptance. Keep broad statuses partial.

After this adapter slice, Phase 1 items 6–7 introduce presentation lifecycle,
worker ownership and memory-only headless/demo commands. Live headless acceptance
(~six turns and a chapter break) remains separate for each backend. Neither a
small adapter harness nor the Phase 0 fixture is that product gate.
