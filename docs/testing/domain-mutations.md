# Automated domain mutation testing

The user's 2026-10-03 decision prioritizes independent scrutiny of domain tests:
core invariants must not be accepted merely because a weak test passes. The
existing 37 targeted mutations remain useful for the wider contract harness.
Automated mutation generation is additionally required for the domain (TESTING-001).

## Running the gate

Install the pinned standalone Cargo tools; they are not runtime dependencies:

```sh
cargo install cargo-mutants --version 27.1.0 --locked
cargo install cargo-nextest --version 0.9.132 --locked
cargo fetch --locked
bash scripts/check_domain_mutations.sh
```

The domain sweep also runs in `bash scripts/check_contracts.sh`, locally and in CI.
CI installs the same tool versions and uploads `mutants.out` even after failure.
The suite needs no vendor credentials, live model calls or Python. Once Cargo's
cache is populated, builds/tests are locked and offline. The tools themselves
require a download on initial installation. Set `CYOA_MUTATION_JOBS` to change
concurrency (default 2); report durations depend on the machine and job count.

The wrapper invokes the standard `cargo-mutants` engine, not custom mutation
operators: it mutates only `cyoa-core` and runs only that crate's unit tests and
its domain-only integration tests. No application/infrastructure test can hide a
weak domain test. A clean baseline and a full sweep are mandatory. Survivors,
mutation-run/build timeouts, missing tools and incorrect tool versions fail the
check. Compile-invalid mutations are reported as **unviable**, never as test kills.
A small report check also requires actual nextest test failures (exit 100) after
successful builds, rejecting invocation errors and zero selected tests.

`cargo-nextest` is the runner, not the mutation generator. Each test has its own
process; the `domain-mutations` profile has no retries and fails a pure domain test
that has not finished after ten seconds. This catches mutation-induced infinite
loops without hanging the suite. These **per-test nontermination failures** are
reported by cargo-mutants as caught, but evidence distinguishes them from assertion
failures. The outer mutation-run deadline is at least 30 seconds; reaching it is
an inconclusive run and fails the gate. The handwritten mutation gate's strict
assertion/outcome policy is unchanged.

## Exclusions and interpretation

`.cargo/mutants.toml` includes every domain source file, with exactly two reviewed
behavior-equivalent exclusions:

- `CharacterCast::is_empty` replaced by `false`: the private collection is nonempty
  after construction; updates cannot remove entries. This replacement is correct
  for every publicly constructible cast. Replacing it by `true` is still tested.
- `<` replaced by `<=` in the quick-action fill branch: when three actions are
  already chosen, the first iteration immediately breaks at `>= 3`, without
  reading or adding an action. At other counts both versions behave identically.
  This exclusion pins the comparison's exact source location so a new comparison
  elsewhere in the function is not silently excluded. Moving it requires review
  and an updated exclusion; a stale exclusion leaves the mutant in the sweep.

Do not exclude a surviving mutation just to obtain a green score. Inspect it,
strengthen the domain tests when behavior differs, and document a precise proof
when behavior does not differ. Review these two exclusions when the underlying
invariants/algorithm change. Tool upgrades require a full sweep and renewed review
of exclusions and report semantics.

A passing sweep means every **generated, compilable, non-excluded** mutation was
detected. It is not a proof against all faulty implementations. This tool does not
expand our string declaration macros or mutate compiler-generated derives;
nonblank constructors, exact-text preservation and compile-time ownership still
need their registered tests and code review. Nor do its operator patterns cover
every possible faulty rewrite. These limits must remain visible; do not describe
the domain as mathematically "mutation-proof".

The initial sweep and strengthened results are retained in
[the evidence record](../../reviews/2026-10-03-domain-mutations/README.md).
