# Architecture decision records

These records adapt decisions already made in this project into the engineering
skills' ADR format. Their numbers identify documents; existing contract IDs retain
their meaning and test registrations.

Read [the glossary](../../GLOSSARY.md) for vocabulary. Read relevant ADRs for
rationale, then their linked contracts for precise rules, coverage status and
remaining obligations. [Project contracts](../decisions/README.md) remain normative;
the [required-test registry](../decisions/required-tests.json) remains unchanged.
An accepted decision can still have pending implementation. These ADRs add no new
behavioral decisions or verification claims.

| ADR | Decision | Existing contracts |
| --- | --- | --- |
| [0001](0001-contracts-govern-source-parity.md) | Project contracts govern source parity | All contract IDs |
| [0002](0002-inward-dependencies-and-use-cases.md) | Inward dependencies and lightweight CQRS | ARCH-001 |
| [0003](0003-domain-invariants-at-boundaries.md) | Domain invariants and separate boundary representations | ARCH-002, STATE-001, TEXT-001 |
| [0004](0004-coequal-subscription-backends.md) | Co-equal stateless subscription backends | BACKENDS-001 |
| [0005](0005-backend-local-compatibility.md) | Backend-local compatibility adaptation | ARCH-003 |
| [0006](0006-character-identity-and-namesakes.md) | Stable identity and preservation of namesakes | IDENTITY-001, IDENTITY-002, PROMPTS-001 |
| [0007](0007-unicode-matching-policy.md) | Unicode lowercase and readable generated IDs | TEXT-002 |
| [0008](0008-turn-log-and-summary-memory.md) | Bounded memory, turn history and chapter retitling | CHAPTER-001, ACCEPTANCE-001 |
| [0009](0009-explicit-restoration-limits.md) | Explicit restoration limits | LIMITS-001 |
| [0010](0010-templates-and-guarded-overrides.md) | Native schemas and guarded template overrides | PROMPTS-002, PROMPTS-003 |
| [0011](0011-canonical-state-and-tentative-previews.md) | Canonical state and tentative previews | STREAM-001, PRODUCT-001 |
| [0012](0012-atomic-saves-and-reconciliation.md) | Atomic saves, conflict detection and reconciliation | PRODUCT-001, TEXT-001, ARCH-001 |
| [0013](0013-supervised-storage-and-autosave.md) | Supervised storage and autosave coordination | PRODUCT-001, STREAM-001, ARCH-001 |
| [0014](0014-independent-test-evidence.md) | Independent expectations and domain mutation scrutiny | TESTING-001, ACCEPTANCE-001, TOOLING-001 |

Build order and product scope remain in [PLAN.md](../../PLAN.md#build-order).
Backend observations remain in the [Claude](../../reference/01-claude-cli.md) and
[Codex](../../reference/02-codex-cli.md) references. Dated implementation evidence
remains in [decision history](../decisions/history.md) and the linked reviews.
