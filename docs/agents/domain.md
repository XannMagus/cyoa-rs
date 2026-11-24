# Domain docs

## Reading order

Follow the existing AGENTS.md reading order first: PLAN.md,
docs/decisions/README.md and its required-test registry, then the
required engine and backend references.

For domain exploration, read root GLOSSARY.md for vocabulary and
docs/adr/README.md to select ADRs relevant to the work. Follow each ADR's
contract links for precise rules, coverage status and remaining obligations.

Explicit user decisions and project contracts retain their existing
precedence. Surface glossary or ADR conflicts with those contracts.

## Layout

This repo uses one domain context across its Rust workspace:

- GLOSSARY.md: shared domain vocabulary.
- docs/adr/: concise decision rationale and links to authoritative contracts.
- docs/decisions/: existing normative contracts and required regressions.

Create glossary entries and supplementary ADRs lazily through
domain-modeling when terms or decisions are resolved. If GLOSSARY.md
or docs/adr/ is absent, continue without suggesting scaffolding upfront.

## Vocabulary and decisions

Use glossary terms in issues, proposals, hypotheses and tests.
Record meaningful terminology gaps for domain-modeling.
Identify conflicting decision IDs explicitly before proposing changes.
Behavioral decisions follow the existing project-contract change procedure.
