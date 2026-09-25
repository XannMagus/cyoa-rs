//! Vendor-neutral process supervisor (Phase 1 item 3).
//!
//! Launches a program via an explicit argv (never `sh -c`/joined shell
//! text), writes an explicit stdin payload and closes it, drains stdout and
//! stderr concurrently, and observes cancellation even while every pipe is
//! silent. Delivers stdout split on newlines as opaque byte records to a
//! caller-supplied consumer — this module must never inspect Claude/Codex
//! event names; that interpretation belongs to `backends::claude_cli`/
//! `codex_cli` (Phase 1 items 4/5), built on top of this supervisor.
//!
//! Unix-only. `run` on a non-Unix target returns
//! [`SupervisorError::Unsupported`] without attempting anything platform
//! specific; this module makes no portability claim beyond that.

use cyoa_core::text::TransportDiagnostics;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

/// Explicit, caller-constructed environment for a launched process. Never
/// silently inherits the calling process's full environment via
/// `Command::envs(std::env::vars())` — this session's own discovery (item 1,
/// `reference/01-claude-cli.md`'s advisor section) is that ambient
/// `CLAUDE_CODE_*`/`CLAUDECODE` variables change vendor CLI behavior
/// unexpectedly. Every variable the child receives must be named here.
#[derive(Debug, Clone, Default)]
pub struct EnvPolicy {
    vars: Vec<(OsString, OsString)>,
}

impl EnvPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consumes and returns self, per this project's consuming-transform
    /// convention.
    pub fn set(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.vars.push((key.into(), value.into()));
        self
    }

    pub fn vars(&self) -> &[(OsString, OsString)] {
        &self.vars
    }
}

/// A checked-nonzero byte bound for captured stdout. A distinct type from
/// [`MaxStderrBytes`] so the two cannot be swapped at a `ProcessBounds::new`
/// call site without a compile error — both are otherwise bare `usize`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxStdoutBytes(std::num::NonZeroUsize);

impl MaxStdoutBytes {
    pub fn new(bytes: usize) -> Result<Self, InvalidProcessBounds> {
        std::num::NonZeroUsize::new(bytes)
            .map(Self)
            .ok_or(InvalidProcessBounds::ZeroBound)
    }

    fn get(self) -> usize {
        self.0.get()
    }
}

/// See [`MaxStdoutBytes`]; the stderr counterpart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxStderrBytes(std::num::NonZeroUsize);

impl MaxStderrBytes {
    pub fn new(bytes: usize) -> Result<Self, InvalidProcessBounds> {
        std::num::NonZeroUsize::new(bytes)
            .map(Self)
            .ok_or(InvalidProcessBounds::ZeroBound)
    }

    fn get(self) -> usize {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidProcessBounds {
    #[error("a process bound must be nonzero")]
    ZeroBound,
    #[error("the deadline must be nonzero")]
    ZeroDeadline,
}

/// Checked, finite resource bounds. These are transport resource settings
/// (never gameplay `Limits`), so there is no unchecked/infinite constructor:
/// every bound is an explicit, checked-nonzero value the caller picked.
#[derive(Debug, Clone, Copy)]
pub struct ProcessBounds {
    deadline: Duration,
    max_stdout_bytes: MaxStdoutBytes,
    max_stderr_bytes: MaxStderrBytes,
}

impl ProcessBounds {
    pub fn new(
        deadline: Duration,
        max_stdout_bytes: MaxStdoutBytes,
        max_stderr_bytes: MaxStderrBytes,
    ) -> Result<Self, InvalidProcessBounds> {
        if deadline.is_zero() {
            return Err(InvalidProcessBounds::ZeroDeadline);
        }
        Ok(Self {
            deadline,
            max_stdout_bytes,
            max_stderr_bytes,
        })
    }

    /// A generous default, derived from this session's own live evidence
    /// (`reviews/2026-09-25-claude-cli-refresh/`): advisor-suppressed calls
    /// measured 29-31s, and the advisor-inclusive baseline was 60.9s. This
    /// triples the slower observed baseline (180s) to absorb vendor tail
    /// latency without being effectively unbounded.
    ///
    /// The byte-size bounds are NOT evidence-derived — no probe has measured
    /// response size distributions — and are a deliberately generous,
    /// explicitly-labeled guess (64 MiB stdout / 16 MiB stderr) chosen only
    /// to bound memory. Tests must pass explicit, finite bounds of their own
    /// rather than relying on this default.
    pub fn generous_default() -> Self {
        Self {
            deadline: Duration::from_secs(180),
            max_stdout_bytes: MaxStdoutBytes::new(64 * 1024 * 1024).expect("nonzero literal"),
            max_stderr_bytes: MaxStderrBytes::new(16 * 1024 * 1024).expect("nonzero literal"),
        }
    }

    pub fn deadline(&self) -> Duration {
        self.deadline
    }

    pub fn max_stdout_bytes(&self) -> usize {
        self.max_stdout_bytes.get()
    }

    pub fn max_stderr_bytes(&self) -> usize {
        self.max_stderr_bytes.get()
    }
}

/// One generation call's launch parameters. `stdin` is written in full, then
/// the write end is closed (or delivery is abandoned on a broken pipe — see
/// [`run`]'s module-level stdin policy note).
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: EnvPolicy,
    pub stdin: Vec<u8>,
    pub bounds: ProcessBounds,
}

/// Which captured stream exceeded its configured bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// Explicit process lifecycle, per the plan's "spawned -> stopping or
/// exited -> reaped" requirement — not `is_running`/`has_result` booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Spawned,
    Stopping,
    Exited,
    Reaped,
}

