# Phase 1 — real CLI backends and headless play

Status: **planned, not implemented**. Written 2026-09-25 against `3dfce38`.
This is the next implementation sequence after
[Phase 0 acceptance](../decisions/phase0-acceptance.md). It refines
[PLAN.md](../../PLAN.md)'s walking-skeleton phase; project contracts and explicit
user decisions retain precedence. Proposed names below may change during a
behavior-preserving refactor; the required behaviors and evidence must survive.

## Outcome and scope

Deliver `cyoa play --headless --backend <claude|codex>`: enter a brief, generate and
accept/edit the outline, generate a cast, select a protagonist and play successive
turns. Narrative progress appears when the selected backend supplies it; complete
responses work too. Failures are visible, state remains intact, and retry is an
explicit player action. Cancellation must terminate the actual subprocess and
prevent a late result from becoming canonical game state.

Deliver `cyoa play --headless --demo` through the same application/presentation
path, without a vendor CLI, credentials or network. Keep headless mode permanently
as a debugging interface. Demo does not stand in for live acceptance.

Both backends are co-equal. Implement and exercise the authenticated backend for
the implementing session; scaffold the other from its own reference and current
published documentation, clearly recording what was not exercised. One live
backend makes the skeleton usable; Phase 1's **both-backend acceptance remains
open** until both pass. Never silently select a different backend on failure.

Out of scope: save/load/autosave/migrations, TUI rendering, exports, character-edit
propagation, semantic cast consolidation, image generation, a plugin registry,
full XDG configuration and `doctor`. Runtime backend/model/transport settings do
not authorize prompt overrides: `GenerationTemplates::bundled()` remains the public
entry point until PROMPTS-003's semantic validator is implemented. Headless play
must state that sessions are currently memory-only; do not imply failures are
protected by an autosave that does not exist.

## Starting point and gaps to close

| Existing component | Reuse | Missing behavior |
|---|---|---|
| `StoryUseCases<G>` / `StoryGenerator` | Typed outline/cast/turn generation and rewind | Presentation orchestration and ownership of in-flight work |
| `GenerationEngine<B>` | Rendering, wire decoding, checked construction, prose scanner | Live transport diagnostics/provenance mapping |
| `Backend: Send` | One exclusive synchronous call; raw structured JSON fragments | Child lifecycle, event protocols, final success reconciliation |
| `CancellationToken` | Observation-only token, caller-owned source | Waking while pipes are silent, killing/reaping children |
| `GenerationResponse::from_json` | Raw payload and parsed value constructed together | Extracting exact payloads from vendor envelopes |
| `BackendError` / `GenerationFailure` | Error-as-value and state preservation | Lossless transport diagnostics, distinct timeout/protocol outcomes where useful |
| `backend_compat::claude_cli` | Isolated root `$schema` adaptation | Full adapter; Codex-specific adaptation in its own sibling file |
| Scripted/chunked transports | Deterministic engine acceptance | Subprocess fixture executable and binary-level headless tests |
| Presentation and `main.rs` | Existing help/version and composition boundary | Command parsing, lifecycle, input/output, worker wiring |

Do not recreate the game engine inside either backend or the headless controller.
Do not move the low-level JSON `Backend` into application/domain. In particular,
`GenerationEngine` currently drops transport usage and supplies default provenance;
that is a known mapping gap, not evidence of working model/usage reporting.

## Architecture and ownership

Proposed module homes:

