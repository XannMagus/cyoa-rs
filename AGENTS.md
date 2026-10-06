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
3. When a live run reveals a backend-specific tolerance limit (a flag it
   rejects, a shape it needs different), fix it in a small adapter scoped to
   that backend alone — never in the shared, backend-agnostic code path
   (wire DTOs, schema generation, prompt rendering). See `docs/decisions/README.md`'s
   `ARCH-003`. Building your backend must never require editing code the other
   backend's session already wrote for shared use.

After `PLAN.md`, read `docs/decisions/README.md` and its required-test registry.
Those project contracts override conflicting Calibre behavior, source notes, and
historical plan sketches. Never change a regression's expected behavior, delete it,
or ignore it just to match Python. Behavioral changes need an explicit user decision
recorded with rationale and updated tests; behavior-preserving test moves/renames
must update the registry without weakening assertions. Report affected decision IDs
and run `bash scripts/check_contracts.sh` before considering implementation complete.
Pending contracts must gain real tests when their features are implemented; do not
claim an unimplemented feature is enforced because a document mentions it.

Then read `reference/00-engine-notes.md` for the ported game logic,
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

The scripted Phase 0 engine, generation use cases, streaming and complete story
acceptance scenario are implemented. Both real subprocess adapters and Linux
headless/demo play are implemented; both Codex's and Claude's live headless gates
passed, completing Phase 1. Phase 2 persistence is implemented with its full
acceptance audit; see `reviews/2026-10-05-persistence-s7/README.md`. The pre-Phase 3
cleanup is in `reviews/2026-10-05-cleanup/README.md`; the handwritten behavioral
mutations are listed in `scripts/mutations/manifest.json`. TUI and export are subsequent work — see `README.md`'s
"Status". The contract gate also runs isolated behavioral
mutations; stale patches, compiler failures and missing tests must fail the gate,
not be counted as detected regressions. Keep domain, boundary, orchestration and
live-backend evidence distinct. Before starting the next phase, read PLAN.md's
current status and build order, and the `Remaining:` line of each Partial contract.

The gate also runs pinned cargo-mutants/cargo-nextest against `cyoa-core` and only
its own tests (TESTING-001). Install the documented tool versions before running
the gate; see `docs/testing/domain-mutations.md`. Never allowlist a behavior-changing
survivor. Review and justify behavior-equivalent exclusions; keep compile-invalid
mutations and per-test nontermination failures distinct from assertion failures.

Workspace runtime tests use pinned cargo-nextest with the `workspace` profile
(no retries, no fail-fast); Cargo still runs all doctests/compile-fail examples.
Keep both mutation gates and the separate `domain-mutations` profile intact.
Use `bash scripts/check_contracts.sh` for the shared local/CI completion gate.

Avoid Python for project tooling. Prefer shell scripts for small checks or Rust
utilities for larger tools. The copied Python under `reference/calibre/` is source
reference for the port, not a tooling dependency.
