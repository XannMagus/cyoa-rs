# Step 2 handoff: Codex schema and invocation preparation

> **Historical record (2026-10-05).** This slice is complete. Current status lives in
> [PLAN.md](../../PLAN.md) and [the contracts](../decisions/README.md); statements below
> describe the state when this document was written.

Prepared against `70a669c` (2026-09-26); clean tree at preparation start.
**Planning only. Step 2 is not implemented.** This expands step 2 of
[the adapter plan](phase1-codex-adapter.md) without extending its scope.
Read AGENTS.md, PLAN.md and their required decisions/registry/references first.
The [frozen profile](../../reference/02-codex-cli.md) and
[step 1 evidence](../../reviews/2026-09-26-codex-profile/README.md) govern vendor behavior.
Check for later commits before starting implementation.

## Deliverable and ownership

Produce a checked, owned request ready for the existing process supervisor, with
its isolated workspace, adapted schema file, fixed argv, explicit environment,
exact stdin and finite bounds. Preparation must not launch a model process.
One coherent implementation commit includes error/edge/nominal tests, registry
updates, observed TDD trace and truthful status updates.

Expected file ownership (names may vary, boundaries may not):

| File | Responsibility |
|---|---|
| `cyoa-infrastructure/src/generation/backend_compat/codex_cli.rs` | Consuming, fallible schema transformation; exact vendor-local stdin framing |
| `generation/backend_compat/mod.rs` | One module registration |
| `cyoa-infrastructure/src/backends/codex_cli.rs` or its private preparation module | Checked executable/model settings and prepared request ownership |
| `backends/mod.rs` | One module registration |
| `cyoa-infrastructure/tests/codex_preparation.rs` or private unit tests | Credential-free preparation and boundary checks |
| `docs/decisions/required-tests.json` | Register real new tests under applicable existing contracts |

Reuse ProcessSpec, RequestWorkspace, EnvPolicy and ProcessBounds. Do not build
another runner or change global cwd. Do not implement Backend::generate, parse
events, reconcile process outcomes, change the failure API, expose gameplay or
launch new live probes in this step. No shared wire/schema/template or Claude
adapter changes are needed. Keep source changes narrow; no generic adapter framework.

## Schema contract

Implement a consuming `Result`-returning transformation with a location-bearing
preparation error. Start from an owned copy of the shared schema.

1. Require every property of each object in property insertion order. Do not add
   nullability or remove defaults. Existing outline/cast/zero-NPC schemas should
   remain semantically identical; only turn schemas currently need changes.
2. For the observed `$ref` plus description shape, move the reference into a
   single-branch `anyOf` and retain the description on the outer node. Example:
   `{"$ref":"#/$defs/QuickActionKind","description":"..."}` becomes
   `{"description":"...","anyOf":[{"$ref":"#/$defs/QuickActionKind"}]}`.
   Do not overwrite an existing anyOf or silently drop other ref siblings.
   Unhandled combinations must fail explicitly, not trigger invented adaptations.
3. Walk schema-bearing children: property/definition maps, items and supported
   union branches. Explicitly enumerate the supported subset and validate child
   shapes. Boolean additionalProperties is valid. Do not dereference references
   recursively or walk arbitrary annotations, defaults, examples or enum values
   as though their objects were schemas. Unsupported structural forms should
   identify the exact location without claiming all JSON Schema is supported.
4. Preserve root `$schema`, `$defs`/`$ref`, property order (narrative first), all
   descriptions, existing nullable unions and additionalProperties constraints.
   Idempotent application must not nest another anyOf or change output again.

Use original schemas in `reviews/2026-09-26-codex-profile/requests/` as inputs and
reviewed successful discovery copies in `opening_ref_union` / `continuation_ref_union`
as independent golden expectations. Compare JSON semantically where insignificant
formatting differs, and assert property ordering separately. Never regenerate
expected outputs using the production transformation under test.

## Invocation contract

Use the exact fixed flags in reference/02-codex-cli.md, including
`-c project_doc_max_bytes=0`, `--output-schema schema.json` and positional `-`.
Prepare schema.json inside the owning RequestWorkspace and keep it alive until
ownership transfers to the supervisor. Drop/preparation failures must release the
workspace. No caller-supplied arbitrary argv/config escape hatch.

