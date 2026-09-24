//! JSON Schema generation: structure from `#[derive(JsonSchema)]` on the wire
//! types in `wire.rs`, field/`_doc` descriptions injected from
//! `schema_docs.toml`. See PLAN.md's "Schema generation" section.
//!
//! The builder functions below (`world_outline_schema`, etc.) are
//! **backend-agnostic and produce the fullest, most standards-compliant
//! schema available** — a root `"$schema"` draft declaration, and nested
//! types via `$defs`/`$ref` (schemars' default, keyed by the
//! `#[schemars(rename = "...")]` name on each wire struct so they line up
//! 1:1 with `schema_docs.toml`'s `[TypeName]` tables), never trimmed for any
//! one backend's convenience. Whatever a specific backend's CLI can't
//! tolerate is stripped by *that backend's own* adapter in
//! `generation::backend_compat`, starting from this full schema — this file
//! is never touched to add a backend (`ARCH-003`).
//!
//! **Verified against a real `claude -p --json-schema` call** (2026-09-24,
//! see `01-claude-cli.md`'s "Verified test #3"): `$defs`/`$ref` are resolved
//! correctly, including following an arbitrary instruction stated only in a
//! `$ref`'d nested field's `description` — but a root-level `"$schema"` key
//! is rejected outright ("not a valid JSON Schema: no schema with key or ref
//! ..."), which is `backend_compat::claude_cli::adapt_schema`'s reason to
//! exist.

use std::sync::OnceLock;

use cyoa_core::limits::Limits;
use schemars::{
    JsonSchema,
    generate::{SchemaGenerator, SchemaSettings},
};
use serde_json::Value;

use super::wire::{GeneratedCastWire, StoryTurnWire, WorldOutlineWire};

const SCHEMA_DOCS_TOML: &str = include_str!("defaults/schema_docs.toml");

fn schema_docs() -> &'static toml::Table {
    static DOCS: OnceLock<toml::Table> = OnceLock::new();
    DOCS.get_or_init(|| {
        SCHEMA_DOCS_TOML
            .parse::<toml::Table>()
            .expect("defaults/schema_docs.toml parses as TOML")
    })
}

fn doc_text(type_name: &str, key: &str, limits: &Limits) -> Option<String> {
    let raw = schema_docs()
        .get(type_name)?
        .as_table()?
        .get(key)?
        .as_str()?;
    if raw.is_empty() {
        return None;
    }
    let env = minijinja::Environment::new();
    let context = super::limits_context::limits_context(limits);
    Some(env.render_str(raw, context).unwrap_or_else(|error| {
        panic!("schema_docs.toml[{type_name}].{key} failed to render: {error}")
    }))
}

fn inject_descriptions_for(mut value: Value, type_name: &str, limits: &Limits) -> Value {
    let Some(obj) = value.as_object_mut() else {
        return value;
    };
    if let Some(doc) = doc_text(type_name, "_doc", limits) {
        obj.insert("description".into(), Value::String(doc));
    }
    if let Some(properties) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        let fields: Vec<String> = properties.keys().cloned().collect();
        for field in fields {
            let Some(text) = doc_text(type_name, &field, limits) else {
                continue;
            };
            if let Some(field_obj) = properties.get_mut(&field).and_then(Value::as_object_mut) {
                field_obj.insert("description".into(), Value::String(text));
            }
        }
    }
    value
}

/// Every object-type schema (root and every `$defs` entry) forbids
/// unlisted properties, matching the shape verified against a live
/// `claude -p --json-schema` call in `01-claude-cli.md`.
fn forbid_additional_properties(mut value: Value) -> Value {
    if let Some(obj) = value.as_object_mut()
        && obj.contains_key("properties")
    {
        obj.insert("additionalProperties".into(), Value::Bool(false));
    }
    value
}

/// The generic, backend-agnostic settings: full standards compliance
/// (a root `"$schema"` key present) and `$defs`/`$ref` for nested types
/// (not inlined) — the richest, most complete representation. **Never strip
/// anything here for one backend's tolerance** — that belongs in that
/// backend's own adapter in `generation::backend_compat`, which each
/// independently decide what *their* CLI can't handle, starting from this
/// full schema and only ever removing what they must.
fn generator() -> SchemaGenerator {
    SchemaSettings::default().into_generator()
}

fn build_schema<T: JsonSchema>(root_type_name: &str, limits: &Limits) -> Value {
    let schema = generator().into_root_schema_for::<T>();
    let mut value = Value::from(schema);
    value = inject_descriptions_for(value, root_type_name, limits);
    value = forbid_additional_properties(value);
    if let Some(defs) = value.get_mut("$defs").and_then(Value::as_object_mut) {
        let names: Vec<String> = defs.keys().cloned().collect();
        for name in names {
            if let Some(def) = defs.remove(&name) {
                let def = inject_descriptions_for(def, &name, limits);
                let def = forbid_additional_properties(def);
                defs.insert(name, def);
            }
        }
    }
    value
}

pub fn world_outline_schema() -> Value {
    build_schema::<WorldOutlineWire>("WorldOutline", &Limits::default())
}

pub fn generated_cast_schema(limits: &Limits) -> Value {
    build_schema::<GeneratedCastWire>("GeneratedCast", limits)
}

pub fn story_turn_schema(limits: &Limits) -> Value {
    build_schema::<StoryTurnWire>("StoryTurn", limits)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        for (type_name, schema) in all_object_schemas(&limits) {
            seen_types.insert(type_name.clone());
            let doc_table = schema_docs()
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
