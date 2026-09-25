//! Diagnostic evidence owned by the application port, separate from story state.

/// Raw transport stdout/stderr, retained separately from the structured
/// payload for troubleshooting (see `docs/decisions/README.md`'s TEXT-001).
///
/// Byte-backed, not `String`-backed: a subprocess's diagnostic streams may
/// contain invalid UTF-8, and lossy display must never replace the retained
/// bytes. Kept as a hand-written type rather than an instance of
/// `define_verbatim_string_type!` because that macro is `String`-backed and a
/// single-purpose byte macro would be premature abstraction for one type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TransportDiagnostics {
    stdout: CapturedBytes,
    stderr: CapturedBytes,
}

impl TransportDiagnostics {
    pub fn new(stdout: impl Into<Vec<u8>>, stderr: impl Into<Vec<u8>>) -> Self {
        Self {
            stdout: CapturedBytes::complete(stdout),
            stderr: CapturedBytes::complete(stderr),
        }
    }

    pub fn from_captures(stdout: CapturedBytes, stderr: CapturedBytes) -> Self {
        Self { stdout, stderr }
    }

    pub fn stdout_capture(&self) -> &CapturedBytes {
        &self.stdout
    }
    pub fn stderr_capture(&self) -> &CapturedBytes {
        &self.stderr
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn stdout(&self) -> &[u8] {
        &self.stdout.bytes
    }

    pub fn stderr(&self) -> &[u8] {
        &self.stderr.bytes
    }

    pub fn stdout_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self.stdout())
    }

    pub fn stderr_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self.stderr())
    }

    pub fn is_empty(&self) -> bool {
        self.stdout().is_empty() && self.stderr().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_diagnostics_preserve_non_utf8_bytes_exactly_and_lossy_display_never_panics() {
        let invalid_utf8 = vec![b'e', b'r', b'r', 0xff, 0xfe, b'\r', b'\n'];
        let diagnostics = TransportDiagnostics::new(b"out\r\n".to_vec(), invalid_utf8.clone());
        assert_eq!(diagnostics.stdout(), b"out\r\n");
        assert_eq!(diagnostics.stderr(), invalid_utf8.as_slice());
        // `as_bytes` (via `stderr`) is the source of truth; `to_string_lossy`
        // must not silently become the retained value.
        assert_ne!(diagnostics.stderr_lossy().as_bytes(), diagnostics.stderr());
        assert!(!diagnostics.is_empty());
        assert!(TransportDiagnostics::empty().is_empty());
    }
}

/// Whether EOF was observed without losing bytes. A prefix does not claim how
/// many bytes are missing, nor that a killed producer finished its intended output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureCompleteness {
    #[default]
    Complete,
    Prefix,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapturedBytes {
    bytes: Vec<u8>,
    completeness: CaptureCompleteness,
}

impl CapturedBytes {
    pub fn complete(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            completeness: CaptureCompleteness::Complete,
        }
    }
    pub fn prefix(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            completeness: CaptureCompleteness::Prefix,
        }
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn completeness(&self) -> CaptureCompleteness {
        self.completeness
    }
}
