//! Adapters that translate between calibre-shaped model wire contracts and
//! `cyoa-core` domain types. `wire.rs` owns the request/response DTOs and
//! their domain mapping; `schema.rs` builds the JSON Schemas sent to a
//! backend; `prompts.rs` renders the instructions/prompt text sent alongside
//! them. `engine.rs` (a later slice) is the only consumer that orchestrates
//! all three together with a live backend.

pub mod prompts;
pub mod schema;
pub mod wire;
