# Automated domain mutation gate — 2026-10-03

Explicit user decision: independently assess the domain's own tests using standard
Rust automated mutation tooling; retain the handwritten mutations for the wider
harness. Affected contracts: TESTING-001 (new), TOOLING-001, ARCH-002,
IDENTITY-001/002, TEXT-001/002, STATE-001, LIMITS-001 and CHAPTER-001.
No domain production behavior or existing regression expectations changed.

## Observed initial sweep

Pinned cargo-mutants 27.1.0 generated 178 mutations with no exclusions. Using
`cargo test`, four jobs, a 20-second test-run timeout and only `cyoa-core` tests:

| Outcome | Count |
|---|---:|
| Test-detected | 81 |
| Survived | 17 |
| Did not compile (unviable) | 78 |
| Mutation-run timeout | 2 |

The sweep took 34 seconds and exited 3, not success. Inventory, outcome metadata
(including timestamps, commands and diffs) and summary log are retained here.
Fifteen survivors exposed unasserted chapter ordinals, selected styles/style changes,
scene descriptions, valid cost values, input/provenance/prompt-trace retention and
NPC relationships. Existing fixtures commonly left optional fields absent.
Two survivors were behavior-equivalent: nonempty-cast `is_empty` returning false,
and a quick-action fill branch entering at full capacity then immediately breaking.
Their reasoning is recorded in `docs/testing/domain-mutations.md` and beside the
exact exclusion patterns. No behavior-changing survivor is allowlisted.

## Strengthened sweep

Five new domain-only tests exercise independent expected values and unchanged game
history through public domain operations. No application or adapter test is used.
The suite now has 60 runtime tests; existing compile-fail docs remain in the normal
workspace gate (Nextest does not run doctests).

With cargo-nextest 0.9.132, explicit ten-second per-test deadlines, four jobs and
the two reviewed equivalent exclusions, the sweep tested 176 mutations in 33 seconds:

| Outcome | Count |
|---|---:|
| Test failures without timeout | 96 |
| Per-test nontermination failures | 2 |
| Survived | 0 |
| Did not compile (unviable) | 78 |
| Outer mutation-run timeout | 0 |

Cargo-mutants reports 98 caught: the two nontermination detections are included,
not two additional kills. They change the prose-context decrement to division and
the unique-ID suffix increment to multiplication. Dedicated Nextest logs retain
which actual domain tests timed out. Baseline tests completed normally. This is
bounded failure of a mutated domain algorithm, distinct from an outer watchdog
interrupting a runner. The handwritten mutation gate's assertion policy is unchanged.

The shipped wrapper additionally checks tool versions and successful builds before
Nextest test failure (exit 100), rejecting empty/no-test/invocation results. It runs
a full-domain sweep and forbids a survivor baseline. An initial wrapper
run with its default two jobs took 54 seconds and passed; final contract-gate
results are recorded below. CI installs the pinned tools and retains reports.

These results validate the generated mutation set, not all possible faulty code.
Macro-expanded string types and generated derives are not mutated; existing
constructor/compile-time tests and review remain necessary. No live backend claim
is made; the peer's live headless gate remains pending.

## Final contract gate

`bash scripts/check_contracts.sh` passed with the shipped config and wrapper:
required/ignored test checks, architecture, workspace tests and compile-fail docs,
all 37 handwritten mutations, and the full 176-mutation domain sweep. The domain
portion took 54 seconds with two jobs, with the same outcomes as above and an
explicit count of the two per-test nontermination failures. `contracts.log`,
`final-inventory.json` and `final-outcomes.json` retain this run.
`cargo fmt --all --check`, Bash syntax checks and workspace Clippy with warnings
denied also passed. CI wiring was inspected; no remote CI run is claimed.