| Location | Responsibility |
|---|---|
| `cyoa-infrastructure/src/backends/process.rs` | Vendor-neutral process launch, concurrent pipe I/O, cancellation, exit and cleanup |
| `cyoa-infrastructure/src/backends/claude_cli.rs` | Claude invocation and event state machine |
| `cyoa-infrastructure/src/backends/codex_cli.rs` | Codex invocation and event state machine |
| `cyoa-infrastructure/src/generation/backend_compat/{claude_cli,codex_cli}.rs` | Backend-local schema adaptations only |
| `cyoa-application/src/generation.rs` | Necessary vendor-neutral outcomes/diagnostics/progress contracts |
| `cyoa-presentation/src/{commands,headless,worker}.rs` | CLI intent, lifecycle reducer, terminal I/O and worker coordination |
| `cyoa-cli/src/main.rs` | Parse intent, build chosen concrete adapter/use cases, inject into presentation |
| `cyoa-infrastructure` test-support fixture bin (ungated `[[bin]]`, reached via `CARGO_BIN_EXE_*` by infra's own tests, via `escargot` by `cyoa-cli`'s) | Controlled child behavior for both infra and binary tests; never installed by `cargo install cyoa-cli` |
| `cyoa-cli/tests/headless.rs` | Binary-level tests composing all layers |

Keep subprocess and serde types out of application, including dev-dependencies.
Presentation tests use inward-owned fake ports; infrastructure tests compose the
real engine and real adapter; cross-layer binary tests belong in `cyoa-cli`.
Do not add a sixth workspace crate without assigning its architectural layer in
the dependency gate. The fixture lives once, as a `[[bin]]` in
`cyoa-infrastructure`. That crate's own supervisor tests reach it directly via
`CARGO_BIN_EXE_*` (verified: this variable resolves only for integration tests
of the crate that owns the bin). `cyoa-cli`'s binary tests cannot use that
variable across the crate boundary, so they build the same bin at test time via
`escargot` (`-p cyoa-infrastructure --bin <name>`), honoring the active
`CARGO_TARGET_DIR` — including the mutation runner's isolated one — and passing
`--locked --offline`. `escargot` is a new dev-dependency of `cyoa-cli` only; the
offline gate needs `cargo fetch --locked` to pick it up.

Use `std::process` and a supervisor thread, not an async runtime: a sync
readiness loop (`rustix::event::poll`) over the child's stdout/stderr fds, stdin
set non-blocking and polled for `POLLOUT` alongside them so a full stdin pipe
never needs a separate blocking writer thread, and a self-pipe/eventfd fd the
supervisor also polls to wake on cancellation without blocking on any one fd.
`CancellationSource`/`CancellationToken` (`cyoa-application/src/cancellation.rs`)
stay OS-neutral: `cancel()` flips the existing atomic and additionally invokes
registered notifier callbacks. Infrastructure owns the eventfd/self-pipe and
registers a notifier that writes to it; no fd or rustix type appears in
application. A notifier registered after `cancel()` already fired must be
invoked immediately on registration — a lost wakeup here reproduces the idle-
cancel hang this design exists to prevent.

Child exit alone never signals the poll set (no `POLLHUP` arrives while a
descendant still holds the pipe open), so the loop also needs an explicit exit
signal: a `pidfd` in the same poll set on Linux, or a bounded poll timeout plus
`Child::try_wait()` as the portable fallback. Once exit is known, drain whatever
is currently readable without blocking, then drop the fds — do not wait for EOF.

Generic process mechanics must not inspect Claude/Codex event names. Backend
codecs should be independently testable consuming state machines, without
spawning a process. Schema transformation functions consume and return values;
owned resource methods may use `&mut self`. Retain `unsafe_code = forbid`;
`rustix` (features `process`/`event`/`pipe`/`fs` for `O_NONBLOCK`, safe public
API) is the first dependency needed for group-signal and poll, added under
`[target.'cfg(unix)'.dependencies]` — platform coverage is Unix-first, Windows
cleanup must report unsupported rather than silently succeed.

Use named types for executable paths, model selection, request IDs, session
revisions and checked transport bounds where mixing values would be a defect.
Use enums carrying the data required by each process/lifecycle state. Do not build
`is_running`, `has_result`, `was_cancelled` and unrelated optional fields whose
combinations require comments to explain. Absence of usage differs from zero.

## Cross-cutting contracts

### Success requires the complete transport outcome

A valid structured object is necessary but insufficient. Return success only once
there is an acceptable vendor terminal outcome, successful process exit, and no
protocol/reader failure or observed cancellation. Receiving a candidate payload
must not return early before a later error or nonzero exit can be observed.

