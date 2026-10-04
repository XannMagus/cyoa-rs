# Phase 2 S3: local atomic repository

Implements the blocking Linux repository, not yet exposed to headless play.
Affected contracts: PRODUCT-001, TEXT-001, ARCH-001 and LIMITS-001.

The inward GameRepository port is implemented by LocalRepository. Directory-relative
no-follow opens validate ownership, type and link count; a stable nonblocking lock
and exact-byte optimistic stamp protect slot updates. Names are checked ASCII IDs
with OS-random suffixes; collisions have eight attempts, titles remain Unicode.
Missing-root list does not create directories. New directories/files use 0700/0600
and creation syncs parents. ProjectDirs resolves the default; relative explicit/XDG
paths fail. Assets are reserved, never created. Linux local filesystems are the
exercised scope; no physical power-loss or uninterruptible-I/O guarantee is claimed.

New file and exact previous-byte backup are separately flushed/synced; backup
replacement and directory sync precede primary replacement. NamedTempFile persistence
uses an owned directory fd path. Corrupt/future primary cannot replace a healthy
backup. Success requires final directory sync; post-replacement errors retain
Replaced and a prepared-write token. Reconciliation of exact intended bytes syncs
without rotating backup or creating duplicate slots; changed bytes conflict.

Six integration tests verify real files, independent writers, stable lock inodes,
symlink/hardlink/nonregular/root permissions, paged corrupt/future/backup-only rows,
recovery into a new slot, and repeated disk restore/narrow/save/original/rewind cycles.
Four private filesystem fault tests exercise every write/flush/sync/persist/recheck/
cleanup checkpoint, first-create uncertainty, ordering, constant clocks, collisions,
entropy failure, overflow and preserved initiating/cleanup causes. API-red test run
is /tmp/phase2-s3-red.log. An initial fixture-ID mistake was corrected before acceptance;
it was a harness failure, not evidence of an implementation regression.

S4 adds process supervision and helper crash-window acceptance; S5–S7 remain pending.

Completion: formatting, workspace/all-target Clippy with warnings denied and
`git diff --check` passed. The full contract gate `/tmp/phase2-s3-gate.log`
passed 379 runtime tests, seven compile-fail doctests, all 37 handwritten
mutations and 179 domain mutants (100 caught, 79 unviable). Two caught cases
were per-test nontermination, reported separately. No mutation patches/profiles/
exclusions or historical assertions were weakened. No credentials/model calls.
