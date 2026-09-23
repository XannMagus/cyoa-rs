//! Vendor-independent CYOA domain.
//!
//! Game types, invariants, summary merging, chapters, and rewind belong here.
//! No application orchestration, serialization protocols, terminal handling, or
//! external adapters. Constructors establish invariants; merges return new values
//! without mutating the previous summary. Wire/save DTOs belong outside this crate.

pub mod character;
pub mod game;
pub mod ids;
pub mod limits;
pub mod style;
pub mod summary;
pub mod text;
pub mod turn;
pub mod world;
