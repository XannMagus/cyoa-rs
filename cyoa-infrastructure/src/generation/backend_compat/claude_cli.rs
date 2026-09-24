//! `claude -p`'s own tolerance adapter. Starts from the shared, maximal
//! output of `generation::schema`/`wire`/`prompts` and adjusts only what
//! `claude -p`'s CLI actually can't handle — nothing here may feed back
//! into those generic modules.

use serde_json::Value;

/// `claude -p --json-schema` rejects a root `"$schema"` key outright —
/// verified live, `01-claude-cli.md`'s "Verified test #3": `"not a valid
/// JSON Schema: no schema with key or ref ..."`. This is `claude -p`'s own
/// tolerance limit, not a general JSON Schema defect, so it is stripped
/// here, immediately before a `ClaudeCliBackend` would pass the schema to
/// `--json-schema` — never inside `generation::schema`'s generic builders,
/// which other backends also call and which may have no objection to the
/// key.
pub fn adapt(schema: &mut Value) {
    if let Some(obj) = schema.as_object_mut() {
        obj.remove("$schema");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::schema::story_turn_schema;
    use cyoa_core::limits::Limits;

    #[test]
    fn adapt_strips_the_root_schema_key_and_nothing_else() {
        let mut schema = story_turn_schema(&Limits::default());
        let before = schema.clone();
        adapt(&mut schema);
        assert!(schema.get("$schema").is_none());
        let mut expected = before;
        expected.as_object_mut().unwrap().remove("$schema");
        assert_eq!(schema, expected, "adapt must change nothing else");
    }
}
