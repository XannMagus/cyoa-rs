//! Adapters that translate between calibre-shaped model wire contracts and
//! `cyoa-core` domain types. `wire.rs` owns the request/response DTOs and
//! their domain mapping; `schema.rs` builds the JSON Schemas sent to a
//! backend; `prompts.rs` renders the instructions/prompt text sent alongside
//! them. All three are backend-agnostic and stay that way; `backend_compat`
//! holds each backend's own tolerance adjustments (`ARCH-003`). `engine.rs`
//! (a later slice) is the only consumer that orchestrates all of this with
//! a live backend.

pub mod backend_compat;
pub mod prompts;
pub mod schema;
pub mod wire;
