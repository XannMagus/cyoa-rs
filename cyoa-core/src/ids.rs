//! Stable character identity, independent of the character's mutable name.

use crate::text::{define_nonblank_string_type, matching_key};

define_nonblank_string_type!(CharacterId);

impl CharacterId {
    pub fn protagonist() -> Self {
        Self("protagonist".into())
    }

    /// Slug names using calibre's punctuation, fallback, and 32-character rules.
    /// Explicit ids are only trimmed, not slugged or truncated.
    pub fn for_name(name: &str) -> Self {
        let words: String = matching_key(name)
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect();
        let slug = words.split_whitespace().collect::<Vec<_>>().join("-");
        let truncated: String = slug.chars().take(32).collect();
        let id = truncated.trim_end_matches('-');
        Self(if id.is_empty() {
            "character".into()
        } else {
            id.into()
        })
    }

    pub(crate) fn unique(&self, is_taken: impl Fn(&Self) -> bool) -> Self {
        let mut candidate = self.clone();
        let mut suffix = 2_u64;
        while is_taken(&candidate) {
            candidate = Self(format!("{}-{suffix}", self.0));
            suffix += 1;
        }
        candidate
    }
}
