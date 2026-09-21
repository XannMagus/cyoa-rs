# cyoa-rs

A standalone Rust TUI port of calibre's "Create your own adventure" (CYOA)
game — an LLM-driven choose-your-own-adventure engine — using `claude -p`
(Claude Code headless mode) as the inference backend, so generation rides a
Claude Code **subscription** instead of metered Anthropic API credits.

This repo is a **seed**: research, verified design decisions, and ported
prompt/schema content, prepared in one session so implementation can start
cold in a later session, on a different machine, without needing calibre
checked out.

## Status

Nothing has been implemented yet. This is the starting point.

## Where to start

1. Read `PLAN.md` in full. It has the architecture, the reasoning behind
   every non-obvious decision, the build order, and a ranked list of the
   risky parts. Follow the phased build order in there — Phase 0 (engine, no
   I/O) through Phase 1 (a real, playable headless loop against the actual
   model) is the fastest way to find out if anything about the design is
   wrong, before any TUI code exists.
2. `reference/00-engine-notes.md` — the game logic (schemas, the delta-merge
   rules, validation, chapter/rewind logic) in dense reference form.
3. `reference/01-claude-cli.md` — the exact `claude -p` flags verified to
   work, including a documented trap (`--bare`) that looks correct and would
   silently break the whole point of the project.
4. `reference/prompts.toml`, `reference/styles.toml`, `reference/schema_docs.toml`
   — every prompt string, style-table entry, and schema field description,
   copied verbatim from calibre. **Read the warning comment at the top of
   `prompts.toml` before using it** — it's the honest source text, not a
   ready-to-load config; it still contains JSON-formatting instructions from
   calibre's provider-agnostic prompt path that must be stripped for
   `--json-schema` use (see `01-claude-cli.md`'s "Gotcha" section for why).
5. `reference/calibre/` — **temporary** copies of the three calibre Python
   files this is ported from (`cyoa.py`, `structured.py`, `epub.py`). Kept
   around so you can check an edge case against the original without cloning
   calibre. Delete this directory once the port is complete and no longer
   needed for reference — the licensing position (see below) does not change
   when you do.

## Prerequisites

- Rust (stable toolchain)
- The `claude` CLI installed and logged into a Claude Code subscription
  (`claude auth`) — verify with `claude --version` and a manual `claude -p`
  call before assuming the backend works
- No calibre installation or checkout required

## License

**GPL-3.0** — see `LICENSE` and `NOTICE.md`. This is a derivative of GPLv3
code (calibre's CYOA feature, © Kovid Goyal), not a choice made freely for
this project; see `NOTICE.md` for why, and PLAN.md's "Licensing" section if
you're ever tempted to relicense it.