The vendor codec identifies the authoritative final payload. Preview fragments
never authorize a commit. Ignore documented unrelated metadata, but do not silently
skip malformed output needed to establish completion. Treat unknown terminal
shapes as unsupported protocol errors. Define the handling of duplicate terminal
results explicitly; conflicting terminal outcomes must not select success merely
because it arrived last. Ordinary intermediate messages must not be concatenated
into a pretend final structured response.

One user generation command launches one generation child. No application retry,
backend fallback, JSON-repair call, cast-deduplication call or subprocess retry.
A vendor CLI may have its own internal reconnect behavior; record it separately
and do not claim our spawn count controls vendor-internal model calls.

### Payloads and diagnostics have different meanings

`GenerationResponse.raw_response` is the actual structured payload, not the JSONL
transport transcript. Preserve it without trimming/reformatting. For a string
payload, preserve the string's decoded contents exactly. For an object embedded in
an envelope, extract its original JSON span (for example with serde's raw-value
support) instead of serializing a parsed `Value` and calling that original text.
The final payload can differ from preview fragments; the final response remains
authoritative. Cover that case explicitly.

Retain transport stdout/stderr separately for troubleshooting, including partial
output on failures and cancellation. A byte stream may contain invalid UTF-8;
lossy display must not replace the retained diagnostic bytes. If the current error
types cannot carry both structured payload and transport evidence, extend them in
an atomic boundary/mapping commit. Application attachments must be vendor-neutral
named diagnostic data, not serde values, child handles or vendor event DTOs.
Do not send stderr/status chatter through `on_json` to preserve it accidentally.

Optional recording writes sanitized fixture/evidence artifacts only when explicitly
enabled. Normal runs must not persist full prompts by default. Never commit auth
files/tokens, environment dumps or a user's story as a test fixture. Record fixture
provenance: live-captured, redacted, or synthetic; synthetic events are not live
verification. Preservation applies to original in-memory evidence even when a
published copy is deliberately redacted.

### Cancellation and cleanup must work without output

A blocking stdout reader cannot be the cancellation controller. The supervisor's
poll loop observes the token's wake signal, the child's stdout/stderr readiness,
non-blocking stdin's write-readiness, and an explicit exit signal (`pidfd` or a
bounded timeout plus `try_wait`), without blocking on any single one. This must
actually close the inherited-pipe case (a descendant that retains a pipe end
after the child has exited): once exit is known, the supervisor drains what is
currently readable without blocking, then drops its fds rather than waiting for
an EOF that may never come. Avoid deadlocks when the child fills stderr, never
reads stdin, closes stdin early, or a write to stdin exceeds the pipe buffer
with nothing reading it.

Use owned child/resource guards and an explicit lifecycle: spawned → stopping or
exited → reaped. Every exit path closes owned handles and reaps the launched child,
including spawn-followup failure, protocol error and cancellation. If descendants
can retain resources, implement and test process-group/job cleanup for supported
platforms. Do not promise portable tree termination based only on `Child::kill`.
State platform coverage honestly; unsupported cleanup must not silently succeed.

Define checked, configurable transport timeouts and output bounds with documented
units, defaults and rationale before implementation. These are resource settings,
not gameplay `Limits`. A deadline/output-limit failure is explicit, never a silent
successful truncation. If diagnostics must be bounded, retain the captured prefix
and an explicit truncation marker/count; never claim an incomplete transcript is
complete. Pick operational defaults from measured representative runs, and allow
finite generous test-independent settings. Reader/writer joins must themselves have
a shutdown strategy; killing the parent and blocking forever on a reader is failure.

Cancellation observed before accepting a completion wins. Presentation additionally
rejects cancelled/stale completions even if they were queued just before cancellation.
Create a fresh cancellation source for each request. Returning to idle after cancel
must mean the prior worker/child has finished cleanup, not that another generation
may overlap it.

## Implementation sequence — one atomic commit per item

