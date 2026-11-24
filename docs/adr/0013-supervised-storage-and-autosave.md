# Supervise blocking storage and coordinate autosave with canonical state

Filesystem calls can block in ways a Rust thread cannot cancel, so storage runs
through bounded helper processes behind the inward repository port rather than on
the event loop or a detached writer. Accepted game changes trigger autosave; a save
failure preserves canonical state and blocks further game mutations until storage
is resolved, and storage retry never regenerates a turn. Shutdown joins generation
before bounded final saving, retaining prepared identity when a write's outcome is uncertain.

Application owns this policy; infrastructure owns worker/helper execution. Prepared
documents stay private to infrastructure behind an immutable application handle.
Embedding clients configure the helper executable explicitly; application completion
does not include a frontend's output drain.

Contracts: [ARCH-001](../decisions/README.md#arch-001-dependencies-point-inward),
[PRODUCT-001](../decisions/README.md#product-001-standalone-tui-persistence-export-and-image-boundary),
[STREAM-001](../decisions/README.md#stream-001-preview-text-never-authorizes-a-state-commit).
Rationale: [storage scheduling](../plans/phase2-persistence.md#scheduling-choice)
and [session transitions](../plans/phase2-persistence.md#admission-and-transitions).
