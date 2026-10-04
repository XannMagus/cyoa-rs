//! Save-format boundary. Filesystem repository and supervised I/O follow later.
pub mod codec;
mod dto;
#[cfg(target_os = "linux")]
mod filesystem;
pub mod helper;
mod migrations;
pub mod repository;
