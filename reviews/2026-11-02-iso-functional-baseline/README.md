# Before-refactor regression baseline

Observed 2026-11-02, before implementation of the iso-functional cleanup.
Source baseline: `f433d3d23c07d86162e122419c3ade14950c2599`, with documentation
changes in the worktree. Source, Cargo configuration/lock, test registry and
scripts have no tracked diff. No new backend live calls were made.

Command: `bash scripts/check_contracts.sh`.
Result: exit **0**. Full retained [gate log](check-contracts.log).

- Pinned cargo-mutants 27.1.0 and cargo-nextest 0.9.132 checks passed.
- Registry/presence and architecture checks passed.
- Workspace Nextest: **460 tests passed, 0 skipped**.
- Cargo doctests/compile-fail examples: **7 passed**, no failures.
- **45 handwritten mutations detected**, with the gate's baseline/build/test
  outcome requirements intact.
- Automated domain-only sweep: **181 generated mutants tested; 101 caught,
  80 compiler-unviable, 0 missed, 0 outer mutation timeouts**. Two caught outcomes
  were per-test nontermination failures, not assertion failures; compiler-unviable
  cases are not counted as detected behavior regressions.

This establishes the current baseline, not proof that future code is equivalent.
Preserve assertions and rerun the same gate after implementation. New library-
client acceptance checks must additionally prove the architectural objective.
