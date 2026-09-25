//! Driven adapters for external systems and vendor protocols.
//!
//! Implement application ports here. Raw JSON and subprocess details stay behind
//! this boundary; neither the domain nor application depends on this crate.

pub mod backend;
pub mod backends;
pub mod generation;
pub mod image;