Resolve executable paths before the request cwd changes. Cover absolute paths,
relative paths containing directory components, and bare names resolved against
an explicitly selected PATH. Reject blank/NUL/invalid configuration with useful
errors; OS launch checks remain authoritative later (no promise against filesystem
races). Use named types when executable/model values would otherwise be confused.
Keep optional model selection separate from reported model provenance: step 1
used no explicit model, so any prepared override is **unverified**, never reported
as an observed model. Do not broaden the frozen live claim by writing an argv test.

Frame the unmodified instructions and prompt in JSON fields using the exact
recorded preface in the discovery recorder. Serialize strings; never interpolate
player input as template code or shell text. Keep both large strings on stdin.
This is one combined user message, not a privileged instruction channel.

Construct environment from named values only: HOME, PATH and optional CODEX_HOME,
matching step 1. Tests must inject deterministic values rather than mutate the
process-global environment. Exclude OPENAI_API_KEY, CODEX_API_KEY and ambient
configuration overrides. Auth status must eventually be checked using these same
home/config selections; **request preparation is not an auth check**. Actual
preflight execution and generation sequencing belong to adapter wiring, not a
fabricated verified-auth boolean in this step. Keep full isolation unverified:
AGENTS.md suppression is observed, while tools can still execute reads.

## Error → edge → nominal test sequence

Write the test, observe a meaningful runtime failure, implement the minimum, then
refine. Compiler errors are setup failures, not red TDD evidence. Record tests that
pass immediately as existing/expanded coverage rather than manufacturing failures.

| Order | Required checks |
|---|---|
| Error | Malformed/unsupported schema nodes with locations; annotated-ref combination that would overwrite a union; invalid executable/model settings; schema file preparation failure; no model child launched |
| Edge | Schema-like data inside defaults/examples untouched; unannotated refs; idempotence; nullable chapter title and null versus [] threads; defaulted strings stay nonnullable; required but empty relationships/scene text remain valid; zero NPCs; unknown action kind retains existing wire recovery |
| Edge | Quotes, newlines, CRLF, Unicode and template-like player text round-trip exactly; large prompts stay off argv; relative executable remains resolved outside request cwd; environment excludes injected metered/auth override variables; workspace released on drop/failure |
| Nominal | All three bundled schema kinds and zero-NPC variant match reviewed adaptations; exact argv/stdin/schema-file contents; file remains visible while owned; prepared result can transfer ownership into ProcessSpec without spawning |
| Parity | Original shared schemas and existing Claude adaptation remain unchanged; narrative-first and annotation preservation assertions are independent of implementation |

Where existing wire regressions already prove domain tolerance, retain and run them;
add focused composition checks for adapted outgoing schemas rather than duplicating
the whole domain suite. There is no network/credential/vendor-binary requirement.
Use controlled temporary executable fixtures to test resolution.

## Completion gate and status

Register preparation regressions under ARCH-003 and relevant TEXT-001/PROMPTS-002
or ARCH-002 contracts. Add only tests that exist. BACKENDS-001 remains pending:
a preparation helper is not a concrete Backend. No fabricated live coverage.
Name all affected decision IDs in the report, including ARCH-001/003, TEXT-001 and
PROMPTS-002, and retain PROMPTS-003's public-override restriction.

Run formatting, workspace Clippy with warnings denied, and
`bash scripts/check_contracts.sh` (retain all eleven existing mutations). If a new
focused mutation naturally belongs here, follow the adapter plan's mutation policy;
never count stale patches/compiler failures as detection. A later review should
inspect the real schema diff and ownership/API surface, not only green tests.

Stop after step 2. Step 3 is the private event state machine; step 4 reconciles it
with real process outcomes. Neither belongs in this commit.

## Ready-to-use implementation request

> Read AGENTS.md and PLAN.md in full, then their required decisions, test registry
> and references. Check current changes since 70a669c. Implement only step 2 of
> docs/plans/phase1-codex-adapter.md using docs/plans/phase1-codex-step2-handoff.md.
> Preserve the frozen protocol and backend parity. Use error/edge/nominal TDD,
> backend-local transformations, independent golden expectations and checked owned
> preparation. Keep process launching, codec implementation and Backend execution out of scope.
> Run all required gates and report the diff and evidence for review before commit.
