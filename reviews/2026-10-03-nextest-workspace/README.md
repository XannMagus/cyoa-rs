# Nextest workspace runner — 2026-10-03

User goal: make Nextest the workspace runtime runner, retain doctests and both
mutation gates, verify the complete gate, and commit changes atomically.
Affected decision: TESTING-001. Existing PRODUCT-001/STREAM-001 backpressure
repairs are committed separately as `bef6097`; benchmark evidence is `8984377`.

## Change

`scripts/check_contracts.sh` checks pinned tools before its registry/architecture
checks, runs every workspace library/binary/integration runtime test through
Nextest's `workspace` profile, then runs all workspace doctests with Cargo.
The profile explicitly disables retries and fail-fast. No filter or ignored-test
exception was added. Test registration and ignored-test checks still use Cargo
listing. CI installs the same pinned tools and invokes the same shared gate.

Both mutation gates remain mandatory and in the same order. The 37 targeted
behavioral mutations keep their exact Cargo test invocations and strict outcome
parser. Automated mutation testing still mutates only `cyoa-core` and runs only
its own tests, with the unchanged `domain-mutations` profile, reviewed exclusions
and separate compiler-invalid/nontermination accounting. The version guard was
moved intact into `scripts/check_test_tools.sh`, shared by the complete gate and
standalone domain sweep. Versions remain cargo-nextest 0.9.132 and cargo-mutants
27.1.0. README, PLAN, AGENTS and testing/decision documentation reflect this split.

## Actual local validation

`CYOA_MUTATION_JOBS=4 bash scripts/check_contracts.sh` exited zero:

- Nextest: 337 runtime tests passed, zero skipped, with the `workspace` profile.
- Cargo: all seven doctests passed (one application, five core, one infrastructure;
  all are compile-fail examples).
- Registry presence/ignored-state, coverage declarations and architecture passed.
- All 37 targeted behavioral mutations detected after passing baselines/restores.
- Domain: 176 mutants, 98 caught, 78 compiler-invalid, zero survivors and outer
  timeouts. Two caught results are per-test nontermination; the other 96 are
  test failures. Compiler-invalid results are not test detections.

Bash syntax checks, fmt, denied-warning Clippy and diff whitespace checks passed.
CI wiring is inspected; this does not claim a remote CI run or live backend calls.

`contracts.log` retains the complete gate output and `domain-summary.json` retains
its automated mutation summary. Cargo and Nextest inventories each contain exactly
337 runtime cases; the sorted test-name multisets compare byte-for-byte equal
(`cargo-test-names.txt`, `nextest-test-names.txt`). Raw listings and inventory-build
output are retained. The named profile's retry/fail-fast settings are explicit in
`.config/nextest.toml`; the domain profile remains separate.

Run the complete gate with `bash scripts/check_contracts.sh`. For runtime tests
alone, use:

```sh
cargo nextest run --workspace --lib --bins --tests --profile workspace --locked --offline
```

Then run doctests separately with `cargo test --workspace --doc --locked --offline`.
The full gate remains dominated by mutation sweeps; the runtime benchmark does not
claim a threefold speedup for the complete gate.
