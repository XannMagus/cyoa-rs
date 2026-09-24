use cyoa_core::limits::Limits;
use minijinja::{Value, context};

/// Shared generation preferences, constrained by the supplied domain limits.
/// Preserve the default 3–5 playable / 3–8 NPC requests without contradicting
/// higher playable minimums or smaller NPC caps. These are requests, not new
/// domain validation bounds: two usable playables still suffice by default.
pub(super) fn limits_context(limits: &Limits) -> Value {
    context! {
        min_generated_playables => limits.min_playable_characters.get().max(3),
        max_generated_playables => limits.min_playable_characters.get().max(5),
        min_generated_npcs => limits.max_generated_npcs.get().min(3),
        max_generated_npcs => limits.max_generated_npcs.get(),
        max_major_events => limits.max_major_events.get(),
    }
}
