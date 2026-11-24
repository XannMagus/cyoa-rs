# Domain construction establishes invariants

Tolerant model output and saved documents have different constraints from trusted
game state, so boundary DTOs map into checked domain values rather than weakening
the domain's guarantees. Distinct concepts use distinct types, aggregates own
related values together (including world and protagonist selection), and optional
values represent legitimate absence. Business text is normalized while audit and
transport evidence retains its exact received content; this preserves diagnostics
without treating raw evidence as a valid turn.

Contracts: [ARCH-002](../decisions/README.md#arch-002-domain-types-establish-invariants),
[STATE-001](../decisions/README.md#state-001-selection-belongs-to-its-world),
[TEXT-001](../decisions/README.md#text-001-normalized-business-text-and-verbatim-audit-text).
Rationale: [Rust domain modeling](../../PLAN.md#rust-domain-modeling-requirements).
