//! The style choices for a game: how its prose reads and its images look.
//! Ports calibre's `StoryStyle` (cyoa.py:440-451). `None` means the first
//! (default) entry of the matching table, so a style that was never chosen
//! stays valid without a migration when a table gains a first-ever entry.
//! Style tables themselves are prompt-layer data, not domain state.

use crate::text::{ArtStyleKey, NarrationKey, PaceKey, ToneKey};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StoryStyle {
    pub art_style: Option<ArtStyleKey>,
    pub pace: Option<PaceKey>,
    pub tone: Option<ToneKey>,
    pub narration: Option<NarrationKey>,
}
