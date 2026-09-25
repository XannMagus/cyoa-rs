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
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl TransportDiagnostics {
    pub fn new(stdout: impl Into<Vec<u8>>, stderr: impl Into<Vec<u8>>) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    pub fn stdout_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }

    pub fn stderr_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.stderr)
    }

    pub fn is_empty(&self) -> bool {
        self.stdout.is_empty() && self.stderr.is_empty()
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
