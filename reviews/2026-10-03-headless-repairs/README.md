# Headless review repairs — 2026-10-03

Astra identified two defects: blocking final error reporting on a full stderr
pipe, and demo replay consuming the next response even when cancellation prevented
canonical acceptance. Both repairs affect STREAM-001 and PRODUCT-001.

The stderr hang is unlikely with an ordinary terminal, but reproducible with an
open pipe whose reader stops draining. Final error reporting now scopes Linux
nonblocking flags around its best-effort write after normal terminal cleanup.
The binary regression fills a pipe, keeps its reader open and requires exit 1
within two seconds; it explicitly kills and reaps a stuck child on failure.

The demo now chooses responses by generation stage and canonical committed turn
count. It still uses the generation engine, bundled templates and wire validation.
The composed regression uses a handshake after real demo consumption, cancels
before acceptance and checks unchanged state. It retries outline, cast and all
five passages, alternates retry with changed player direction, then checks stable
exhaustion. No timing assumption decides when cancellation happens.

Both regressions failed before repair and passed afterward; retained red/green
logs record this. The demo constructor changed with the typed generator API; the
behavioral assertions were preserved. Clippy passed with warnings denied.

The contract gate retains its existing mutations and adds
`block-final-error-report` and `advance-demo-on-cancel` (37 total). Mutation testing
here deliberately changes production code to recreate a defect and requires the
registered regression to fail on its behavioral assertion. Compilation failures,
stale patches, missing tests and timeouts fail the gate itself; they do not count
as successful detection.

These are offline repair checks. Earlier live Codex evidence remains dated and
separate; Claude's independent live headless gate remains pending.

`bash scripts/check_contracts.sh` passed: required-test registration, architecture
checks, workspace tests and all 37 behavioral mutations. See `contracts.log`.
`cargo fmt --all --check` and
`cargo clippy --workspace --all-targets --locked --offline -- -D warnings` passed.
