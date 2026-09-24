use cyoa_infrastructure::generation::stream::StreamingStringField;
#[test]
fn truncated_and_invalid_json_only_yield_available_preview_text() {
    let mut scanner = StreamingStringField::new("narrative");
    assert_eq!(scanner.feed("{\"narrative\":\"visible \\uD83"), "visible ");
    assert_eq!(scanner.feed(""), "");
    let mut scanner = StreamingStringField::new("narrative");
    assert_eq!(scanner.feed(r#"{"narrative":"X\uZZZZ"}"#), "X�");
}

#[test]
fn surrogate_pairs_lone_halves_and_escapes_survive_single_character_chunks() {
    for (raw, expected) in [
        (r#"{"narrative":"\uD83D\uDE00\uD800!\uDC00"}"#, "😀�!�"),
        (
            r#"{"narrative":"\"\\\/\b\f\n\r\t"}"#,
            "\"\\/\u{8}\u{c}\n\r\t",
        ),
        (r#"{"narrative":"\uD800"}"#, "�"),
    ] {
        let mut scanner = StreamingStringField::new("narrative");
        let actual = raw
            .chars()
            .map(|c| scanner.feed(&c.to_string()))
            .collect::<String>();
        assert_eq!(actual, expected);
        assert_eq!(scanner.feed(r#"{"narrative":"second"}"#), "");
    }
}
#[test]
fn only_the_requested_top_level_string_is_streamed() {
    let raw = "```json\n{\"nested\":[{\"narrative\":\"wrong\"}],\"number\":3,\"narr\\u0061tive\":\"Right 🎭\",\"tail\":\"ignored\"}\n```";
    assert_eq!(StreamingStringField::new("narrative").feed(raw), "Right 🎭");
    for raw in [
        r#"{"other":"none"}"#,
        r#"{"narrative":{"nested":"wrong"}}"#,
        r#"{"narrative":null}"#,
    ] {
        assert_eq!(StreamingStringField::new("narrative").feed(raw), "");
    }
}

#[test]
fn arbitrary_scalar_splits_match_the_independent_narrative() {
    let alphabet = [
        'a', '\n', '\r', '\t', '"', '\\', '🎭', '界', 'ß', '\u{0}', 'é',
    ];
    let mut seed = 42u64;
    for length in 1..80 {
        let narrative = (0..length)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                alphabet[(seed as usize) % alphabet.len()]
            })
            .collect::<String>();
        let raw =
            serde_json::json!({"nested":{"narrative":"wrong"},"narrative":narrative}).to_string();
        let expected = serde_json::from_str::<serde_json::Value>(&raw).unwrap()["narrative"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(StreamingStringField::new("narrative").feed(&raw), expected);
        let positions = raw
            .char_indices()
            .map(|(i, _)| i)
            .chain([raw.len()])
            .collect::<Vec<_>>();
        for &split in &positions {
            let mut scanner = StreamingStringField::new("narrative");
            let actual = scanner.feed(&raw[..split]) + &scanner.feed(&raw[split..]);
            assert_eq!(actual, expected, "split {split} in {raw}");
        }
        let mut scanner = StreamingStringField::new("narrative");
        let mut actual = String::new();
        let mut i = 0;
        while i + 1 < positions.len() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let next = (i + 1 + (seed as usize % 7)).min(positions.len() - 1);
            actual.push_str(&scanner.feed(&raw[positions[i]..positions[next]]));
            i = next;
        }
        assert_eq!(actual, expected);
    }
}
