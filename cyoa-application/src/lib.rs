//! Application orchestration and inward-owned ports.
//!
//! The session owns canonical lifecycle, admission, acceptance and persistence
//! policy. Frontends submit commands and read borrowed views; execution adapters
//! own threads/subprocesses behind the contracts in `execution`.

pub mod cancellation;
extern crate self as cyoa_application;
pub mod diagnostics;
pub mod image;

pub mod attempt;
pub mod execution;
pub mod generation;
pub mod persistence;
pub mod runtime;
pub mod session;
pub mod session_persistence;
