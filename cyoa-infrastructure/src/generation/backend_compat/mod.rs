//! Per-backend tolerance adapters. Each backend gets its own file here and
//! starts from the shared, maximal output of `generation::{wire,schema,
//! prompts}`, removing or adjusting only what its own CLI actually can't
//! handle. Nothing in a backend's file may change what those generic
//! modules produce, and no backend's adapter may depend on another's.
//! Building a new backend means adding a new file (and one `pub mod` line
//! below to register it) — never editing an existing backend's file or the
//! generic `generation` modules themselves. See `docs/decisions/README.md`'s
//! `ARCH-003`.
//!
//! **Pending, not a priority right now:** with only two backends, one
//! `pub mod` line per backend is simpler than it's worth to generalize.
//! If a third backend ever joins, revisit whether a dynamic
//! registration/plugin mechanism (discover and (de)activate adapters at
//! runtime) earns its complexity, rather than building it speculatively now.

pub mod claude_cli;
