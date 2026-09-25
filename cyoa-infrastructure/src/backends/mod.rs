//! Vendor-neutral process launch and lifecycle (Phase 1 item 3). Sibling
//! `claude_cli.rs`/`codex_cli.rs` modules (Phase 1 items 4/5) interpret each
//! vendor's own event shapes on top of this generic supervisor; this module
//! must never inspect Claude/Codex event names.

pub mod process;
