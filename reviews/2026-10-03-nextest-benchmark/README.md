# Cargo versus Nextest benchmark — 2026-10-03

Measured revision: `e58e3e0b4e6fdf1bc38399ebaf1e2d3fa93fd965`.
AMD Ryzen 9 5900X, 12 physical / 24 logical CPUs; Cargo and Rust 1.95.0;
cargo-nextest 0.9.132. Ordinary local machine, no CPU affinity or frequency control.
No source changes were made for this measurement. No contracts changed.

## Method

Seven warm trials per runner per scope, alternating runner order, after workspace
warmups. Both runners use identical `--workspace --lib --bins --tests` selectors,
locked/offline dependencies and the same explicit test-thread limit (24 or 8).
Both inventories contain 327 runtime tests. Domain-only trials select `cyoa-core`
and use 24 threads (60 tests). The Nextest default profile is used, not the mutation
profile. Timing includes Cargo invocation, cached build checks and runner overhead.
A dependency-free Rust `Instant` timer captures combined output and exit status.

Doctests are separate because Nextest does not run them. Cold compilation means
an empty Cargo target directory, with dependency downloads and filesystem caches
already warm. One trial per runner, Cargo first; this is an illustrative comparison,
not a statistically established compilation difference.

The temporary evidence from the preceding session was lost. These are fresh
measurements; earlier numbers are not mixed into this dataset. `timings.tsv` retains
warmups and failures. Medians below include successful measured runs only, with
pass counts explicitly shown. Failed Cargo runs can stop before later test binaries.

## Results

| Scope | Cargo successful median | Nextest successful median | Cargo passes | Nextest passes |
|---|---:|---:|---:|---:|
| Workspace, 8 threads | 6.917 s | 3.146 s | 5/7 | 7/7 |
| Workspace, 24 threads | 6.798 s | 2.302 s | 5/7 | 1/7 |
| Domain, 24 threads | 0.052 s | 0.147 s | 7/7 | 7/7 |

At eight threads, Nextest's successful median was 2.20 times faster (55% less
elapsed time). At 24 threads its single passing run does not establish a reliable
speedup. For these tiny domain tests, Nextest's process overhead makes it about
2.83 times slower; its per-test timeout/isolation remains useful for mutation testing.
Cargo doctests passed in 0.560 s and must be added separately to a Nextest workflow.
Cold compilation took 10.204 s with Cargo and 10.092 s with Nextest: essentially
the same in this single pair. Neither runner avoids compiling the test binaries.

## Failures found

Six Nextest and two Cargo 24-thread runs failed
`buffered_multiple_lines_are_read_without_waiting_for_more_kernel_bytes`.
One Cargo eight-thread run failed the same test. Logs show
`Resource temporarily unavailable (os error 11)` before the test's six-second
wait for `Session closed` expired. The fixture sends 400 `/help` commands; source
inspection and the captured error indicate stderr backpressure under scheduling
load. This is a concurrency-sensitive fixture/app-output interaction, not evidence
that Nextest itself is defective. No assertion was weakened or test excluded.

Another Cargo eight-thread run failed
`preparation_failure_and_removed_executable_never_launch_a_generation`:
launching a just-copied Claude fixture returned `Text file busy (os error 26)`.
The recorded error is real; this benchmark does not establish its root cause.
Neither issue involves a live vendor call. Both need follow-up before treating
full-suite runs as consistently reliable. Seven clean Nextest eight-thread trials
are evidence for this batch, not proof of absence of flakiness.

## Evidence and reproduction

`run.sh` and `measure.rs` reproduce commands and timing methodology. Run from a
quiet machine and archive existing outputs first: filenames are reused and timing
rows append. `timings.tsv`, inventories and all per-run `.log` files retain raw
observations; `summary.json` separates successes and failures. Re-running the full
mutation gate was unnecessary because this change only records benchmark evidence.
