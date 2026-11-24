# Save atomically and reconcile uncertain writes

Direct primary-file writes risk partial saves, and locking alone permits sequential
writers to lose each other's updates. Saves therefore publish complete documents
by rename, retain a validated preceding primary as backup, and compare exact-byte
content stamps under a stable slot lock. Failures after replacement retain the
prepared attempt for reconciliation rather than blind retry; explicit backup
recovery creates a fresh slot, preserving damaged originals and the good backup.

Contracts: [PRODUCT-001](../decisions/README.md#product-001-standalone-tui-persistence-export-and-image-boundary),
[TEXT-001](../decisions/README.md#text-001-normalized-business-text-and-verbatim-audit-text),
[ARCH-001](../decisions/README.md#arch-001-dependencies-point-inward).
Rationale and failure matrix: [atomic replacement](../plans/phase2-persistence.md#5-atomic-replacement-backups-and-uncertain-outcomes).
Evidence scope: [Phase 2 acceptance](../../reviews/2026-10-05-persistence-s7/README.md);
process-kill tests do not establish power-loss durability on arbitrary filesystems.
