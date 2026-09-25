# Review: Phase 1 evidence, diagnostics and process supervision

Reviewed 2026-09-25. Baseline `ecd7978` (the Phase 1 plan); reviewed HEAD
`ae8cece223a75beea4ea8c59c179e67b019d438f`. Eight commits, 70 changed files,
9,668 additions and 248 deletions; much of the volume is captured Claude evidence.
The working tree was clean at the start. This review changes no implementation.

**Assessment: useful progress, but do not build the vendor adapters on the supervisor
as though item 3 were complete.** Four additional runtime probes fail despite the
existing green gate. The most important gap is that shutdown itself bypasses output
bounds and drains before stopping the writer. Several remaining failure paths also
turn incomplete observations into success or silently lose evidence.

## Commit-by-commit assessment

| Commit | Change | Assessment |
|---|---|---|
| `b68fc3f` | Pins synchronous rustix readiness polling and the infrastructure fixture executable | Coherent architecture refinement; a poll loop is compatible with the existing synchronous ports. Treat this documented discussion as accepted, not a deviation requiring reversal. |
| `c8c7f8b` | Refreshes Claude evidence using actual bundled requests; captures advisor behavior | Substantial useful evidence, including failures and an inconclusive termination probe. Vendor-specific observations remain distinct from Codex. |
| `431c0f3` | Records advisor avoidance with an appended instruction | Two recorded successful observations support the tactic; they do not guarantee a model will never invoke the tool. Preserve codec filtering regardless. |
| `fadb4f1` | Adds byte-backed diagnostics, error mapping and real-child fixture | Correctly separates payload from arbitrary-byte diagnostics. Incomplete propagation and domain placement need attention. |
| `64991cd` | Adds actual argv/stdin reporting and tightens registration | Improves evidence: assertions now observe transmitted input rather than only canned output. |
| `059aeef` | Introduces process supervisor, cancellation notifier and successful-response diagnostics | Useful boundary, but introduces R1–R3/R5 below. Several tests bless or miss incomplete behavior. |
| `a28ba66` | Repairs final-record cancellation, record delivery and panic cleanup; corrects TDD claims | Material fixes, not cosmetic cleanup. The candid correction is valuable. Remaining shutdown/error paths still need a systematic pass. |
| `ae8cece` | Marks items 1–3 complete for handoff | Overstates item 2/3 completion. A named deferred defect is still incomplete work; README also claims an advisor adapter flag that is not implemented. |

## Verification performed

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: passed.
- `bash scripts/check_contracts.sh`: passed; **171 Rust tests including doctests**,
  registry/coverage checks, inward dependency checks and all four required Phase 0
  mutations. The initial offline attempt lacked newly locked rustix/bitflags;
  `cargo fetch --locked` resolved that environment issue without changing the lockfile.
- Four supplementary tests compiled and **all four failed at runtime** in an
  isolated archive of the reviewed commit. Source is [review_regressions.rs](review_regressions.rs)
  and observed output is [probe-output.txt](probe-output.txt).
- Inspected changes, tests, commit messages, decision registrations and committed
  Claude evidence. No new authenticated vendor call was made in this review; it
  establishes no additional live-backend claims.

Reproduce the historical failures with `bash
reviews/2026-09-25-process-supervisor/reproduce.sh` after fetching locked dependencies.
The command intentionally exits nonzero. These are review artifacts, not ignored
acceptance tests. Promote the relevant cases into the normal test targets when
fixing them; do not register intentionally failing historical probes in the gate.

## Findings requiring fixes

### R1 — P1: shutdown bypasses bounds before stopping the child

