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

Nothing has been implemented yet — see `README.md`'s "Status".