Each item includes implementation, focused regressions, registry updates and a
truthful status/evidence entry. Do not commit an intentionally failing test suite.
Within an item: write/run error tests first, edge tests next, then nominal tests;
observe each new behavior's runtime failure before implementing it, then green.
Fix test-fixture/compiler errors before claiming a behavioral red. If an assertion
already passes because behavior exists, record it as additional coverage; do not
break correct code or corrupt expectations to manufacture red evidence.

After green, refine types/layering and run formatting, Clippy and the complete
contract gate. Test moves require equivalent assertions and registry updates.
Update existing mutation patches if refactoring moves their targets; stale patches
must never become a reason to disable mutation checks.

### 1. Refresh backend evidence and freeze protocol fixtures

Read both reference files, re-check installed CLI help/version/auth mode, then use
bounded authenticated probes for the available backend. Use the actual bundled
world/cast/turn requests and current schemas; label any temporary adaptation.
Record commands, version, selected model if known, exits and raw event evidence.
Use subscription authentication; never switch to an API key to make a test pass.
Re-check protocol docs when implementation begins; this plan adds no new claims
about today's vendor behavior.

For Codex specifically, the existing evidence is a successful cast on 2026-09-24
with all object properties required. It does **not** establish StoryTurn nullable
fields, long-output streaming, complete isolation or all failure shapes. Probe an
opening and continuation with nested deltas and explicit null/empty threads, and
whether a longer narrative emits genuine partial output. Do not infer optional
field semantics by blindly adding `null` to every property.

For Claude, existing evidence covers structured-output tool fragments and a final
result envelope, plus root `$schema` rejection. Re-check the actual version before
relying on those observations. Track content-block identity so unrelated tool/text
blocks cannot become story JSON. Separate confirmed observations from the mixed
historical “verified/documented” notes without upgrading claims by inference.

Failure fixtures can be synthetic when inducing a real rate limit would be wasteful;
label them. Capture success, protocol failure, no terminal result, nonzero exit,
unrelated events and preview/final disagreement for both codecs. Reorganizing a
reference file into Confirmed/Open questions is allowed; invented evidence is not.

**Exit:** available-backend questions needed to implement its codec are answered or
explicitly bounded; the other backend has a precise unverified handoff. Commit the
evidence before code depending on newly discovered quirks.

### 2. Establish transport outcome types and the subprocess test harness

Extend low-level errors/results and application mapping only where the contracts
above require it: payload versus diagnostics, cancellation evidence, timeout and
provider/model provenance. Preserve all existing error behavior and tests. Normalize
usage behind infrastructure, retaining missing versus zero and the cached-input
invariant. Token display and pricing UI remain deferred; never present notional
cost as subscription spending. Invalid telemetry must have an explicit tested
policy, not silently fabricated counts or accidental rejection of otherwise valid
fiction.

Build a Rust fixture child accepting explicit test scenarios, not shell command
strings. It can report argv/stdin, emit scripted bytes on either pipe, wait for an
acknowledgement, close a pipe, spawn a controlled descendant, or exit with a chosen
code. Keep it out of the shipped product command surface. Use handshake files or
channels for ordering; avoid timing-only sleeps as proof a child started/stopped.
External test timeouts are deadlock watchdogs, not normal control flow.

**TDD:** error mapping retains bytes; malformed payload cannot construct a success;
cancellation keeps partial diagnostics. Edge cases include empty payload, CRLF,
non-UTF-8 diagnostics, unknown usage, zero usage and invalid cache accounting.
Nominal request/response capture proves quoted/newline/Unicode text survives.

**Exit:** fixtures can exercise the real process boundary without installed CLIs,
credentials or network. Application still has no outer-layer dependency.

### 3. Implement the vendor-neutral process supervisor

Launch using executable plus argument vector (`Command`), never `sh -c` or joined
shell text. Write the request to stdin and close it. Use an isolated working
directory and explicit environment policy that preserves subscription auth without
loading project instructions accidentally. Keep request-scoped schema files alive
until process completion; clean them on every outcome. A long Claude system prompt
or schema passed through argv still faces OS size limits: handle launch failure
clearly and verify supported alternatives before inventing flags.