Introduced in `059aeef`, retained in `a28ba66`.
[unix_impl.rs:197](../../cyoa-infrastructure/src/backends/process/unix_impl.rs#L197)
passes `usize::MAX` into both final drains, then kills the group at line 200.
`drain_into` loops until EOF/EAGAIN, appends to two vectors and checks no cancellation
or deadline. “Nonblocking” means an individual read will not wait; it does not make
an indefinitely replenished sequence of reads bounded.

A continuously writing child can therefore keep the cleanup drain busy and grow
memory after a timeout, cancellation or output-limit decision. Even ordinary reads
append an entire chunk before testing the cap. The reproducible finite case sets a
100-byte stdout cap, writes 4,096 bytes and retains all 4,096. The diagnostic type
contains no captured-prefix/truncation metadata. The existing bound test checks only
that an error is returned and the child disappears, not memory retained or cleanup
latency under a continuing writer.

**Fix:** stop the writer before final collection; enforce finite per-stream capture
and cleanup budgets on every path, including errors. Retain only the allowed prefix
with explicit truncation metadata. Bound work per readiness iteration so cancellation,
deadline and stderr cannot be starved by stdout. Avoid duplicating ignored stderr
bytes into an extra accumulator. Add continuously producing stdout/stderr fixtures,
exact retained-byte assertions, cancellation/deadline tests and bounded cleanup.

Contracts: STREAM-001, TEXT-001; Phase 1's explicit resource-bound/cleanup requirements.
The indefinitely replenished case is a code-path finding; the retained-size violation
is runtime-reproduced. This review did not run an unbounded producer to exhaustion.

### R2 — P1: incomplete request delivery is reported as successful transport

Introduced in `059aeef`.
[unix_impl.rs:361](../../cyoa-infrastructure/src/backends/process/unix_impl.rs#L361)
turns every nontransient write error into `close_stdin = true`; successful exit never
requires `stdin_pos == spec.stdin.len()`. The module explicitly documents abandonment
as nonfatal, but that is a new policy, not a consequence of the accepted plan.

The probe supplies 2 MiB of input to a child that never reads stdin, emits a candidate
JSON object and exits zero. `run` returns `Ok`. A future codec can validate the object
but cannot establish that the requested story/context was delivered: `ProcessOutcome`
does not expose delivery status. This can silently accept generation against incomplete
input. The required test `broken_stdin_delivery_does_not_hang_and_the_process_still_completes`
currently protects that wrong success policy.

**Fix:** distinguish delivery completion from child exit. Report a typed write/short
input failure if request bytes remain undelivered; preserve diagnostics and clean up.
A request fully written into the pipe need not prove the child semantically read it,
but known undelivered bytes cannot disappear. Retain prompt-delivery failure coverage
under STREAM-001, updating the misleading test's expected outcome with this review as
rationale. Check partial writes, explicit stdin close and all non-EAGAIN/EINTR errors.

Contracts: ARCH-002, STREAM-001; planned exact-input delivery and error-as-value.

### R3 — P2: the wake notification is treated as cancellation truth

Introduced in `059aeef`; defended in `a28ba66`'s mutation accounting.
[unix_impl.rs:339](../../cyoa-infrastructure/src/backends/process/unix_impl.rs#L339)
uses only the self-pipe event, deliberately removing the token-state fallback so a
mutation test can make the wake hook “load-bearing”. No unconditional token check
precedes final successful return when there are no records.

`CancellationSource::cancel` sets the flag and then invokes callbacks sequentially,
outside its lock. A registered callback can delay the supervisor's callback. The
probe registers one such callback, cancels after the child handshake, and lets a
silent child exit: the token is cancelled, but `run` returns `Ok`. The callback is
released and joined before the assertion, so the probe leaves no blocked thread.
Current GenerationEngine post-checks may catch this in a future adapter; that does
not satisfy the supervisor's own success contract.

**Fix:** token state is authoritative; the pipe is an optimization for waking a
blocked poll. Recheck the token at loop/acceptance boundaries, including empty-output
exit. Keep a separate test demonstrating prompt wakeup using an appropriately
controlled polling interval/test seam; never remove a correctness fallback to make
one mutation observable. Define notifier lifetime/panic behavior before exposing
additional long-lived uses.

Contracts: STREAM-001; cancellation-before-completion precedence.

### R4 — P2: validation failures discard available transport diagnostics

The type/mapping gap starts in `fadb4f1`; `059aeef`/`a28ba66` explicitly acknowledge
it but leave it open. [engine.rs:107](../../cyoa-infrastructure/src/generation/engine.rs#L107)
constructs every post-transport decode/domain-validation error with empty diagnostics,
although its `GenerationResponse` already owns them.

The probe returns syntactically valid JSON with a blank world title and attached
stderr; `InvalidResponse` is correct, but stderr is lost. The same helper serves
missing wire fields, unusable cast and invalid turn construction—precisely the errors
where transport evidence is useful. Fixing only cancellation paths does not close
item 2's diagnostic requirement.

**Fix:** carry the response diagnostics into `invalid()` and cover outline, cast,
turn wire errors and domain errors. Audit the application post-success cancellation
checks too: `Generated<T>` currently does not retain diagnostics, so that later
boundary can still replace evidence with empty data. Also complete or explicitly
defer provider/model provenance mapping; `generated()` still supplies defaults.

Contracts: TEXT-001, ARCH-002; Phase 1 item 2's end-to-end diagnostic requirement.

### R5 — P2: setup/read/cleanup failures are swallowed

Introduced in `059aeef`, retained in `a28ba66`.
[unix_impl.rs:48](../../cyoa-infrastructure/src/backends/process/unix_impl.rs#L48)
ignores failure to enable nonblocking I/O; `drain_into` treats actual read errors
like EAGAIN at line 143; the poll loop only distinguishes EINTR; cleanup ignores
signal errors and returns without reporting failure to reap within its grace period
(lines 81–108). `finish()` can still return successful `ProcessOutcome` afterward.

Consequences differ by failure: a blocking descriptor can invalidate the entire
cancellation design; a reader error can silently drop output; unsuccessful cleanup
can leave a child alive while callers assume the request finished. The comments
admit this, but callers have no typed way to discover it. Existing tests do not inject
these faults. This is a static control-flow finding, not a claim to have induced an
OS-level fcntl/read/kill failure in this session.

**Fix:** distinguish EAGAIN/EINTR from fatal I/O, propagate setup/read/poll failures,
and make normal-path cleanup return an explicit result. Preserve both the initiating
failure and cleanup failure. Drop remains a best-effort backstop, not evidence that
normal cleanup succeeded. Test through a narrow syscall seam or controlled resource
faults. Retain safe Rust and the synchronous poll architecture.

Contracts: ARCH-002, STREAM-001; success requires no reader error and verified cleanup.

## Architecture, coverage and completion assessment

These are additional implementation/handoff issues, not claimed new runtime probes:

- **Wrong owner for transport detail:** `TransportDiagnostics` in
  `cyoa-core/src/text.rs` models stdout/stderr, not story business rules. It is
  vendor-neutral but still transport-specific. An application-owned diagnostic
  attachment/value type would keep infrastructure evidence at the port boundary.
  Dependency checks stay green because they check crates, not conceptual ownership.
- **Consuming-transform rule was bypassed:** production `split_records`,
  `drain_into`, `dispatch_records` and `finish` mutate caller-owned values through
  free-function `&mut` arguments. Encapsulate the accumulator/capture/child resources
  in owned structs with invariant-preserving methods, or consume and return values.
  This is the exact class of helper PLAN.md's rule addresses, not an argument against
  `&mut self` on a resource owner.
- **Working-directory isolation is not implemented:** `ProcessSpec` has no cwd and
  `Command` inherits the repository directory. Before live adapters, add a per-request
  isolated directory and resource ownership; never temporarily change global cwd.
  Item 3's original acceptance text still requires this, so mark it outstanding.
- **A green process test is not necessarily a cleanup proof:** the timeout test
  expressly lacks a PID assertion despite `a28ba66`'s message claiming deadline
  tests gained one. `/proc` checks are Linux-specific while the file is gated on
  all Unix, so they can vacuously pass on Unix systems without Linux procfs. Name the
  actually tested platform and use an appropriate process-liveness mechanism.
- **No new process mutations are retained:** scripts and the mutation manifest are
  unchanged since `ecd7978`. Manual revert/restore evidence in `a28ba66` is useful
  but not a lasting gate. Item 8 schedules them later, so this is not a claim it was
  already required complete; add key failure mutations before expanding adapters.
- **Status needs correction:** README marks items 1–3 complete and says the advisor
  flag is isolated in `backend_compat::claude_cli`. That file still only adapts the
  schema; the appended instruction exists in evidence/plans, not production code.
  Mark supervisor/diagnostics partial until these findings are resolved. Claude
  observations are valuable; neither vendor adapter nor headless flow is complete.

## TDD and review discipline

`a28ba66` explicitly corrects `059aeef`'s false red-first claims: the final-record
and dropped-record tests were initially green, and the one earlier failure was a
fixture argv-size error. The correction also reports additional tests written after
fixes and then mutation-verified. Accept the corrected account, but this does not
meet the user's requested error → edge → nominal red/green workflow.

The deeper problem is accepting locally green tests as completion while the tests
omit adversarial transitions or encode a new policy (incomplete stdin success).
Removing the token fallback to prove the wake mutation is particularly backwards:
a test should protect correct behavior, not make a redundant safety path undesirable.
The retained registry can enforce a bad assertion just as effectively as a good one;
review the behavior being registered, not only test presence/count.

## Recommended repair order

1. Bound normal and cleanup reads; stop the writer before final capture; make
   cleanup/I/O failures explicit (R1/R5). Add resource-failure tests first.
2. Make known incomplete input a failure and restore authoritative cancellation
   checks (R2/R3), preserving the wake optimization.
3. Complete diagnostic/provenance mapping and move diagnostic ownership to the
   application boundary (R4 and architecture findings).
4. Add cwd/resource isolation, strengthen real-child liveness tests and retain new
   mutations. Refactor buffers into owning types under the green behavioral suite.
5. Correct completion claims, then proceed to the available backend's actual codec
   and separately documented live verification. Keep the peer's status independent.

Each repair is its own coherent commit with observed failing tests before the fix,
updated contract registration and the existing gate intact. Nothing here authorizes
namesake/ID/limit/prompt semantics to change. No production code was fixed in this
review, and the green gate should not be described as proof these findings are fixed.
