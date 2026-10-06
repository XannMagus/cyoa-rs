//! Generated save-slot identities, shared by the local repository and the
//! supervised helper so both name new slots identically.
use cyoa_application::persistence::SaveId;

/// Random IDs tried before a create reports a collision conflict.
pub(super) const COLLISION_ATTEMPTS: usize = 8;

/// `<slug>-<32 hex>`: an ASCII slug of the world title (at most 40 characters,
/// `story` when nothing survives) and 16 random bytes.
pub(super) fn generated_save_id(title: &str, random: [u8; 16]) -> SaveId {
    let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
    SaveId::new(format!("{}-{suffix}", slug(title))).expect("generated grammar")
}

fn slug(title: &str) -> String {
    let mut result = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            if result.len() == 40 {
                break;
            }
            result.push(c.to_ascii_lowercase());
        } else if !result.is_empty() && !result.ends_with('-') && result.len() < 40 {
            result.push('-');
        }
    }
    let result = result.trim_end_matches('-');
    if result.is_empty() {
        "story".into()
    } else {
        result.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_slug_titles_and_always_satisfy_the_id_grammar() {
        let random = [0xab; 16];
        let hex = "ab".repeat(16);
        for (title, slug) in [
            ("The Bell That Kept the Tide", "the-bell-that-kept-the-tide"),
            ("../../ 霧 🔥 / Story!?", "story"),
            ("霧", "story"),
            (&"x".repeat(50), &"x".repeat(40)),
            ("a  b--c", "a-b-c"),
        ] {
            assert_eq!(
                generated_save_id(title, random).as_str(),
                format!("{slug}-{hex}"),
                "{title}"
            );
        }
    }
}
