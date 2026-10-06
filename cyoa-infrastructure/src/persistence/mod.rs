//! Save format, local atomic repository and supervised storage-helper I/O.
pub mod codec;
mod dto;
#[cfg(target_os = "linux")]
mod filesystem;
pub mod helper;
mod migrations;
pub mod repository;
mod save_id;
