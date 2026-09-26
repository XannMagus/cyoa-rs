# Codex adapter step 2 review record

Implementation is complete and intentionally uncommitted for Astra review.
Scope is step 2 of `docs/plans/phase1-codex-adapter.md` only.

The original implementation report below is preserved as history. Subsequent
independent findings and user-authorized repairs are documented in
[review-and-repairs.md](review-and-repairs.md), with retained runtime red/green logs.

## Starting state

- Starting commit: `70a669c7e5a67e9ce4f4dc507f925d37775567fa`.
- No commits followed that starting commit when work began.
- No tracked changes existed. The pre-existing untracked file
  `docs/plans/phase1-codex-step2-handoff.md` was read and left untouched.
- The handoff says it was prepared against a clean tree; the actual tree differed
  only by that handoff file.

## Implementation and ownership

`generation::backend_compat::codex_cli` owns the consuming schema transformation
and the exact combined-stdin envelope. It starts from an owned shared schema,
requires all object properties in their insertion order, and wraps only the
observed `$ref` plus description form in a one-branch `anyOf`. It retains the
description outside the union, rejects other `$ref` siblings, keeps defaults,
nullability, references, root `$schema`, and additional-property constraints,
and does not scan arbitrary annotation/example/default values as schemas. The
walker lists its supported structural subset and returns location-bearing errors
for malformed shapes.

`backends::codex_cli` owns checked executable/model settings and creates a
`PreparedCodexRequest` around the existing `ProcessSpec`. It resolves absolute,
relative-with-directory, and bare executable names before assigning the request
cwd; accepts explicit HOME/PATH/optional CODEX_HOME values only; and constructs
the frozen fixed argv, schema file, stdin, and finite bounds. The optional model
is a distinct type, remains documented as unverified, and cannot begin with `-`.
The request owns its private `RequestWorkspace` through transfer to `ProcessSpec`.
Preparation contains no process launch or authentication check.

Files changed by this work:

- `cyoa-infrastructure/src/generation/backend_compat/codex_cli.rs`
- `cyoa-infrastructure/src/generation/backend_compat/mod.rs`
- `cyoa-infrastructure/src/backends/codex_cli.rs`
- `cyoa-infrastructure/src/backends/mod.rs`
- `docs/decisions/required-tests.json`
- `docs/plans/phase1-codex-adapter.md`
- `reviews/2026-09-26-codex-step2/README.md`

No shared wire/schema/template code or Claude adapter changed. The existing
handoff remains untracked and unchanged.

## Red/green evidence

- **Schema error, red then green:** before implementing the schema walker,
  `malformed_property_map_is_a_located_preparation_error` failed at runtime
  because the initial adapter returned the malformed object unchanged. The
  located validation was added and the test passed.
- **Schema edge and golden cases:** the first adapted-reference run exposed a
  real implementation defect: a `$ref` string had been passed to the recursive
  schema walker as if it were a schema node. Wrapping it as `{"$ref": ...}`
  fixed the shape, after which the schema regressions passed. An intermediate
  comparison against freshly rendered schemas differed from the committed
  discovery requests because those requests use a two-event description limit
  while current bundled defaults use thirty. The test now adapts the original
  committed request files and compares them with the separately reviewed
  discovery outputs; no golden was generated from production code. One initial
  assertion also addressed the wrong path for `upcoming_events`; its path was
  corrected to the referenced definition.
- **Malformed `required`, red then green:** the new test failed at runtime
  because a non-array `required` value passed through when no `properties` map
  existed. Validation now checks `required` independently and the test passes.
- **Invalid model option, red then green:** the new test failed at runtime
  because a value beginning with `-` was accepted as a model. The checked
  constructor now rejects option-prefixed model names; the focused test passes.
- **Invocation assertion correction:** an early expected argv placed optional
  `--model` after the sandbox flag, while the implementation inserts it after
  the Codex config override. The exact-argv assertion was aligned with that
  fixed ordering; this was an expectation mismatch, not a production failure.
- **TDD deviation:** the initial invocation preparation implementation was
  written before most invocation success-path tests were added. The later
  option-prefixed-model error case followed red/green, and the prepared-request
  assertions passed, but the entire invocation feature did not follow a strict
  test-first sequence. No tests or production behavior were weakened to conceal
  this ordering.
- **Setup/lint evidence:** new Rust tests compiled successfully; there were no
  compile failures counted as red evidence. The first warnings-denied Clippy run
  found one `collapsible_if` style lint in the schema walker. It was simplified,
  and the final workspace Clippy run passed.

The focused final suites pass: six `backends::codex_cli` tests and eight
`generation::backend_compat::codex_cli` tests. These are offline regressions,
not live Codex calls.

## Validation and decisions

- `cargo fmt --all --check` — passed.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` — passed.
- `bash scripts/check_contracts.sh` — passed. Workspace tests and architecture
  checks passed, and all eleven pre-existing mutations were detected. No mutation
  was removed or changed.
- No model call, auth preflight, process launch, event codec, `Backend::generate`,
  or full-isolation claim was made.

Affected decision IDs: **ARCH-001, ARCH-002, ARCH-003, TEXT-001, PROMPTS-002**.
BACKENDS-001 remains pending; PROMPTS-003 remains pending and still gates public
arbitrary prompt overrides. The test registry adds only implemented regressions
and does not promote either pending contract.

Remaining work belongs to later steps: Codex event parsing and outcome
reconciliation, executable/auth preflight sequencing, real `Backend::generate`,
and live acceptance. Full account/tool isolation and the optional model override
remain unverified.
