# Phase 2 S6: public headless persistence and resume

S6 implementation, focused tests and the final shared gate are complete.
S7 remains separate: this record does not claim complete Phase 2 fault acceptance
or the new focused persistence mutation suite. No new live model call was made.

The public binary supports global absolute `--data-dir`, credential-free list and
inspect, and startup `play --load ID --limits current|original [--backup]`.
Checked parsing rejects invalid IDs, missing/implicit restore policy, policy or
backup without load, relative data roots, invalid paging and zero/overflow rewind.
Startup reads and source validation precede vendor resolution/authentication;
failed auth cannot rewrite a policy-adjusted save. Both live backends remain peers.
Demo source pins harbour-v1, rejects other scenarios/excess turns and never falls
back to a vendor. The selected source is fixed for the session.

Headless drives the S5 coordinator for `/save`, `/save-copy`, `/list`, `/load`,
`/rewind` and quit, retaining canonical `/inspect` and independent diagnostics.
New selection saves before its one automatic opening. Loaded zero-turn games wait
for explicit continuation. Accepted policy changes persist after admission;
backup recovery creates and shows a fresh slot while preserving both originals.
Successful load clears lifecycle edits; failed load preserves the previous edit
buffer. Unknown optional save fields are displayed as resave warnings.

Output remains bounded/nonblocking. Shutdown joins both workers before the
absolute one-second output drain. Input continues during storage and cleanup;
quit/EOF/SIGINT are idempotent. A failing input/output sink cannot bypass the
canonical final save or turn a prior I/O failure into success. Silent helpers
use their existing total deadline, then at most one final reconciliation; actual
helper PIDs are checked absent. Selection admission additionally reserves both
session revisions and the opening request ID before changing canonical state.
Worker spawn errors remain latched for eventual exit status.
Output faults also latch in the queue: cleanup stops writing to that sink while
continuing the canonical save and pumping the healthy sink. The broken-output
regression asserts a single write attempt and exact saved canonical state.

## Acceptance scope audit

| Obligation | Executable evidence |
|---|---|
| Checked command surface / A9 invalid syntax | `presentation/tests/commands.rs`: public CLI and in-session syntax tests |
| Auth-free list/inspect, invalid load before auth | `headless_persistence::auth_free_list_inspect_and_invalid_resume_leave_save_bytes_unchanged` |
| Explicit limits; admission after auth; zero inference on zero-turn load | `startup_policy_changes_persist_after_auth_and_failed_auth_never_rewrites_a_save` runs both backend choices and both policies |
| Zero-turn continuation and durable rewind before another request | `zero_turn_resume_waits_for_explicit_opening_and_rewind_persists_before_next_request` |
| Explicit backup fresh-slot recovery preserving primary/backup | `explicit_backup_recovery_creates_a_fresh_slot_and_preserves_damaged_originals` |
| Draft edit retention, successful load clearing, save-copy binding and in-session listing | `in_session_load_keeps_failed_edit_buffers_and_save_copy_switches_slots_only_after_success` |
| Both fixture backend restarts / A9/A11 | `both_fixture_backends_restart_from_disk_and_live_saves_are_vendor_neutral`: save under each backend, resume under its peer, exact original turn/world retained, captured next prompt and one generation |
| Demo restart/rewind/exhaustion / A11 | `demo_persistence` binary tests and `demo_retry::cancelled_consumed_demo_passages_preserve_disk_and_retry_all_five_with_persistence` |
| Unknown/future/source/excess rejection | `demo_persistence::unknown_scenario_future_version_source_mismatch_and_excess_demo_turns_fail_before_auth` verifies unchanged bytes/no UI admission |
| Input responsiveness, bounded shutdown and no detached writer / A10 slice | `persistence_shutdown` tests: real silent helpers under quit/EOF/SIGINT/read-error, at most two dispatches, kernel PID disappearance; real successful final saves after input/broken-output errors |
| Existing story/cancellation/output behavior | Original 14 headless binary tests and six output regressions retain cleanup, call-count, ordering, deadline and story assertions |
| Test isolation | Every public play binary test supplies a temporary data root; direct headless compositions own temporary roots through their storage factory |

S1–S5 retain their domain/boundary/application/local-disk/helper/coordination
coverage. The new tests are registered under PRODUCT-001, STREAM-001,
BACKENDS-001 and ACCEPTANCE-001. LIMITS-001's S5 registrations retain the typed
nondefault policy/rewind checks; the new binary policy tests exercise current
(default runtime) versus original creation settings. Presentation depends only
inward; main wires the supervised repository. No vendor codec, shared schema or
prompt change. Existing test helpers moved to `cli/tests/support/headless.rs`
without moving any registered test or weakening a mutation assertion.

Observed development: the command acceptance test failed on unsupported `list`
before command implementation; invalid cases rejected first. Nominal command,
restart, policy, demo and shutdown tests then passed. A blocked-helper test initially
expected DrainingOutput after an ordinary successful output drain; corrected it to
Closed for normal exits, retaining DrainingOutput on an initiating input error.
This was an incorrect new harness expectation, not a contract behavior change.

## S5/S6 completion audit

The objective is exactly `implement S5 and S6`. The dependency-ordered rows in
`docs/plans/phase2-persistence.md` define its scope.

S5's canonical coordination is committed as `9281ff9`. Its registered port-based
and shipped-helper tests establish accepted-state autosave, deferred opening,
Dirty/Uncertain admission and storage-only retry, stale receipt rejection,
atomic load/rewind transitions, counter reservations, independent failures and
bounded final saving. Those tests remain in the current shared gate. S6 supplies
the public command/composition path, restart acceptance through both fixture
backends and demo, temporary data roots, retained output/cleanup checks and usage
documentation, mapped individually above. The selection capacity and output fault
checks added during S6 preserve those S5 guarantees in the public driver.

S7's complete A1–A12 fault matrix and new persistence-specific mutations are a
subsequent plan row, not part of this objective. This audit does not use existing
green tests to claim that later work or full Phase 2 completion.

## Final verification — 2026-10-04

`bash scripts/check_contracts.sh` passed on the final runtime/test tree:
427 workspace runtime tests, seven doctest/compile-fail examples, all 37
handwritten behavioral mutations and the domain-only sweep of 179 mutants
(100 caught, 79 unviable). Two caught mutants fail by per-test nontermination,
reported separately from assertion failures; unviable mutants are not detections.
The final log is `/tmp/phase2-s6-reviewed-gate.log`. Registry presence/coverage
and architecture checks passed within that same gate. No mutation patch was
removed or weakened.

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked
--offline -- -D warnings`, the separately included repository test source's
rustfmt check and `git diff --check` also passed. The public help lists play,
list, inspect and the global data directory option. This completes the S5/S6
objective; complete Phase 2 acceptance still requires S7.
