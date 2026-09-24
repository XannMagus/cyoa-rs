//! Backend-independent structure comes from Rust wire types; descriptions are
//! supplied only by a validated GenerationTemplates instance.
use super::wire::{GeneratedCastWire, StoryTurnWire, WorldOutlineWire};
use schemars::JsonSchema;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn structure<T: JsonSchema>() -> Value {
    Value::from(schemars::generate::SchemaGenerator::default().into_root_schema_for::<T>())
}

pub(super) fn documentation_fields() -> BTreeMap<String, BTreeSet<String>> {
    let mut types = BTreeMap::new();
    for (name, schema) in [
        ("WorldOutline", structure::<WorldOutlineWire>()),
        ("GeneratedCast", structure::<GeneratedCastWire>()),
        ("StoryTurn", structure::<StoryTurnWire>()),
    ] {
        let schemas = std::iter::once((name, &schema)).chain(
            schema
                .get("$defs")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|defs| defs.iter().map(|(name, schema)| (name.as_str(), schema))),
        );
        for (name, schema) in schemas {
            let mut fields: BTreeSet<String> = schema
                .get("properties")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|props| props.keys().cloned())
                .collect();
            fields.insert("_doc".into());
            types.insert(name.into(), fields);
        }
    }
    // Outgoing memory is not a generated response; its serializer is separate.
    for (name, fields) in [
        (
            "StorySummary",
            &[
                "world",
                "major_events",
                "characters",
                "current_situation",
                "upcoming_events",
            ][..],
        ),
        (
            "CharacterState",
            &[
                "name",
                "description",
                "backstory",
                "relationships",
                "current_state",
                "id",
            ][..],
        ),
    ] {
        types.insert(
            name.into(),
            fields
                .iter()
                .copied()
                .chain(["_doc"])
                .map(str::to_owned)
                .collect(),
        );
    }
    types
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::templates::GenerationTemplates;
    use cyoa_core::limits::Limits;
    fn schema_docs() -> toml::Table {
        toml::from_str(include_str!("defaults/schema_docs.toml")).unwrap()
    }
    fn world_outline_schema() -> Value {
        GenerationTemplates::bundled()
            .unwrap()
            .world_schema()
            .unwrap()
    }
    fn generated_cast_schema(limits: &Limits) -> Value {
        GenerationTemplates::bundled()
            .unwrap()
            .cast_schema(limits)
            .unwrap()
    }
    fn story_turn_schema(limits: &Limits) -> Value {
        GenerationTemplates::bundled()
            .unwrap()
            .turn_schema(limits)
            .unwrap()
    }
    use std::collections::BTreeSet;

    /// Every `TypeName` this schema generation actually covers, i.e. every
    /// table in `schema_docs.toml` that corresponds to a real
    /// generated/`$ref`d schema. `StorySummary`/`CharacterState` are
    /// documented there too (they describe the JSON the engine *sends*,
    /// serialized by hand, not a schema we ask the model to conform to) and
    /// are deliberately excluded from this list and from the coverage test.
    const GENERATED_TYPE_NAMES: &[&str] = &[
        "WorldOutline",
        "GeneratedCast",
        "PlayerCharacter",
        "NonPlayerCharacter",
        "CharacterDelta",
        "SummaryUpdate",
        "QuickActionKind",
        "QuickAction",
        "StoryTurn",
    ];

    /// Every object schema this module can produce (root + `$defs`), as
    /// (type_name, field_names).
    fn all_object_schemas(limits: &Limits) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        for (name, schema) in [
            ("WorldOutline", world_outline_schema()),
            ("GeneratedCast", generated_cast_schema(limits)),
            ("StoryTurn", story_turn_schema(limits)),
        ] {
            out.push((name.to_string(), schema.clone()));
            if let Some(defs) = schema.get("$defs").and_then(Value::as_object) {
                for (def_name, def_schema) in defs {
                    out.push((def_name.clone(), def_schema.clone()));
                }
            }
        }
        out
    }

    #[test]
    fn schema_docs_cover_every_field_and_no_others() {
        let limits = Limits::default();
        let mut seen_types = BTreeSet::new();
        let docs = schema_docs();
        for (type_name, schema) in all_object_schemas(&limits) {
            seen_types.insert(type_name.clone());
            let doc_table = docs
                .get(&type_name)
                .unwrap_or_else(|| panic!("schema_docs.toml is missing a [{type_name}] table"))
                .as_table()
                .unwrap();
            let documented: BTreeSet<&str> = doc_table
                .keys()
                .map(String::as_str)
                .filter(|k| *k != "_doc")
                .collect();
            let actual: BTreeSet<&str> = schema
                .get("properties")
                .and_then(Value::as_object)
                .map(|p| p.keys().map(String::as_str).collect())
                .unwrap_or_default();
            assert_eq!(
                documented, actual,
                "{type_name}: schema_docs.toml fields vs. generated schema fields disagree"
            );
        }
        // Every generated-type name is actually covered by this test run —
        // guards against silently dropping a type from GENERATED_TYPE_NAMES.
        for name in GENERATED_TYPE_NAMES {
            assert!(
                seen_types.contains(*name),
                "{name} is listed as generated but no schema produced it"
            );
        }
    }

    #[test]
    fn generic_schemas_declare_a_root_schema_key() {
        // The generic builders are backend-agnostic and stay fully
        // standards-compliant: they always declare which JSON Schema draft
        // they conform to. Only a specific backend's adapter (below) may
        // decide its own CLI can't tolerate that key.
        let limits = Limits::default();
        for schema in [
            world_outline_schema(),
            generated_cast_schema(&limits),
            story_turn_schema(&limits),
        ] {
            assert!(
                schema.get("$schema").is_some(),
                "generic schema builders must declare a root \"$schema\" key"
            );
        }
    }

    #[test]
    fn story_turn_schema_lists_narrative_first() {
        let schema = story_turn_schema(&Limits::default());
        let properties = schema.get("properties").unwrap().as_object().unwrap();
        assert_eq!(
            properties.keys().next().map(String::as_str),
            Some("narrative")
        );
    }

    #[test]
    fn schema_docs_placeholders_render_against_supplied_limits() {
        let limits = Limits {
            max_generated_npcs: cyoa_core::limits::MaxGeneratedNpcs::new(5),
            ..Limits::default()
        };
        let schema = generated_cast_schema(&limits);
        let description = schema["properties"]["npcs"]["description"]
            .as_str()
            .unwrap();
        assert!(
            description.contains("5 other characters"),
            "expected the supplied limit (5) in: {description}"
        );
        assert!(!description.contains("{{"));
    }

    #[test]
    fn every_generated_object_schema_forbids_additional_properties() {
        let limits = Limits::default();
        for (name, schema) in all_object_schemas(&limits) {
            if schema.get("properties").is_some() {
                assert_eq!(
                    schema.get("additionalProperties"),
                    Some(&Value::Bool(false)),
                    "{name} does not forbid additional properties"
                );
            }
        }
    }
}
