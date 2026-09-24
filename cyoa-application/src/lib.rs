//! Application orchestration and inward-owned ports.
//!
//! Commands will express state-changing intent; queries will expose read-only
//! views. Use cases depend on the domain and ports, never concrete adapters or
//! presentation types. Game use cases arrive with the domain implementation.

pub mod cancellation;
pub mod image;

pub mod generation;
