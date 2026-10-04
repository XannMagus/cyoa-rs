# Phase 2 S7: fault acceptance and persistence mutations

Phase 2 is complete through S7 on the exercised Linux platform. This record audits the remainder of Phase 2
against the stable A1–A12 requirements in the implementation plan, rather than
using the preceding S1–S6 completion claims as proof of Phase 2 completion.

## Acceptance audit

| Requirement | Current executable evidence |
|---|---|
| A1 complete story and exact records | `infrastructure/tests/persistence_roundtrip.rs`: actual application generation with malformed response, cancellation and explicit retries; independent frozen full/zero-turn fixtures; exact audit/provenance/trace values. The full story now also creates actual files, reloads, rewinds, replaces, reloads and continues with exact next-request comparison and exact prior-byte backups. |
| A2 checked/lossless reconstruction | Core `restored_world` and infrastructure `save_validation`: established cast independence, checked positions/IDs, metadata consistency, strict known-field types, required fields, duplicate keys, bounded bytes/depth, lossless normalization rejection. `filesystem_repository` rejects unsafe files, links and ownership/type violations. |
| A3 repeated restore choices | `persistence_limits` and `filesystem_repository::disk_restore_cycles_preserve_original_limits_and_every_narrowed_snapshot`: original/current typed limits, every authoritative snapshot, no resurrected events or pruned cast, rewind/continuation and exact captured prompt/schema. CLI policy tests exercise both choices under both fixture backends. |
| A4 version/extension compatibility | `save_compatibility`, frozen v1 minimal/full/additive fixtures and private migration tests: future/missing versions fail, omission/null/empty meanings remain distinct, optional extras warn and disappear on resave, reads do not rewrite, unknown styles preserve their keys and use existing fallback. No invented historical migration support. |
| A5 atomic repository/path/conflict policy | `filesystem_repository` and private repository tests: create/replace with exact backup bytes, independent-writer conflicts, nonblocking stable locks, private permissions/no-follow/hardlink/type checks, bounded listing, hostile titles and stable IDs; deterministic collision, entropy, clock and revision exhaustion checks. |
| A6 write/crash/recovery failures | Private repository tests inject preparation/read/encode/write/flush/file sync/backup persist/directory sync/primary persist/cleanup failures and verify exact visibility/files/retry material. Real barrier-child kills cover first-create and replacement crash windows, lock release and idempotent reconciliation. Storage transport tests reject lost/malformed/capped/multiple replies. Explicit binary backup recovery now covers corrupt, future-version and missing primaries and preserves both original states. |
| A7 canonical autosave and storage-only retry | Presentation `persistence` tests: accepted selection/turn/rewind/policy changes, deferred opening exactly once, preserved Dirty/Uncertain game, no inference/recommit during retry, save-copy binds only on success. New mutations target rejected-result autosave, false Clean and inference retry. |
| A8 stale/busy/load races | Private coordinator stale/wrong-revision/duplicate receipt tests and counter reservations; presentation session stale-generation test; failed load preserves stage/revision/binding/retry/edit, successful load advances counters; busy/dirty admission and quit precedence. Independent repository writers and uncertain reconciliation cannot overwrite another writer. |
| A9 public commands/restarts | Presentation `commands` and CLI `headless_persistence`: invalid syntax/IDs/policies, auth-free list/inspect, temporary roots, explicit zero-turn opening, durable rewind, save-copy, edits and both backend restart paths. Real fixture children capture the next prompt and prove one continuation call. |
| A10 shutdown/output/fault matrix | CLI `headless`, `headless_output`, `persistence_shutdown`, `storage_helper`, presentation worker/storage/coordinator tests. Quit/EOF/SIGINT/input errors join real generation children; their final disk state is now asserted. Broken stdout retains only accepted authoritative payloads. Real full pipes retain exact output and absolute drain bounds. New accepted-turn test combines silent mutating helpers with broken/full stdout and stderr, retains the fifth passage in memory and exact previous primary bytes, bounds current+final attempts and verifies every preparation/write PID is gone. Repeated termination input cannot extend deadlines; worker panic remains an exit failure even after a clean final save. |
| A11 demo and backend source | CLI `demo_persistence`, `demo_retry`, `headless_persistence`: all five fixed passages survive restart, consumed-result cancellation/retry, rewind and exhaustion; unsupported scenario/version/source/count rejects before inference/auth. Live saves resume symmetrically under the peer backend and preserve prior records/provenance. |
| A12 shared gate/status/tooling | Required-test registry retains prior assertions and adds the new sink test. Handwritten manifest now requires all 42 mutations; the five new entries require exact assertion messages. Existing pinned workspace/domain profiles, doctests, architecture checks and mutation outcome validation remain mandatory. No Python or runtime credential dependency was added. Final gate/fmt/Clippy results will be recorded below. |

