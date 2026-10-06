# Pre-Phase 3 cleanup — 2026-10-05

Branch `cleanup/pre-phase3`, from `e29dce1` (Phase 2 complete). This addresses every
finding from the 2026-10-05 repository review: architecture and layering, domain
modeling, claims against reality, and code-level correctness. Each change is an
atomic commit. Behaviour changes were made only on recorded user decisions (marked
**decision**); everything else preserves behaviour. Each handwritten mutation that
touched code was regenerated and confirmed detected: it compiles, fails with its
registered message, and passes once restored.

## Correctness fixes

| Finding | Resolution | Commits |
|---|---|---|
| Codex records and the shared payload kept the last of duplicate JSON keys (`turn.failed`/`turn.completed`) | **Decision**: strict parsing for Codex records (`DuplicateField`) and the shared `from_json`; new mutation `accept-duplicate-codex-control-key` | `6785f9d`, `57891a0` |
| `persist_noclobber`'s hard-link fallback could leave a two-link primary that never loads | Direct `RENAME_NOREPLACE`, falling back to a plain rename after the in-lock recheck | `84d29ec` |
| Orphaned `.cyoa-*` temps were never removed | Temps are tagged with their slot ID and swept under the slot lock | `211253b` |
| Save DTO defaults (`characters`, character `id`) could never succeed | **Decision**: required fields | `89ec39d` |
| Preview chunks after an overflow could leave a gap | An incomplete preview stops growing | `6577be1` |

## Architecture

- `PreparedWriteEvidence` is now part of the `GameRepository` signature, not
  constructor-injected; factories are `Fn() -> R` (**decision**, `fe894e8`).
- The architecture gate allowlists inner-layer crates and checks ARCH-003 placement
  mechanically (`49bfea6`; self dev-dependencies allowed in `35cee9a`).
- `main.rs` is composition only. Option types build commands, interruptible jobs
  name their operation, and backend configs own their environment and executable
  names (`f80c2ac`).
- Vendor-neutral transport plumbing is shared in `backends::transport`; protocols
  and auth predicates stay per backend (`4f5189b`).
- The fixture binaries build only for tests (`35cee9a`).
- Accepted and recorded rather than changed:
  - `&mut` repository queries, because both repositories own mutable resources;
  - `TransportDiagnostics` stays in application;
  - session policy stays in presentation, per the Phase 1 and 2 plans.

## Domain modeling

- `SaveSnapshot` is checked, and the demo length is a scenario fact (`69be487`).
- `take_turn` consumes the game and returns it, or hands it back in a
  `TurnFailure`; the rewind wrappers are removed (`e19b6e6`).
- Load rebinds limits by consuming the game instead of cloning it (`7941183`).
- `GenerationFailure`'s response is `Option<RawResponse>`, so there is no
  empty-string sentinel (`27421fa`).
- Persistence values are typed: `PageSize`, `SaveFormatVersion`, `SavedTurnCount`,
  `SaveSummary`, and an unambiguous listing status (`5c543aa`).
- `StorageFailure` has private fields; `replaced(pending)` derives its stamp
  (**decision**, `d35ac2a`).
- Error payloads, the image prompt and the `GameState` docs are typed or
  corrected (`155086a`). `PersistedSession`'s counter and save arguments are
  typed (`208667b`). Unrecognized fields are typed (`2ad6987`).
- Combining-mark slugs are recorded as a TEXT-002 deviation (**decision**,
  `b7113e1`).

## Tidying and performance

- Save-ID generation and the scoped duplicate-key check are shared (`217d452`).
- Save bytes are shared through `Arc`, helper replies are validated without a
  value tree, and transport diagnostics are moved instead of cloned (`5c40f36`).
- The helper wire protocol moved into its own module, and the headless `drive`
  and `line` were split into named steps (`8a99064`, `056914f`, `bf9c2ba`).
- Dead code, `unreachable_pub`, lock poisoning, the empty `XDG_DATA_HOME` and the
  quadratic preview copy (`6577be1`).
- Scheduler-sensitive latency assertions were replaced by logical-progress
  checks, and stuck tests now time out (`da8670a`).
- Deliberately not changed:
  - `needless_pass_by_value` hits on commands and freshly built values, which
    PLAN's consuming-transform rule prefers to pass by value;
  - three functions just over 100 lines, each a single linear transition guarded
    by mutations or byte-exact captures: `accept_storage`, `adapt_node` and
    repository `apply`.

## Documentation

- PLAN and README carry one current status, and next is Phase 3.
- Completed slice plans carry a historical banner.
- Dated evidence moved from `docs/decisions/README.md` to
  [history.md](../../docs/decisions/history.md).
- Every Partial contract states a `Remaining:` gap, and `check_contracts.sh`
  now enforces it.
- LIMITS-001 and PROMPTS-002 wording is corrected, the stray agent prompt is
  removed, and the Phase 2 A6 drift and the Claude group-kill question are
  annotated.

Not changed: tests registered under several decisions. Each registration states a
distinct contract the test protects, so the overlap is intended.

## Gate

The final `bash scripts/check_contracts.sh` run on this branch is recorded below.

The run passed (exit 0; [log](contracts.log)):

- Workspace tests pass. All 43 handwritten behavioral mutations are detected; each
  must compile, fail with its registered message and pass once restored.
- The `cyoa-core` sweep tested 181 mutants: 101 caught and 80 unviable, with no
  survivors.

The first full run on this branch failed, and that failure was genuine.
`disable-cancellation-wake` survived because the reworked self-pipe test's 10 s
bound exceeded its 5 s request deadline, so the deadline wake masked a broken
self-pipe. The fix is a separate commit that keeps the deadline beyond the bound.

## Merged: Astra's independent review repairs (2026-10-06)

Astra's [review](../2026-10-05-repository-review/README.md) found two boundary
bugs this cleanup had missed, and a third it shared. All five of Astra's
commits are cherry-picked here with authorship kept:

- **R1, Codex control keys:** this replaces the cleanup's whole-record strictness
  (**decision**: strict where vital, tolerant elsewhere, so minor CLI versions can
  evolve). Duplicate keys reject only within the record and its `item`, `error`
  and `usage` control objects. Other metadata is tolerated, and a new regression
  pins that tolerance. The payload stays strict. The check reuses the shared
  `json::duplicate_key_location` helper (which now reports the location) instead
  of a second visitor. Astra's `accept-codex-duplicate-control` mutation replaces
  `accept-duplicate-codex-control-key` and was regenerated against the merged
  code.
- **R2, Claude auth status:** a typed DTO rejects repeated known fields and
  tolerates unknown ones.
- **R3, input read-ahead:** the 64 KiB limit applies to each complete line, not
  to buffered read-ahead.

After the merge, `bash scripts/check_contracts.sh` passed (exit 0;
[log](contracts-merged.log)): 460 workspace tests, all 45 handwritten mutations
detected, and 181 domain mutants (101 caught, 80 unviable, no survivors).
Astra's live Codex 0.160.0 smoke evidence predates the merge and is not a live
run of the merged code.