/// A successful transport outcome: the process exited zero, no protocol
/// consumer rejected a record, and cancellation was never observed.
#[derive(Debug, Clone)]
pub struct ProcessOutcome {
    pub exit_code: i32,
    pub diagnostics: TransportDiagnostics,
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("failed to launch process: {0}")]
    Spawn(std::io::Error),
    #[error("process supervision is not implemented on this platform")]
    Unsupported,
    #[error("generation cancelled")]
    Cancelled { diagnostics: TransportDiagnostics },
    #[error("process exceeded its deadline")]
    Timeout { diagnostics: TransportDiagnostics },
    #[error("process exited with status {exit_code}")]
    NonzeroExit {
        exit_code: i32,
        diagnostics: TransportDiagnostics,
    },
    #[error("{stream:?} output exceeded its configured bound")]
    OutputBoundExceeded {
        stream: OutputStream,
        diagnostics: TransportDiagnostics,
    },
    #[error("record consumer rejected a record: {reason}")]
    ConsumerRejected {
        reason: String,
        diagnostics: TransportDiagnostics,
    },
}

impl SupervisorError {
    /// No pipes were ever opened for `Spawn`/`Unsupported` (the process
    /// never launched), so those variants report empty diagnostics rather
    /// than borrowing a value that doesn't exist.
    pub fn diagnostics(&self) -> TransportDiagnostics {
        match self {
            SupervisorError::Spawn(_) | SupervisorError::Unsupported => {
                TransportDiagnostics::empty()
            }
            SupervisorError::Cancelled { diagnostics }
            | SupervisorError::Timeout { diagnostics }
            | SupervisorError::NonzeroExit { diagnostics, .. }
            | SupervisorError::OutputBoundExceeded { diagnostics, .. }
            | SupervisorError::ConsumerRejected { diagnostics, .. } => diagnostics.clone(),
        }
    }
}

/// Splits complete newline-terminated records off the front of `buf`,
/// leaving any trailing partial record in place. Pure, vendor-blind byte
/// framing: `\n` is the only delimiter; a preceding `\r` (CRLF framing)
/// stays attached to its record rather than being stripped, since this
/// layer must not assume vendor content shape.
fn split_records(buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    let mut start = 0;
    while let Some(offset) = buf[start..].iter().position(|&b| b == b'\n') {
        let end = start + offset;
        records.push(buf[start..end].to_vec());
        start = end + 1;
    }
    buf.drain(0..start);
    records
}

#[cfg(test)]
mod split_record_tests {
    use super::split_records;

    #[test]
    fn empty_buffer_yields_no_records_and_stays_empty() {
        let mut buf = Vec::new();
        assert_eq!(split_records(&mut buf), Vec::<Vec<u8>>::new());
        assert!(buf.is_empty());
    }

    #[test]
    fn a_single_complete_record_is_extracted_and_removed() {
        let mut buf = b"hello\n".to_vec();
        assert_eq!(split_records(&mut buf), vec![b"hello".to_vec()]);
        assert!(buf.is_empty());
    }

    #[test]
    fn a_trailing_partial_record_without_newline_remains_buffered() {
        let mut buf = b"one\ntwo".to_vec();
        assert_eq!(split_records(&mut buf), vec![b"one".to_vec()]);
        assert_eq!(buf, b"two".to_vec());
    }

    #[test]
    fn crlf_records_keep_the_carriage_return_attached_to_the_record() {
        let mut buf = b"one\r\ntwo\r\n".to_vec();
        assert_eq!(
            split_records(&mut buf),
            vec![b"one\r".to_vec(), b"two\r".to_vec()]
        );
        assert!(buf.is_empty());
    }

    #[test]
    fn an_empty_record_between_two_newlines_is_preserved_as_empty() {
        let mut buf = b"a\n\nb\n".to_vec();
        assert_eq!(
            split_records(&mut buf),
            vec![b"a".to_vec(), Vec::new(), b"b".to_vec()]
        );
    }

    #[test]
    fn feeding_bytes_across_multiple_calls_reassembles_a_split_record() {
        let mut buf = b"hel".to_vec();
        assert_eq!(split_records(&mut buf), Vec::<Vec<u8>>::new());
        buf.extend_from_slice(b"lo\n");
        assert_eq!(split_records(&mut buf), vec![b"hello".to_vec()]);
    }
}

#[cfg(unix)]
mod unix_impl;
#[cfg(unix)]
pub use unix_impl::run;

#[cfg(not(unix))]
/// Records the requested record consumer's type without ever compiling
/// platform-specific code: `run` always returns [`SupervisorError::Unsupported`].
pub fn run(
    _spec: &ProcessSpec,
    _cancel: &cyoa_application::cancellation::CancellationToken,
    _on_record: &mut dyn FnMut(&[u8]) -> Result<(), String>,
) -> Result<ProcessOutcome, SupervisorError> {
    Err(SupervisorError::Unsupported)
}
