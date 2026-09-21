# Agent instructions

Read `PLAN.md` in full before doing anything else in this repo. It is the
architecture, the reasoning behind every non-obvious decision, the build order,
and a ranked risk list — not a summary to skim.

Two things in there apply directly to you as an implementing agent, regardless
of which coding agent you are:

1. **"Backend parity and cross-agent handoff"** (in `PLAN.md`, right before the
   two backend sections). This project has two co-equal inference backends —
   `claude -p` (Claude Code) and `codex exec` (OpenAI Codex CLI) — and is
   implemented across sessions that each have only one of them authenticated.
   Verify the backend matching your own CLI against real, live calls; scaffold
   the other from its published docs and mark it unverified. Don't treat either
   backend as the primary one.
2. Each backend's status lives in its own file, `reference/01-claude-cli.md` or
   `reference/02-codex-cli.md` — a "Confirmed" section (only things actually run
   and observed) and an "Open questions" section (documented but not exercised).
   If you can resolve an open question with a real authenticated run this
   session, do, and move it into "Confirmed" with the evidence. Never mark
   something confirmed without having actually run it.

After `PLAN.md`, read `reference/00-engine-notes.md` for the ported game logic,
and whichever `reference/0N-*-cli.md` matches the CLI you have available.

Follow `PLAN.md`'s "Rust domain modeling requirements": preserve Python behavior
using idiomatic Rust, prefer domain types over bare primitives, and make invalid
states unrepresentable where practical. Semantic wrappers/aliases are acceptable;
use newtypes for compile-time distinctions, checked construction for invariants,
and enums or marker types/typestate for legal states and transitions. Aliases alone
do not enforce distinctions. Keep boundary validation and serde behavior faithful
to the port.

Follow `PLAN.md`'s "Clean Architecture, DDD, and lightweight CQRS" decisions.
Dependencies point inward: presentation calls application use cases; application
orchestrates the domain through inward-owned ports; infrastructure implements
those ports; `main.rs` wires concrete implementations. Application code must not
depend on concrete presentation/infrastructure types. Separate vendor/save DTOs
from domain types where constraints differ. Use commands for state-changing intent
and queries for read-only views, without requiring a message bus, separate databases,
or event sourcing. These boundaries supersede the original module placement and
"one layer of types" guidance in the plan.

The Rust workspace scaffold is in place; gameplay is not implemented yet — see
`README.md`'s "Status".

Avoid Python for project tooling. Prefer shell scripts for small checks or Rust
utilities for larger tools. The copied Python under `reference/calibre/` is source
reference for the port, not a tooling dependency.
