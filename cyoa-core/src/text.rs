//! Nonblank text value objects. Optional values use `Option<T>` rather than an
//! empty-string sentinel. Every constructor trims boundary whitespace.

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

define_nonblank_string_type!(CharacterName);
define_nonblank_string_type!(WorldDescription);
define_nonblank_string_type!(CurrentSituation);
define_nonblank_string_type!(EventText);
define_nonblank_string_type!(CharacterDescription);
define_nonblank_string_type!(Backstory);
define_nonblank_string_type!(Relationships);
define_nonblank_string_type!(CharacterSituation);

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
    use super::{CharacterName, type_label};

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
