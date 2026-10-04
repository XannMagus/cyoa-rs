# Phase 2 S5: canonical persistence coordination

S5 is complete. S6 public commands and S7 full phase acceptance remain separate
work; the existing public headless entrypoint is still memory-only.

`PersistedSession` surrounds the existing generation runtime and owned storage
runner. Canonical selection, accepted turns and rewind trigger owned snapshot
writes; typed generation changes distinguish turns from outline/cast acceptance.
Generation failure/cancellation never writes rejected state. Storage failure keeps
the accepted game and its revision, blocks further generation, and remains
independent of generation diagnostics. Explicit save reconciles an uncertain
prepared attempt through a fresh worker after panic, without another inference
call. A successful create binds the slot; replacement updates its content stamp
and disk revision only for the current request and canonical revision.

New selections write zero-turn state before one deferred opening. Failed initial
save retains that intent; successful explicit retry runs it once, and quit clears
it. Primary load changes game and binding together through `replace_game`, which
advances the current revision and preserves generation request IDs. Restore-policy
changes and explicit backup recovery require a save before further play; backup
recovery creates a slot rather than replacing the recovery source. Source mismatch
and load errors preserve the old stage, revision, binding and generation retry.

Storage IDs never wrap; admission reserves a separate final-save ID and followup
writes for load/deferred opening. Shutdown cancels/joins generation first, allows
an active write to satisfy the final save, cancels read-only work, and makes at most
one final write/reconciliation attempt. Output draining is a separate terminal
transition for S6 to drive after both workers finish.

Evidence: port-only `cyoa-presentation/tests/persistence.rs`, private counter/stale
receipt tests, the session load/stale-generation regression, and
`cyoa-cli/tests/storage_helper.rs`'s coordinator composition through shipped helper
processes and actual primary/backup bytes. Decision registrations are PRODUCT-001,
STREAM-001 and LIMITS-001. Architecture remains inward; no backend schema, prompt,
authentication or protocol change and no new live model evidence.

Validation: focused presentation tests and all five shipped-helper tests;
workspace Clippy with warnings denied; workspace formatting and the separately
included repository test source formatting. The final shared contract gate passed
413 runtime tests, seven doctests/compile-fail examples, all 37 handwritten
mutations and 179 domain mutants (100 caught, 79 unviable). Two caught domain
mutants fail by per-test nontermination, separately reported from assertions.
The final gate output is `/tmp/phase2-s5-final-gate.log`; no mutation patch needed
refreshing. Review added regressions retaining an earlier uncertain attempt after
failed save-copy and latching a queued generation panic even when quit wins first.
