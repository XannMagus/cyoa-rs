//! Normalized nonblank text and verbatim exchange text have distinct constructors.
//! Optional domain values use `Option<T>` rather than an empty-string sentinel.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("{} must not be blank", type_label(.kind))]
pub struct BlankText {
    pub(crate) kind: &'static str,
}

macro_rules! define_nonblank_string_type {
    ($name:ident) => {
        #[doc = concat!("Nonblank text for `", stringify!($name), "`.")]
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(text: impl AsRef<str>) -> Result<Self, $crate::text::BlankText> {
                let text = text.as_ref().trim();
                if text.is_empty() {
                    return Err($crate::text::BlankText {
                        kind: stringify!($name),
                    });
                }
                Ok(Self(text.to_owned()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}
pub(crate) use define_nonblank_string_type;

macro_rules! define_verbatim_string_type {
    ($name:ident) => {
        #[doc = concat!("Verbatim text for `", stringify!($name), "`; preserves whitespace and empty input.")]
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(text: impl Into<String>) -> Self {
                Self(text.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

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

define_nonblank_string_type!(CharacterName);
define_nonblank_string_type!(WorldDescription);
define_nonblank_string_type!(CurrentSituation);
define_nonblank_string_type!(EventText);
define_nonblank_string_type!(CharacterDescription);
define_nonblank_string_type!(Backstory);
define_nonblank_string_type!(Relationships);
define_nonblank_string_type!(CharacterSituation);

define_nonblank_string_type!(Brief);
define_nonblank_string_type!(WorldTitle);
define_nonblank_string_type!(PlayerInput);
define_nonblank_string_type!(Narrative);
define_nonblank_string_type!(QuickActionText);
define_nonblank_string_type!(SceneDescription);
define_nonblank_string_type!(ChapterTitle);
define_verbatim_string_type!(RawResponse);
define_nonblank_string_type!(ProviderName);
define_nonblank_string_type!(ModelName);
define_nonblank_string_type!(CurrencyCode);
define_verbatim_string_type!(Instructions);
define_verbatim_string_type!(RenderedPrompt);

// Distinct types so a pace key cannot be passed where a tone key is expected,
// even though all four are interchangeable strings at the storage level.
define_nonblank_string_type!(ArtStyleKey);
define_nonblank_string_type!(PaceKey);
define_nonblank_string_type!(ToneKey);
define_nonblank_string_type!(NarrationKey);

fn type_label(name: &str) -> String {
    let mut label = String::new();
    let mut previous: Option<char> = None;
    let mut characters = name.chars().peekable();
    while let Some(character) = characters.next() {
        let starts_word = character.is_uppercase()
            && previous.is_some_and(|previous| {
                previous.is_lowercase()
                    || previous.is_numeric()
                    || (previous.is_uppercase()
                        && characters.peek().is_some_and(|next| next.is_lowercase()))
            });
        if starts_word {
            label.push(' ');
        }
        label.extend(character.to_lowercase());
        previous = Some(character);
    }
    label
}

/// The plan explicitly permits Unicode lowercase instead of Python casefold.
/// Keep this policy in one place for name matching, event deduplication, and ids.
/// This does not equate all casefold pairs (for example, German ß and ss).
pub(crate) fn matching_key(text: &str) -> String {
    text.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        CharacterName, Instructions, RawResponse, RenderedPrompt, TransportDiagnostics, type_label,
    };

    #[test]
    fn exchange_text_preserves_every_byte_including_empty_and_whitespace() {
        for input in [
            "",
            " \t\r\n",
            " \n{\"narrative\":\"Ajax\"}\r\n",
            "\u{2003}é😀\n",
        ] {
            assert_eq!(
                RawResponse::new(input).as_str().as_bytes(),
                input.as_bytes()
            );
            assert_eq!(
                Instructions::new(input).as_str().as_bytes(),
                input.as_bytes()
            );
            assert_eq!(
                RenderedPrompt::new(input).as_str().as_bytes(),
                input.as_bytes()
            );
        }
        assert_eq!(CharacterName::new(" Ajax \n").unwrap().as_str(), "Ajax");
        assert!(CharacterName::new(" \n").is_err());
    }

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

    #[test]
    fn type_names_supply_readable_error_labels() {
        for (name, label) in [
            ("CharacterId", "character id"),
            ("characterId", "character id"),
            ("HTTPResponse", "http response"),
            ("WorldJSON", "world json"),
            ("Backstory", "backstory"),
        ] {
            assert_eq!(type_label(name), label);
        }
        assert_eq!(
            CharacterName::new(" ").unwrap_err().to_string(),
            "character name must not be blank"
        );
    }
}
