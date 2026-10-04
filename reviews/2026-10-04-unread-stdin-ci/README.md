# Non-reading child test: remove the exit/write race

The [2026-10-04 CI job](https://github.com/XannMagus/cyoa-rs/actions/runs/37205066719/job/111444437224)
on `4fb6775` failed in the workspace runtime tests. Its public job metadata confirms
fmt/Clippy passed and the contract gate exited 100. The user supplied the failure
output: `a_child_that_never_reads_stdin_still_completes` unwrapped
`IncompleteInput { written: 0, expected: 22 }`, with stdout `ok\n`.
Authenticated CI logs were unavailable in this session. The unchanged full local
gate passed; no local reproduction of the original race is claimed.

The fixture wrote `ok` and exited immediately without reading stdin. If the child
exited before the supervisor wrote the request, success was correctly rejected.
A small request fitting in the pipe is not evidence that the write already happened.

The test fixture now waits for stdin's writer to close using poll HUP without
requesting readability or consuming any bytes. It checks FIONREAD equals the exact
request length before emitting success. A three-second watchdog bounds a broken
handshake; it is not a timing-based readiness assumption. The test retains its
successful exit assertion and additionally checks exact output, an empty consumed
stdin report, and disappearance of the actual child PID.

Affected contracts: **STREAM-001, TEXT-001** (complete request delivery and real
process cleanup). This is a test-fixture ordering repair; production supervisor
behavior and contract expectations remain unchanged. The existing registered test
keeps its name/location. The separate incomplete-delivery failures and their
behavioral mutation remain intact; no registry or normative coverage changes.

Validation commands:

- `cargo nextest run -p cyoa-infrastructure --test process_supervisor --profile workspace --locked --offline`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`
- `bash scripts/check_contracts.sh`

The focused suite passed all 30 tests, and formatting/Clippy passed. The full gate
passed all 337 runtime tests and seven compile-fail doctests, detected all 37
handwritten behavioral mutations, and passed the 176-mutant domain sweep (98
caught, 78 compile-invalid; two caught cases were per-test nontermination).
These are local results, not a successful GitHub rerun; no push or workflow rerun
was performed.
