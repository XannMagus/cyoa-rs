# Repository architecture and progress review — 2026-10-05

Reviewed baseline: `e29dce1c2438c8623e405d6c9c341c5f628bfc65`, initially clean.
Scope: implementation against PLAN, current project contracts and required-test
registry, README progress claims, Phase 0 acceptance and Phase 2 S7 evidence.
This is a review, not a repair: production code, regression expectations and
decision coverage states were unchanged by the review. Three findings were open
at that baseline; the subsequent [repair record](repairs.md) records their fixes
and verification. The findings and original probes below remain historical evidence.

## Findings

### R1 — P2: reject duplicate Codex control keys before interpreting events

Location: `cyoa-infrastructure/src/backends/codex_cli/protocol.rs:194–199`.
Affected decisions: BACKENDS-001, ARCH-002.

`parse_event` deserializes directly into `serde_json::Value`, which silently keeps
the last duplicate object key. A candidate followed by this terminal record is
accepted by the actual Codex adapter and emits one complete payload:

```json
{"type":"turn.failed","error":{"message":"generation failed"},"type":"turn.completed"}
```

This bypasses the documented failure/ambiguity rejection policy: the failure
disappears before the state machine can inspect it. Duplicate `item.type` fields
have the same structural weakness at the candidate boundary. An exit-zero child
does not establish which of the conflicting values is authoritative.

The real-child reproduction in `boundary_probes.rs` fails its required rejection
assertion with `ambiguous Codex failure was accepted; emissions=1`. This is
synthetic malformed-input evidence, not a claim that a live Codex version emitted
such a record. Existing tests cover conflicting *separate* terminal records but
do not cover this within-record ambiguity.

Recommended repair: reject duplicate keys in Codex control objects before event
interpretation, preserve any previously captured candidate/diagnostics, and add
real-child rejection regressions for top-level and nested item control fields.
Keep the repair inside the Codex adapter (ARCH-003).

### R2 — P2: Claude authentication preflight accepts conflicting auth methods

Location: `cyoa-infrastructure/src/backends/claude_cli/adapter.rs:99–105`.
Affected decisions: BACKENDS-001, ARCH-002.

`subscription_auth` also parses into `Value`. A successful auth-status child
returning the following is accepted by `ClaudeCliBackend::connect`:

```json
{"loggedIn":true,"authMethod":"api_key","authMethod":"claude.ai","apiProvider":"firstParty"}
```

The implementation promises to reject unknown/conflicting authentication, but
discarding the first field makes this conflicting response look subscription-only.
It therefore permits generation to launch without unambiguous subscription
evidence. The later generation-stream `apiKeySource` check is a useful backstop,
but happens after launching generation. This reproduction establishes faulty
admission, not an actual billed request.

The real-child auth probe fails with `conflicting Claude auth methods were
accepted`. The existing auth regression tests wrong/missing/contradictory values
across distinct fields, but no repeated fields. Claude's generation codec already
rejects duplicate control keys; that protection does not cover auth preflight.

Recommended repair: use duplicate-rejecting auth deserialization before checking
the confirmed fields; test conflicting and repeated identical known auth keys.
Keep this vendor-specific validation in the Claude boundary.

### R3 — P3: valid input lines can fail the 64 KiB bound due to read-ahead

Location: `cyoa-presentation/src/terminal.rs:134–141`.
Affected decisions: PRODUCT-001, STREAM-001.

The reader checks the length of the entire buffered input before finding its
first newline. That buffer can contain the current valid line plus bytes of the
next line. Reading `/help\n`, then 65,530 ASCII `a` characters and a newline,
then `/quit\n` from a regular input file exits 1 with
`cyoa: input line exceeds 64 KiB`. The longest line is below 65,536 bytes, even
including its newline. The result depends on chunk alignment, so the same input
can behave differently when delivered in different chunks.

Reproduction against the built public binary:

```sh
awk 'BEGIN { printf "/help\n"; for (i=0;i<65530;i++) printf "a"; printf "\n/quit\n" }' > /tmp/cyoa-review-input.txt
target/debug/cyoa --data-dir /tmp/cyoa-review-input-data play --headless --demo < /tmp/cyoa-review-input.txt
```

`input-boundary.stderr` retains the observed error. No vendor access is involved.
Recommended repair: measure the first line when a newline is present; bound the
buffer as an incomplete line only when no newline exists. Add a regression with
a preceding short line, a valid near-limit line and trailing buffered commands,
plus an actually oversized-line rejection.

## Progress and architecture assessment

The current implementation supports the claimed Phase 0–2 feature set, with the
correctness gaps above. It is not the complete v1 product. The current top-level
status correctly leaves the TUI, exports, character editing, Calibre import,
image plumbing and arbitrary prompt overrides for later work. Missing TUI/EPUB
implementation is therefore planned work, not a newly discovered regression.

