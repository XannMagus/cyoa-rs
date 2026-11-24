# Use two co-equal stateless subscription CLI backends

The player has coding-agent subscriptions across machines and wants inference
through whichever subscription is available. Claude and Codex are therefore peers,
each invoked statelessly for generation, without a primary/fallback hierarchy or a
silent switch to metered API authentication. Each backend requires its own real
authenticated evidence; documented assumptions and offline fixtures do not verify
the other vendor, and list-price estimates do not represent subscription charges.

Contract: [BACKENDS-001](../decisions/README.md#backends-001-two-co-equal-subscription-cli-backends).
Rationale: [backend parity and handoff](../../PLAN.md#backend-parity-and-cross-agent-handoff).
Current observations: [Claude](../../reference/01-claude-cli.md),
[Codex](../../reference/02-codex-cli.md).
