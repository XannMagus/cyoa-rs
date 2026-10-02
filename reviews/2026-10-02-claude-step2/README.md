# Claude adapter step 2 — vendor-neutral executable resolution

Baseline `6df40dd`. Behavior-preserving refactor so the Claude adapter can reuse
executable/PATH resolution without copying it or depending on Codex's file.
Affected decisions: ARCH-001 (no new dependency edge), ARCH-003 (adapters stay
separate; only vendor-blind mechanics are shared). No contract's expected
behavior changed; the registry is untouched.

## What moved

`cyoa-infrastructure/src/backends/executable.rs` now owns `ResolvedExecutable`
(resolution against a selected PATH and absolute base directory, canonicalization,
executable-by-current-user check), `ExecutableError` and the blank/NUL/absolute
value validators. `CodexExecutable` wraps `ResolvedExecutable` with the same public
API; `ConfigurationError` is unchanged, with a `From<ExecutableError>` that maps
variant for variant, so every Codex message ("invalid Codex …", "Codex executable
… was not found …") is byte-identical. Codex's model-specific rule (no leading `-`)
stays in `codex_cli.rs`.

## TDD record

This is a refactor: no red step was manufactured. The existing Codex tests
(`invalid_model_home_and_search_path_values_are_rejected`,
`executable_search_skips_nonexecutables_and_resolves_empty_path_entries`,
`executable_resolution_handles_absolute_relative_and_selected_path_names`, the
preparation tests) are the behavior oracle and pass unchanged under their registered
names. Three direct tests of the shared module were added (typed errors, canonical
PATH resolution, validators); they are new coverage of moved code, not registered.
No mutation patch touches `codex_cli.rs`, so none needed refreshing.

## Verification

`cargo fmt --all --check`, warnings-denied Clippy and `bash scripts/check_contracts.sh`
(including all eighteen mutations) pass; see `contracts.log`.