**Error tests first:** missing executable; spawn/pipe setup failure; broken stdin;
nonzero exit; failure after candidate success; cancellation before spawn and while
silent; deadline; invalid UTF-8 on protocol stdout; malformed/truncated JSONL;
output bound exceeded; consumer/output failure triggers cleanup.

**Edge tests:** byte splits inside UTF-8 and JSON escapes; CRLF records; final
record without newline; empty reads/EOF; stderr larger than pipe capacity; input
larger than pipe capacity; child never reads stdin; descendant retaining a pipe;
cancellation racing terminal output. Prove process and temporary-resource cleanup,
not just that a method returned `Cancelled`.

**Nominal:** one spawn, exact stdin/argv, both pipes drained, framed records in order,
success only after exit, and all resources released. Require an actual child test
for idle cancellation and backpressure; mocks alone cannot establish these.

**Exit:** supervisor runs independently of either vendor codec. Document supported
platforms and measurable cleanup bounds with generous CI watchdog margins.

### 4. Implement the available backend's codec, invocation and schema adapter

Keep vendor DTOs private. Adapt only that backend's schema copy, preserving the
shared schema and the other backend's adapter. Pair any changed schema constraints
with tests showing the original application semantics survive mapping: required
versus defaulted fields, blank values, unknown action kinds, nullable chapter titles
and null-versus-empty upcoming events. Check all three request kinds.

