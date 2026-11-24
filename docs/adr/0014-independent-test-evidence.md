# Use independent contract expectations and domain mutation scrutiny

Python parity cannot validate deliberate deviations, and passing regressions alone
can conceal weak assertions. Required tests therefore use project expectations,
with handwritten mutations across the harness and automated mutations confined to
the domain and its own tests; assertion failures, nontermination and compile-invalid
mutations remain distinct evidence. The shared implementation gate uses pinned
Nextest for runtime tests and Cargo for doctests, while project tooling uses shell
or Rust and copied Python remains source reference only.

Contracts: [TESTING-001](../decisions/README.md#testing-001-automated-mutation-scrutiny-of-the-domain-suite),
[ACCEPTANCE-001](../decisions/README.md#acceptance-001--exercise-the-complete-engine-before-adding-external-io),
[TOOLING-001](../decisions/README.md#tooling-001-no-python-tooling-dependency).
Limits and reviewed exclusions: [domain mutations](../testing/domain-mutations.md).