| Goals/contracts | Current code and executable evidence | Assessment |
|---|---|---|
| IDENTITY-001/002, TEXT-002 | Core world/cast construction and merge; namesake, suffix, ordering and lowercase regressions; composed story and disk round trips | Matches recorded deviations from Python in inspected paths. No name-only collapse introduced. |
| TEXT-001 | Separate verbatim text and byte diagnostics; backend candidate capture; strict save DTO/codec and audit round trips | Raw evidence remains distinct from normalized business text. |
| STATE-001, ARCH-002 | Owning SelectedWorld, private valid domain values, typed limits/actions/turns, checked restore and separate external DTOs | Strong domain modeling and lossless save reconstruction; external control/auth ambiguity gaps are R1/R2. |
| LIMITS-001, CHAPTER-001 | GameState restore rebinds every snapshot; active limits feed requests; chapter membership/title derive from markers; codec/local-disk restore tests | Original settings, no event resurrection, retitling and rewind are implemented rather than merely documented. |
| ARCH-001 | Five-crate dependency graph; typed StoryGenerator/GameRepository ports and use cases; main composes adapters; presentation owns canonical session | Dependency gate passes; inspected domain/application sources contain no concrete infrastructure/presentation imports or filesystem/process work. Worker clones do not replace canonical ownership. |
| ARCH-003 | Separate Claude/Codex compatibility modules and private codecs; shared wire/schema/template construction | Vendor adaptations remain isolated; both shared/peer leakage mutations are detected. |
| PROMPTS-001/002/003 | GenerationTemplates bundled-only public construction, same-instance rendering, strict structure, snapshots and opening identity instruction | Current mechanics implemented; arbitrary overrides remain unavailable pending semantic validation. No general semantic-validator claim is justified. |
| BACKENDS-001 | Both concrete adapters, subscription preflights, actual fixture children, independent historical live records; fresh Codex smoke below | Co-equal wiring, no automatic fallback/retry. R1/R2 remain gaps. Claude was not exercised live in this session. |
| STREAM-001 | JSON scanner, bounded supervisor capture, group cleanup/reaping, cancellation subscriptions, preview mailbox, owned worker join and bounded output queues | Gate checks cancellation/late failures/backpressure. R3 is an uncovered input-boundary case. |
| PRODUCT-001, ACCEPTANCE-001 | Scripted lifecycle; Linux headless/demo; persistence use cases/codec, atomic primary/backup writes, supervised helpers, coordinator and public restart/shutdown scenarios | Phase 0–2 features present. TUI lifecycle/play cache and Markdown/EPUB export remain absent and explicitly pending. |
| TESTING-001, TOOLING-001 | Shared shell gate, pinned Nextest/cargo-mutants, strict handwritten mutation outcome checks, domain-only automated scope, separate doctests | Executed successfully. No Python tooling added. Green tests do not cover the three newly reproduced cases. |

The filesystem path uses directory-relative no-follow access, per-slot locks,
expected-byte stamps, prepared write identity, synced backup-before-primary
replacement and reconciliation. The save codec treats snapshots as authoritative
instead of replaying deltas, and rejects lossy normalization. Coordinator retry
uses storage evidence without regenerating an accepted turn. Existing real-file,
fault-injection, helper and public-binary tests passed; no new physical power-loss
or cross-filesystem durability guarantee follows from this review.

Documentation maintenance: PLAN and README retain old present-tense statements
such as “No save adapter exists yet,” incomplete Codex status, and “Next is” the
already completed headless handoff. Their current status sections and precedence
rules settle the intended meaning, so this was not treated as missing
implementation. Label/archive these historical passages to reduce handoff errors.

## Verification

`bash scripts/check_contracts.sh` passed on the baseline production/test tree:

- 428 runtime tests passed, none skipped; seven doctest/compile-fail examples passed.
- Required-test registration/coverage and inward dependency checks passed.
- All 42 handwritten mutations were detected by their required tests.
- 179 domain mutants: 100 caught, 79 compile-invalid/unviable. Two of the caught
  mutants failed by bounded per-test nontermination, leaving 98 assertion/test
  failures. Compile-invalid cases are not detections.
- `cargo fmt --all --check` and workspace/all-target Clippy with warnings denied passed.

`gate-evidence.txt` retains summaries and every handwritten mutation result;
the full local log is `/tmp/cyoa-review-contracts.log`. The two review probes were
run separately after the gate's baseline source snapshot, and intentionally fail
the required rejection assertions. They are retained as a review artifact, not
installed as failing workspace tests. An initial harness attempt omitted the
fixture's required `exit_code`; it was corrected before collecting the retained
behavioral-failure evidence. No setup/compiler failure is counted as a finding.

To rerun R1/R2, copy `boundary_probes.rs` to
`cyoa-infrastructure/tests/review_boundary_probes.rs` and run
`cargo test -p cyoa-infrastructure --test review_boundary_probes --locked --offline -- --nocapture`.
Remove that temporary test file afterwards; this review left it absent.

## Fresh live backend evidence

Installed `codex --version`: `codex-cli 0.160.0`. The opt-in
`codex_adapter_smoke` example ran with bundled configuration and no explicit model.
The sandboxed attempt failed before generation with a read-only-filesystem
app-server initialization error. The explicitly approved rerun outside the outer
sandbox succeeded, through the actual adapter and domain outline mapping:

```sh
cargo run -p cyoa-infrastructure --example codex_adapter_smoke --locked --offline -- /tmp/cyoa-review-live-approved
```

Accepted title: “The Harbour Without a Bell”; elapsed 53,476 ms; exactly one
payload emission, byte-equal to the final candidate. Reported input tokens 14,445,
cached input 0, output 795; observed model/cost absent. Captures are in
`live-codex-0.160.0/`; failed-attempt metadata/stderr are separately retained in
`live-sandbox-failure/`. `reference/02-codex-cli.md` records this narrow additional
confirmation. This is one live outline smoke, not a new full story/headless gate,
nor proof of global instruction/tool isolation, all versions, model overrides,
rate limits, or live cancellation. Historical Claude live evidence remains
unchanged and separate from synthetic Claude review probes.
