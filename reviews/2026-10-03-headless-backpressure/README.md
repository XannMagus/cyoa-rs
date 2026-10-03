# Headless backpressure repair — 2026-10-03

The user authorized implementation after Astra validated the plan. A GPT-6 Astra
agent reviewed the architecture and implementation, then requested stronger
cancellation and real-pipe handshakes. Those findings are addressed below.
Affected decisions: PRODUCT-001 and STREAM-001; layer boundaries remain ARCH-001.
This is an offline terminal/runtime repair. No vendor-adapter or live-auth evidence
is changed, and Claude's independent live headless gate remains pending.

## Problem and behavior

The benchmark exposed `WouldBlock` on stderr when the buffered-input test sent
400 `/help` commands under parallel load. Nonblocking `write_all` treated a
momentarily full pipe as fatal, even with a reader that would soon catch up.

The headless view now renders into per-stream queues. Finite pumps retain exact
unwritten suffixes on `WouldBlock`, retry interrupted writes within a finite
attempt budget, and observe input/SIGINT and workers between pumps. Raw Linux
writes avoid std's hidden line buffering. Per-stream byte order is maintained;
merged stdout/stderr interleaving is not guaranteed.

Each queue holds at most 1 MiB, including all output rendered before the next
pump. Thus an individual oversized narrative or diagnostics burst errors even
with a healthy reader. This explicit render-burst limit is independent of backend
capture limits; no text is silently truncated. Stall clocks reset only on successful
byte progress and error after one second without progress. Pumps perform at most
64 KiB / 32 write attempts per stream. Final output has an absolute one-second drain
bound after the worker closes/joins; successful exit requires every queued byte
to drain. SIGINT can interrupt draining. Fatal I/O, overflow and timeouts clean up
the worker; the existing final best-effort nonblocking error reporter is unchanged.

## Regressions and Astra review

The existing buffered-input test still writes one batch larger than the 1 KiB read
chunk and keeps stdin open. It now uses 2 KiB of padding followed by `/help`,
`/inspect`, `/quit`, asserting help exactly once, canonical inspection and close.
Output backpressure has its own coverage rather than depending on scheduler luck.

Four queue tests cover exact UTF-8 bytes through partial writes/interruption/
backpressure, enqueue versus progress clocks, zero writes/broken pipes/overflow
(including an oversized burst with a healthy sink), and finite pump budgets.
Six runtime tests cover the 400-help stress with a paused reader, absolute final
shutdown under trickling progress, active worker cancellation/quit during blocked
output, and full real stdout/stderr pipes that resume and deliver exact narrative
or help/close bytes. Real readers resume only after the app's write actually reports
`WouldBlock`, then pause for 100 ms; both tests assert that observation.

Astra identified that quit itself could mask a failed cancel action. The cancellation
case now waits for the worker to observe the earlier interrupt before allowing quit.
A worker-start handshake ensures the generator is actually active first. Canonical
state remains uncommitted and the worker is joined before return.

An independent GPT-6 Astra final review found that checking the closing deadline
after accepting an empty queue could authorize late success. Deadline checks now
run before each final-drain pump and before successful return. A registered
regression models descheduling during input polling until the reader becomes
writable after expiry, requiring timeout and zero late-pumped bytes. Astra
independently ran all six output tests and approved the repair subject to the
final gate, with no remaining material findings.

The existing full-undrained-stderr binary regression retains its two-second bound.
Its mutation and the idle-SIGINT mutation still apply; no mutation was removed or
weakened. New regression names are registered under PRODUCT-001 and STREAM-001.

## Validation

- Focused queue/runtime/binary regressions pass; fmt and denied-warning Clippy pass.
- The full contract gate passes: registry/architecture/workspace/doctests, all 37
  handwritten behavioral mutations, and the automated domain sweep.
- Domain sweep: 176 mutants, 98 caught (96 assertion/runtime failures and two
  per-test nontermination failures), 78 compiler-invalid, zero survivors or outer
  timeouts. Domain code is unchanged; compiler-invalid mutants are not detections.
- Seven consecutive full workspace Nextest runs at 24 threads pass all 337 tests
  each. Successful invocation median: 2.352 seconds. This finite batch establishes observed reliability,
  not a universal absence-of-flakiness proof. No test was retried or excluded.

`contracts.log` retains the final complete gate. The earlier gate is retained as
`contracts-before-final-test-review.log` and `contracts-before-deadline-review.log`;
final test-strengthening was independently verified before the final gate.
`timings.tsv`, `measure.rs` and the seven `workspace-24-final-*-nextest.log`
files retain final repeated-run evidence; `timings-before-deadline-review.tsv`
and `workspace-24-*-nextest.log` preserve the earlier 336-case batch.

Reproduce the gate with `bash scripts/check_contracts.sh`. Reproduce a repeated
suite run with `cargo nextest run --workspace --lib --bins --tests --locked --offline
--color never --test-threads 24`. The Rust timer uses `Instant`, captures combined
output and exit status, and preserves failed runs rather than retrying them.
