# Claude adapter step 3 — isolated request preparation

Baseline `87d2e3b`. Adds `backends::claude_cli` (configuration + fixed-profile
request preparation) and `backend_compat::claude_cli::ADVISOR_SUPPRESSION`.
Nothing launches a child, decodes a record or implements `Backend`. Affected
decisions: ARCH-001, ARCH-003, TEXT-001, PROMPTS-002, BACKENDS-001 (statuses stay
partial). Shared wire/schema/prompt/template code is untouched; `adapt_schema` and
its regression are untouched.

## What exists

- `ClaudeExecutable` (wraps the step-2 shared resolver), `ClaudeModel` (nonblank, no
  NUL, no leading `-`), `ClaudeInvocationConfig` (absolute HOME, optional absolute
  `CLAUDE_CONFIG_DIR`, selected PATH, optional model, finite bounds: 180 s, 4 MiB
  stdout, 256 KiB stderr, each justified by the step-1 live timings/sizes).
- `PreparedClaudeRequest` owns a `ProcessSpec`: argv exactly per the frozen profile
  (`-p --safe-mode --tools "" --permission-prompts none --no-session-persistence
  [--model M] --system-prompt … --append-system-prompt <advisor suppression>
  --json-schema <compact adapted schema> --output-format stream-json --verbose
  --include-partial-messages`), env limited to HOME/PATH (+`CLAUDE_CONFIG_DIR`), the
  prompt as stdin bytes, an empty request workspace as cwd.
- Argv guard: any single argument of ≥131072 bytes (Linux `MAX_ARG_STRLEN`, NUL
  included) or containing NUL is a typed `Argument{name, problem}` preparation
  error raised before the workspace exists. The prompt is stdin and unaffected.

## TDD record (error → edge → nominal)

The types were first written without validation or the argv guard, then eight tests
were run. Observed runtime reds: `invalid_model_home_config_dir_and_search_path_values_are_rejected`
(a blank model was accepted), `nul_and_oversize_arguments_fail_preparation_before_any_launch`
(no error for an oversize/NUL argument) and the exact-argv test. The exact-argv red
was **my test's mistake, not missing production code**: I had hand-written the
compact schema with the shared schema's key order, but `adapt_schema`'s
`Map::remove` is a swap-remove under `preserve_order`, so dropping `$schema` moves
the last root key (`$defs`) to the front. That reordering is harmless (the
2026-10-02 live calls ran with schemas built the same way; `properties` order is
unaffected) and `adapt_schema` is pinned by its regression and a mutation patch, so
the test now asserts the real contract instead: the argument is the canonical
compact serialization of the adapter's output, and `narrative` stays the first
`properties` key (new test). Then the validation and guard were implemented and all
nine tests pass. Five tests passed on first run (model insertion, advisor locality,
schema = adaptation, no-launch, one-rendered-request): recorded as passing at
introduction, not manufactured failures.

## Registry

Nine tests registered once each, appended to the last existing `cyoa-infrastructure`
lib check of: ARCH-001 (config validation, no launch), ARCH-003 (schema adaptation
isolation, advisor locality), TEXT-001 (exact stdin/argv bytes), PROMPTS-002 (one
rendered request; narrative first), BACKENDS-001 (argv guard, model placement; a new
lib check, as the contract had none).

## Verification and limits

fmt, warnings-denied Clippy and `bash scripts/check_contracts.sh` (all eighteen
mutations; `adapt-peer-schema.patch` still applies) pass: `contracts.log`. Not
claimed: that `--model` works (never run), that any argument near the limit works
live, or any codec/Backend behavior. `--system-prompt-file` exists but is not used.
