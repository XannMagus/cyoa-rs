# Retain turn history and bounded story memory

Generation uses the current story summary and bounded prose context rather than
the full transcript; summary updates avoid asking the model to re-emit and possibly
lose accumulated memory. Chapter position and rewind follow the accepted turn log,
while validated saved summary snapshots remain authoritative on restoration rather
than replaying old deltas. The last supplied title within a chapter is effective,
deliberately extending Calibre's first-turn-only title rule and allowing rewind to
restore the earlier title.

Rationale: [memory model](../../PLAN.md#the-memory-model-the-whole-point-of-the-design)
and [derived state](../../PLAN.md#derived-state-makes-rewind-trivial).
Contract: [CHAPTER-001](../decisions/README.md#chapter-001-later-turns-can-retitle-a-chapter).
Save authority: [validation and history](../plans/phase2-persistence.md#validation-and-history-authority).
