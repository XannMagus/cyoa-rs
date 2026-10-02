//! Version 1 demo: the frozen Phase 0 story, with five successful turns.
//! Arbitrary player choices do not change this prerecorded fiction.
use super::scripted::ChunkedBackend;
pub fn harbour_v1() -> ChunkedBackend {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/phase0_story.json"))
            .expect("bundled demo fixture is valid JSON");
    let responses = std::iter::once(fixture["outline"].clone())
        .chain(std::iter::once(fixture["cast"].clone()))
        .chain(
            fixture["turns"]
                .as_array()
                .expect("bundled demo turns")
                .iter()
                .cloned(),
        )
        .map(|v| Ok(v.to_string()))
        .collect::<Vec<_>>();
    ChunkedBackend::new(responses, 17)
}