The codec produces raw structured fragments and a final candidate; the supervisor
reconciles terminal protocol state with process completion. Complete-only mode
emits a complete payload at most once; do not simulate token streaming with sleeps.
Map only the documented/observed final structured message, not arbitrary assistant
text or reasoning. Use the confirmed invocation and explicit subscription-preserving
flags. Account/tool isolation still requires its own evidence. For `claude_cli`
specifically, the invocation must include the confirmed
`--append-system-prompt "Do not consult the advisor tool for this task. Answer
directly."` argument (`reference/01-claude-cli.md`'s advisor section) — a
`backend_compat::claude_cli`-local addition alongside `adapt_schema`, never
folded into the shared `GenerationTemplates` instructions Codex also consumes.

**TDD errors:** error terminal with exit zero; success terminal with nonzero exit;
malformed event; missing payload; conflicting results; unrelated tool/message
mistaken for output; auth/unavailable failure. **Edges:** metadata events, multiple
content blocks, empty deltas, complete-only result, preview/final difference and
missing usage. **Nominal:** frozen world/cast/opening/continuation transcripts feed
`GenerationEngine` and typed use cases, with exact payload and expected call count.

**Exit:** real adapter passes synthetic-child integration and the available-backend
live smoke tests using actual bundled requests. Add live observations to that
backend's reference file only. No claim that the peer passed these calls.

### 5. Implement/scaffold the peer backend with the same contract suite

Repeat item 4 using the peer's own codec, invocation builder and compatibility
file. Reuse process mechanics and test scenarios, not the first vendor's protocol
assumptions. Items 4 and 5 may exchange order depending on authentication; neither
backend is designated primary. If authentication is absent, fixtures/documented
behavior can establish offline correctness, but the status stays live-unverified.
Do not block independent headless work while waiting for peer authentication.

**Exit:** both adapters implement the common low-level contract and have offline
integration coverage; their live statuses remain independent. A fixture passing
with a fake executable is never labelled an authenticated vendor result.

### 6. Add presentation lifecycle and worker ownership

Expose parsed command intent from presentation so `main.rs` can wire concrete
adapters without presentation importing infrastructure. Inject typed use cases or
an inward-owned factory; avoid a service locator or a new message bus.

Model brief/outline review/cast selection/playing/generating/failure states as enums
with required data. Every in-flight operation carries a typed request ID and base
session revision. Keep canonical state with the controller. A worker owns the use
cases and a snapshot; it can call existing `take_turn` on that snapshot and return
it after success. The controller accepts it only for the active, uncancelled
request and matching base revision. Do not commit its turn a second time. Restore
worker ownership after completion/error so retry does not need a second live worker.

Input remains responsive during generation: cancellation/quit are handled, while
ordinary new generation commands cannot queue an accidental second call. Changes
that invalidate an outline/cast/game reject old completion events. No mutex around
canonical GameState is held during inference. Prefer rejecting rewind/edits while
busy for this phase; support them later only with explicit invalidation semantics.
A cancelled request still in cleanup is not idle. Define stdin-reader shutdown so
quitting does not wait forever on a blocked input thread.

**TDD errors:** overlapping requests rejected; stale result after cancel/restart;
old-world cast response rejected; output failure/EOF while generating cancels and
joins worker; failed turn preserves the full canonical game. **Edges:** result
queued just before cancel; duplicate completion; cancel while idle; a fresh retry
after cleanup; no double commit. **Nominal:** world → edited outline → cast →
checked selection → opening → continuation with exactly one command per step.

**Exit:** pure controller tests establish transitions and real worker tests establish
ownership/cancellation. Thread/runtime details do not leak into domain types.

### 7. Wire the headless and demo commands

Proposed minimal interaction (document exact syntax in help when implemented):

- Explicit `--backend claude|codex` or `--demo`; reject conflicting choices. Do not
  discover a fallback by invoking both vendors. Optional model selection belongs
  to the chosen adapter configuration.
- Read a nonblank brief. Show generated outline, allow accept or manual title/
  description replacement, then request cast. Invalid selection prompts again
  without regeneration. Empty input accepting an outline is distinct from an
  empty player action continuing a story.
- In play: empty line continues; ordinary text is player input; `/action N` selects
  the displayed action without conflating numeric prose with a selection. Reserve
  `/retry`, `/cancel`, `/quit`, `/help`; use `//` to send text starting with `/`.
  Retry repeats the failed intent using unchanged state and a fresh cancellation
  source, not a previous partial response. Unsupported commands never become
  accidental model requests. Save/load commands remain Phase 2 work.
- Show quick actions after successful validation/commit. Mark previews as tentative;
  a failed preview must not be presented as a committed chapter. For an append-only
  terminal, use explicit preview/final-status boundaries and print a corrected final
  passage if it differs; never duplicate the ordinary matching preview on success.
- Separate story output from control/error output. Ensure streamed output is flushed.
  Ctrl-C during generation requests cancellation and cleanup; idle Ctrl-C/EOF exits
  cleanly according to documented behavior. Report I/O errors and close workers.

Use the frozen Phase 0 story or an explicitly versioned derivative for demo;
script exhaustion is an explicit outcome, never an implicit live-backend switch.
Expose inspection of canonical chapter/summary in test seams so binary tests prove
state, not just that some expected prose was printed.

**TDD errors:** bad command/selection, invalid brief, missing chosen backend,
malformed response, failed stdout, EOF during generation, retry outside failure.
**Edges:** spaces/quotes/newlines/Unicode, action numbering, slash escaping, repeated
cancel, complete-only output and demo exhaustion. **Nominal:** launch the actual
binary through piped input and a controlled child, covering the whole lifecycle,
failure/retry and clean quit. The headless demo must work with vendor executables
absent from PATH.

**Exit:** usable memory-only headless game and credential-free demo; no TUI or save
claims added to help/README.

### 8. Extend the contract gate across the process and presentation boundaries

Retain all Phase 0 tests and four mutations. Add implemented tests to the owning
contracts as they land; do not pre-register hypothetical names from this plan.
Extend mutations with focused cases for: accepting success before nonzero exit,
ignoring cancellation while output is idle, and accepting a stale worker completion.
An idle-cancellation mutant must cause a bounded assertion failure with cleanup,
not hang the gate until CI kills it. Keep exact-test outcome checks: compilation,
watchdog termination, absent/ignored tests and stale patches are harness failures,
not evidence of a caught behavioral regression.

| Decision | Required new evidence |
|---|---|
| BACKENDS-001 | Both offline adapter contract suites; separate authenticated status/evidence |
| ARCH-001 / ARCH-002 | Controller/use-case boundaries, checked selection, invalid results cannot commit |
| ARCH-003 | Backend-local adaptations and unchanged shared/peer schemas |
| STREAM-001 | Byte framing, complete/partial output, idle cancellation and actual child cleanup |
| TEXT-001 | Exact extracted payload; separate lossless failure diagnostics |
| IDENTITY-001 / IDENTITY-002 / PROMPTS-001 | Existing repaired-ID and call-count scenario through real adapter fixtures |
| LIMITS-001 / CHAPTER-001 | Active limits, retitling and bridge data survive the adapter/controller path |
| ACCEPTANCE-001 | Existing full story retained plus binary-level lifecycle acceptance |
| PRODUCT-001 | Only implemented headless/worker slice gains coverage; persistence/TUI/export remain pending |
| PROMPTS-003 | Remains pending; no CLI/config bypass enabling arbitrary overrides |

**Exit:** the shared local/CI command covers Rust tests, architecture, registry,
coverage declarations and behavioral mutations without network or credentials.
Status changes describe the actual implemented slice, not an entire future feature.

### 9. Run the live walking-skeleton gate and publish the handoff

Use the shipped headless command with the authenticated backend, the real bundled
configuration and explicit model/version recording. Play roughly six successful
turns through at least one chapter break. Exercise player input and continuation;
inspect coherent fiction, action variety, stable identities, summary memory and
previous-chapter continuity. If the model never proposes a chapter break, extend
or repeat the scenario and report the actual result; do not claim an unobserved case.

Cancel a real request and verify cleanup, then explicitly retry. Observe how
incremental or complete-only progress behaves on that vendor. Malformed response,
unknown action, namesake collision and late-failure cases remain deterministically
covered by fixtures; do not claim a live call exercised them unless it did.
No live test can prove arbitrary future model compliance with prompt instructions.

Record outcome, commands, CLI/model versions, known limitations and artifact paths
in the corresponding `reference/0N-*-cli.md`; keep a dated implementation review
linking each commit, decision and test. Record checks still requiring peer auth.
Update README/PLAN with distinct “offline-tested”, “live-verified” and “pending”
statuses. Authentication failure is a handoff condition, not permission to weaken
isolation, switch vendors silently or use metered credentials.

**Exit:** one backend may be usable while the peer remains explicitly unverified.
Declare both-backend Phase 1 acceptance complete only after the same live gate has
passed for both. Subsequent independent persistence work need not wait for peer
authentication, but v1 cannot be declared complete without both backends.

## Dependency order and review checkpoints

Items 1 → 2 → 3 precede the live adapters. Item 4 depends on its own vendor evidence;
item 5 follows the same dependency, and may remain live-unverified. Controller
item 6 can use typed fakes before both vendors are live; item 7 depends on the
controller plus at least one usable adapter and demo transport. Item 8 strengthens
the gate throughout, finalized after the public path exists. Item 9 validates the
shipped path rather than a throwaway probe.

Every item is a separate commit. An evidence-only commit can pass existing tests;
implementation commits must pass their new tests and the retained gate. Keep a
short red/green/refactor record with actual failing assertions and commands, naming
fixture mistakes separately. Do not substitute “tests were added” for observed TDD.

Before considering each item done, reviewers must be able to answer:

1. Which boundary changed, and which existing business decisions can it affect?
2. What test exercises that boundary through its real caller rather than bypassing it?
3. Does failure preserve canonical state and exact available evidence?
4. Who owns the live child, pipes, cancellation source and worker at every transition?
5. Which claims are deterministic offline evidence, and which were authenticated live?
6. Does the next prompt still carry the same repaired IDs, active limits and memory?

The first actionable implementation task is item 1, followed by the failure cases
and fixture child in item 2. This document itself authorizes no new behavior change
and does not mark any Phase 1 feature implemented.