## Persistent mutations

Each mutation runs only in an isolated source copy. The shared runner requires a
passing baseline, a failing exact registered test with its specific assertion,
then a passing restored test. Compiler failures, stale patches, unrelated setup
failures, missing tests and survivors are failures of the gate.

| Mutation | Exact protected assertion |
|---|---|
| `accept-stale-storage-receipt` | Stale storage receipt must never clean or rebind canonical state. |
| `autosave-unaccepted-generation` | Unaccepted generation must never autosave. |
| `rotate-corrupt-primary` | Corrupt primary must never rotate over the good backup. |
| `mark-post-rename-failure-clean` | Post-replacement failure must never be marked clean. |
| `retry-generation-on-save-failure` | Save failure must never retry inference. |

The first patch validation correctly failed on context-free diffs; these were
fixed without changing gate policy. An initial runtime-layer rejected-result
mutation survived because that test's cancellation can return an error before a
successful result queues. It was not allowlisted: the persisted mutation targets
the coordinator's erroneous autosave of a failed/cancelled completion and must
fail its explicit no-write assertion. This is a focused protection, not universal
mutation coverage of infrastructure.

The new sink harness initially broke the generation status output too early;
after correcting admission and handshake ordering it exercises an accepted
passage before the storage/output faults. A transient empty PID file was a
harness read race; an append-only PID log now verifies every helper after join.
Harness timeouts are not behavioral mutation detections.

## Evidence limits and subsequent work

The exercised platform is Linux with actual local files, process termination and
sync ordering. Process kills do not simulate physical power loss or establish
durability on arbitrary filesystems. Only save version one is supported. No new
live model/backend compatibility claim is made: both fixture adapters are peers
and the preexisting live records retain their own scope. TUI, exports, character
editing, Calibre import, images and arbitrary prompt overrides remain subsequent
work; Phase 2 introduces none of them.

## Final verification and completion audit — 2026-10-05

The objective is exactly `implement the remainder of phase 2`. At baseline
`dcde06e`, S1–S6 were committed; the remaining S7 row requires the full A1–A12
audit, composed fault acceptance, five focused persistent mutations and truthful
completion documentation. The table above maps every acceptance group to its
actual executing tests, including the strengthened disk continuations, recovery
matrix and combined sink/storage faults. None of these requirements remains
pending; the evidence limits above are scoped limits, not omitted Phase 2 work.

`bash scripts/check_contracts.sh` passed on the final runtime/test tree:
428 runtime tests, seven doctest/compile-fail examples, all 42 handwritten
mutations and 179 domain mutants (100 caught, 79 unviable). Two caught domain
mutants fail by per-test nontermination, separately reported from assertions;
unviable mutants are not test detections. Required-test presence/coverage and
inward dependency checks passed in that same gate. The final full log is
`/tmp/phase2-s7-reviewed-gate.log`; [gate-evidence.txt](gate-evidence.txt) retains
its summary and each new mutation's detection. [mutation-evidence.txt](mutation-evidence.txt)
retains each focused mutation's passing baseline, exact failing assertion and
passing restored run from the separately verified isolated runner.

Workspace formatting, Clippy across all targets with warnings denied, the
separately included repository test source's rustfmt check, shell syntax and
`git diff --check` passed. Existing regression expectations, domain exclusions,
backend protocols/shared schemas/prompts and both pinned mutation profiles were
preserved. The current documentation/registry now enforces LIMITS-001's complete
restore requirement and leaves PRODUCT-001 partial for the later TUI/export work.
The next build step is Phase 3's TUI play screen over an existing save.
