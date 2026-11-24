# Preserve namesakes and use stable character identity

Calibre's name-based cast deduplication would erase distinct characters sharing a
name. This project removes only complete equal records, preserves role and order,
and repairs colliding IDs while reserving supplied IDs; subsequent updates retain
the established ID-first/name-fallback matching rule. The existing opening-turn
generation includes an identity-review instruction without an extra inference
call; it does not authorize semantic consolidation or deletion through prose.

Contracts: [IDENTITY-001](../decisions/README.md#identity-001-namesakes-and-exact-world-record-deduplication),
[IDENTITY-002](../decisions/README.md#identity-002-stable-ids-suffix-repair-and-ordering),
[PROMPTS-001](../decisions/README.md#prompts-001-opening-turn-identity-review).
