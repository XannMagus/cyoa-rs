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
//! Linux-tested; enabled on Unix targets with rustix's WNOWAIT waitid API.
//! Other targets return [`SupervisorError::Unsupported`]. Other Unix targets
//! have no live verification claim. Each request owns its isolated cwd.

use cyoa_application::diagnostics::TransportDiagnostics;
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
/// the write end is closed. Known incomplete delivery cannot produce success.
#[derive(Debug)]
pub struct ProcessSpec {
    pub workspace: RequestWorkspace,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: EnvPolicy,
    pub stdin: Vec<u8>,
    pub bounds: ProcessBounds,
}

/// Owned scratch directory, including any schema files prepared by an adapter.
/// `run` consumes its request, keeping this directory alive through cleanup.
#[derive(Debug)]
pub struct RequestWorkspace(tempfile::TempDir);

impl RequestWorkspace {
    pub fn new() -> std::io::Result<Self> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("cyoa-request-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        builder.tempdir().map(Self)
    }
    pub fn path(&self) -> &std::path::Path {
        self.0.path()
    }
    fn close(self) -> std::io::Result<()> {
        self.0.close()
    }
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
    #[error("process I/O failed: {failure}")]
    Io {
        failure: IoFailure,
        diagnostics: TransportDiagnostics,
    },
    #[error("process cleanup failed: {failures:?}; initiating failure: {initial:?}")]
    Cleanup {
        initial: Option<Box<SupervisorError>>,
        failures: Vec<IoFailure>,
        diagnostics: TransportDiagnostics,
    },
    #[error("request delivery incomplete: wrote {written} of {expected} bytes")]
    IncompleteInput {
        written: usize,
        expected: usize,
        diagnostics: TransportDiagnostics,
    },
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
            | SupervisorError::Io { diagnostics, .. }
            | SupervisorError::Cleanup { diagnostics, .. }
            | SupervisorError::IncompleteInput { diagnostics, .. }
            | SupervisorError::Timeout { diagnostics }
            | SupervisorError::NonzeroExit { diagnostics, .. }
            | SupervisorError::OutputBoundExceeded { diagnostics, .. }
            | SupervisorError::ConsumerRejected { diagnostics, .. } => diagnostics.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoOperation {
    WorkspaceCleanup,
    Nonblocking,
    Read,
    Write,
    Poll,
    ObserveExit,
    KillGroup,
    Reap,
}

#[derive(Debug, thiserror::Error)]
#[error("{operation:?}: {source}")]
pub struct IoFailure {
    pub operation: IoOperation,
    #[source]
    pub source: std::io::Error,
}

/// Offsets into the owned stdout capture: framing never duplicates its buffer.
#[derive(Default)]
struct RecordFramer {
    start: usize,
    scanned: usize,
}

impl RecordFramer {
    fn records<'a>(&mut self, bytes: &'a [u8], final_record: bool) -> Vec<&'a [u8]> {
        let mut records = Vec::new();
        for (offset, byte) in bytes[self.scanned..].iter().enumerate() {
            if *byte == b'\n' {
                let end = self.scanned + offset;
                records.push(&bytes[self.start..end]);
                self.start = end + 1;
            }
        }
        self.scanned = bytes.len();
        if final_record && self.start < bytes.len() {
            records.push(&bytes[self.start..]);
            self.start = bytes.len();
        }
        records
    }
}

#[cfg(test)]
mod split_record_tests {
    use super::RecordFramer;
    #[test]
    fn empty_buffer_yields_no_records_and_stays_empty() {
        assert!(RecordFramer::default().records(b"", false).is_empty());
    }
    #[test]
    fn a_single_complete_record_is_extracted_and_removed() {
        let mut framer = RecordFramer::default();
        assert_eq!(framer.records(b"hello\n", false), vec![b"hello"]);
        assert!(framer.records(b"hello\n", true).is_empty());
    }
    #[test]
    fn a_trailing_partial_record_without_newline_remains_buffered() {
        let mut framer = RecordFramer::default();
        assert_eq!(framer.records(b"one\ntwo", false), vec![b"one"]);
        assert_eq!(framer.records(b"one\ntwo", true), vec![b"two"]);
    }
    #[test]
    fn crlf_records_keep_the_carriage_return_attached_to_the_record() {
        assert_eq!(
            RecordFramer::default().records(b"one\r\ntwo\r\n", false),
            vec![b"one\r", b"two\r"]
        );
    }
    #[test]
    fn an_empty_record_between_two_newlines_is_preserved_as_empty() {
        assert_eq!(
            RecordFramer::default().records(b"a\n\nb\n", false),
            vec![b"a".as_slice(), b"", b"b"]
        );
    }
    #[test]
    fn feeding_bytes_across_multiple_calls_reassembles_a_split_record() {
        let mut framer = RecordFramer::default();
        assert!(framer.records(b"hel", false).is_empty());
        assert_eq!(framer.records(b"hello\n", false), vec![b"hello"]);
    }
}

#[cfg(all(
    unix,
    not(any(
        target_os = "cygwin",
        target_os = "horizon",
        target_os = "openbsd",
        target_os = "redox"
    ))
))]
mod unix_impl;
#[cfg(all(
    unix,
    not(any(
        target_os = "cygwin",
        target_os = "horizon",
        target_os = "openbsd",
        target_os = "redox"
    ))
))]
pub use unix_impl::run;

#[cfg(not(all(
    unix,
    not(any(
        target_os = "cygwin",
        target_os = "horizon",
        target_os = "openbsd",
        target_os = "redox"
    ))
)))]
/// Records the requested record consumer's type without ever compiling
/// platform-specific code: `run` always returns [`SupervisorError::Unsupported`].
pub fn run(
    _spec: ProcessSpec,
    _cancel: &cyoa_application::cancellation::CancellationToken,
    _on_record: &mut dyn FnMut(&[u8]) -> Result<(), String>,
) -> Result<ProcessOutcome, SupervisorError> {
    Err(SupervisorError::Unsupported)
}
