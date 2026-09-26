//! Codex CLI's schema and stdin compatibility boundary.
//!
//! Transformations here are based on the authenticated 0.157.1 discovery
//! profile in `reference/02-codex-cli.md`. Shared schemas stay untouched.

use serde_json::Map;
use serde_json::Value;

pub const STDIN_PREFACE: &str = "Perform the story generation task in this envelope. The instructions field supplies generation rules; the prompt field supplies story context and player input. Answer directly without using tools or reading files.";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("Codex request preparation failed at {location}: {reason}")]
pub struct PreparationError {
    location: String,
    reason: String,
}

impl PreparationError {
    fn at(location: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            reason: reason.into(),
        }
    }
}

/// Adapt an owned shared schema for the observed Codex structured-output profile.
pub fn adapt_schema(schema: Value) -> Result<Value, PreparationError> {
    match schema {
        Value::Object(object) => adapt_node(Value::Object(object), "$".into()),
        _ => Err(PreparationError::at("$", "root schema must be an object")),
    }
}

/// Supported schema-bearing keywords emitted by this project's schemas are
/// enumerated here: `$defs`, legacy `definitions`, `properties`,
/// `patternProperties`, `dependentSchemas`, `items`, `prefixItems`, `allOf`, `anyOf`, `oneOf`,
/// `additionalProperties`, `unevaluatedProperties`, `not`, `if`, `then`,
/// `else`, `contains`, `propertyNames`, and `unevaluatedItems`. Other values
/// (including defaults, examples, enum values and annotations) are opaque.
/// Only the emitted 2020-12 dialect (or an implicit dialect) is supported.
/// Legacy applicators and dynamic reference/vocabulary mechanisms are rejected;
/// this is not a general JSON Schema validator or walker.
fn adapt_node(schema: Value, path: String) -> Result<Value, PreparationError> {
    let Value::Object(object) = schema else {
        return match schema {
            Value::Bool(_) => Ok(schema),
            _ => Err(PreparationError::at(
                path,
                "schema node must be an object or boolean",
            )),
        };
    };

    if let Some(dialect) = object.get("$schema")
        && dialect.as_str() != Some("https://json-schema.org/draft/2020-12/schema")
    {
        return Err(PreparationError::at(
            child_path(&path, "$schema"),
            "only the project's draft/2020-12 schema dialect is supported",
        ));
    }
    for keyword in [
        "dependencies",
        "additionalItems",
        "$dynamicRef",
        "$recursiveRef",
        "$vocabulary",
    ] {
        if object.contains_key(keyword) {
            return Err(PreparationError::at(
                child_path(&path, keyword),
                "unsupported structural schema keyword",
            ));
        }
    }

    let wrap_reference = object.contains_key("$ref") && object.contains_key("description");
    if object.contains_key("$ref") {
        if !object["$ref"].is_string() {
            return Err(PreparationError::at(
                child_path(&path, "$ref"),
                "reference must be a string",
            ));
        }
        if object
            .get("description")
            .is_some_and(|description| !description.is_string())
        {
            return Err(PreparationError::at(
                child_path(&path, "description"),
                "description must be a string when adapting a reference",
            ));
        }
        let unsupported_sibling = object
            .keys()
            .any(|key| key != "$ref" && key != "description");
        if unsupported_sibling {
            return Err(PreparationError::at(
                &path,
                "a $ref with additional siblings cannot be adapted safely",
            ));
        }
    }

    // Codex requires every property to be listed. Match the source property
    // insertion order so narrative remains first and schema diffs stay stable.
    if let Some(required) = object.get("required") {
        let required = required.as_array().ok_or_else(|| {
            PreparationError::at(
                format!("{path}.required"),
                "expected an array of property names",
            )
        })?;
        if required.iter().any(|name| !name.is_string()) {
            return Err(PreparationError::at(
                format!("{path}.required"),
                "every required entry must be a property name string",
            ));
        }
    }
    let required_properties = if let Some(properties) = object.get("properties") {
        let properties = properties.as_object().ok_or_else(|| {
            PreparationError::at(format!("{path}.properties"), "expected an object map")
        })?;
        if let Some(required) = object.get("required").and_then(Value::as_array) {
            for (index, name) in required.iter().enumerate() {
                let name = name.as_str().expect("required entries validated above");
                if !properties.contains_key(name) {
                    return Err(PreparationError::at(
                        format!("{path}.required[{index}]"),
                        format!(
                            "required property {name:?} is absent from properties; refusing to discard its constraint"
                        ),
                    ));
                }
            }
        }
        Some(
            properties
                .keys()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    let mut adapted = Map::new();
    let mut reference = None;
    for (keyword, child) in object {
        if keyword == "$ref" && wrap_reference {
            reference = Some(child);
            continue;
        }
        let location = child_path(&path, &keyword);
        let child = match keyword.as_str() {
            "required" if required_properties.is_some() => {
                Value::Array(required_properties.clone().expect("checked above"))
            }
            "$defs" | "definitions" | "properties" | "patternProperties" | "dependentSchemas" => {
                let children = child.as_object().ok_or_else(|| {
                    PreparationError::at(&location, "expected an object map of schemas")
                })?;
                let mut result = Map::new();
                for (name, child) in children {
                    result.insert(
                        name.clone(),
                        adapt_node(child.clone(), child_path(&location, name))?,
                    );
                }
                Value::Object(result)
            }
            "items"
            | "additionalProperties"
            | "unevaluatedProperties"
            | "not"
            | "if"
            | "then"
            | "else"
            | "contains"
            | "propertyNames"
            | "unevaluatedItems" => match child {
                Value::Bool(value) => Value::Bool(value),
                value @ Value::Object(_) => adapt_node(value, location)?,
                _ => {
                    return Err(PreparationError::at(
                        location,
                        "expected a schema object or boolean",
                    ));
                }
            },
            "prefixItems" | "allOf" | "anyOf" | "oneOf" => {
                let children = child.as_array().ok_or_else(|| {
                    PreparationError::at(&location, "expected an array of schemas")
                })?;
                let result = children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| adapt_node(child.clone(), format!("{location}[{index}]")))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::Array(result)
            }
            _ => child,
        };
        adapted.insert(keyword, child);
    }
    if let Some(reference) = reference {
        let reference = Value::Object(Map::from_iter([("$ref".into(), reference)]));
        adapted.insert("anyOf".into(), Value::Array(vec![reference]));
    }
    if let Some(required) = required_properties
        && !adapted.contains_key("required")
    {
        adapted.insert("required".into(), Value::Array(required));
    }
    Ok(Value::Object(adapted))
}

fn child_path(parent: &str, key: &str) -> String {
    format!("{parent}.{key}")
}

/// Freeze instructions and prompt into the combined user-message envelope seen
/// in the discovery recorder. This is not a privileged instruction channel.
pub fn frame_stdin(instructions: &str, prompt: &str) -> Vec<u8> {
    let envelope = serde_json::json!({ "instructions": instructions, "prompt": prompt });
    format!("{STDIN_PREFACE}\n{envelope}\n").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> Value {
        serde_json::from_str(match path {
            "world" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/world/schema.json"),
            "cast" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/cast/schema.json"),
            "turn" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/opening_turn/schema.json"),
            "continuation" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/continuation_turn/schema.json"),
            "zero_npcs" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/zero_npcs/schema.json"),
            _ => panic!("unknown fixture {path}"),
        })
        .unwrap()
    }

    fn expected(path: &str) -> Value {
        serde_json::from_str(match path {
            "world" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/world/schema.codex-discovery.json"),
            "cast" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/cast/schema.codex-discovery.json"),
            "turn" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/opening_ref_union/schema.codex-discovery.json"),
            "continuation" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/continuation_ref_union/schema.codex-discovery.json"),
            "zero_npcs" => include_str!("../../../../reviews/2026-09-26-codex-profile/requests/zero_npcs/schema.codex-discovery.json"),
            _ => panic!("unknown fixture {path}"),
        })
        .unwrap()
    }

    fn canonical(value: Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
            Value::Object(values) => {
                let sorted: std::collections::BTreeMap<_, _> = values
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect();
                Value::Object(Map::from_iter(sorted))
            }
            value => value,
        }
    }

    #[test]
    fn required_names_outside_properties_are_rejected_without_losing_constraints() {
        let error = adapt_schema(serde_json::json!({
            "type": "object", "properties": {"visible": {"type": "string"}},
            "required": ["visible", "must_survive"], "additionalProperties": true
        }))
        .unwrap_err();
        assert!(error.to_string().contains("$.required[1]"), "{error}");
        assert!(error.to_string().contains("must_survive"), "{error}");
        let standalone = serde_json::json!({"required": ["existing"]});
        assert_eq!(adapt_schema(standalone.clone()).unwrap(), standalone);
    }

    #[test]
    fn unsupported_dialects_and_structural_keywords_are_located_errors() {
        for keyword in [
            "dependencies",
            "additionalItems",
            "$dynamicRef",
            "$recursiveRef",
            "$vocabulary",
        ] {
            let schema = serde_json::json!({"properties": {"nested": {keyword: {"properties": {"hidden": {"type": "string"}}}}}});
            let error = adapt_schema(schema).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&format!("$.properties.nested.{keyword}")),
                "{error}"
            );
        }
        let error =
            adapt_schema(serde_json::json!({"$schema": "http://json-schema.org/draft-07/schema#"}))
                .unwrap_err();
        assert!(error.to_string().contains("$.$schema"), "{error}");
    }

    #[test]
    fn malformed_property_map_is_a_located_preparation_error() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": "not a schema map"
        });
        let error = adapt_schema(schema).expect_err("malformed properties must be rejected");
        assert!(error.to_string().contains("$.properties"), "{error}");
    }

    #[test]
    fn a_ref_with_a_union_sibling_is_rejected_at_its_location() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "value": {"$ref": "#/$defs/Value", "description": "doc", "anyOf": []}
            }
        });
        let error = adapt_schema(schema).expect_err("unrecognized $ref siblings must fail");
        assert!(error.to_string().contains("$.properties.value"), "{error}");
    }

    #[test]
    fn unsupported_schema_children_report_their_location_and_boolean_constraints_survive() {
        let malformed = serde_json::json!({
            "type": "object",
            "properties": {"list": {"type": "array", "items": "not a schema"}}
        });
        let error = adapt_schema(malformed).expect_err("unsupported items shape must be rejected");
        assert!(
            error.to_string().contains("$.properties.list.items"),
            "{error}"
        );

        let valid = serde_json::json!({
            "type": "object",
            "properties": {"value": {"type": "string"}},
            "additionalProperties": false
        });
        let adapted = adapt_schema(valid).unwrap();
        assert_eq!(adapted["additionalProperties"], false);
        assert_eq!(adapted["required"], serde_json::json!(["value"]));
    }

    #[test]
    fn malformed_required_keyword_is_rejected_even_without_properties() {
        let schema = serde_json::json!({"type": "object", "required": "value"});
        let error = adapt_schema(schema).expect_err("malformed required must not pass through");
        assert!(error.to_string().contains("$.required"), "{error}");
    }

    #[test]
    fn bundled_schemas_match_independent_discovery_expectations() {
        let requests = [
            ("world", fixture("world")),
            ("cast", fixture("cast")),
            ("turn", fixture("turn")),
            ("zero_npcs", fixture("zero_npcs")),
        ];
        for (kind, input) in requests {
            let adapted = adapt_schema(input).unwrap();
            assert_eq!(
                canonical(adapted.clone()),
                canonical(expected(kind)),
                "Codex {kind} schema differs from the independently reviewed discovery copy"
            );
        }
        assert_eq!(
            canonical(adapt_schema(fixture("continuation")).unwrap()),
            canonical(expected("continuation")),
            "continuation schema must match its separate reviewed live-success copy"
        );
    }

    #[test]
    fn adapted_turn_keeps_order_nullability_defaults_and_empty_strings() {
        let adapted = adapt_schema(fixture("turn")).unwrap();
        let properties = adapted["properties"].as_object().unwrap();
        assert_eq!(
            properties.keys().next().map(String::as_str),
            Some("narrative")
        );
        assert_eq!(
            adapted["required"],
            Value::Array(properties.keys().cloned().map(Value::String).collect())
        );
        assert_eq!(
            properties["chapter_title"]["type"],
            serde_json::json!(["string", "null"])
        );
        assert_eq!(
            adapted["$defs"]["SummaryUpdate"]["properties"]["upcoming_events"]["type"],
            serde_json::json!(["array", "null"])
        );
        let delta = &adapted["$defs"]["CharacterDelta"]["properties"];
        assert_eq!(delta["relationships"]["type"], "string");
        assert_eq!(delta["relationships"]["default"], "");
        assert_eq!(properties["scene_description"]["type"], "string");
        assert!(properties["scene_description"].get("minLength").is_none());
        assert_eq!(
            adapted["$defs"]["SummaryUpdate"]["properties"]["world"]["type"],
            "string"
        );
        assert_eq!(
            adapted["$defs"]["SummaryUpdate"]["properties"]["world"]["default"],
            ""
        );
        assert_eq!(
            adapted["$defs"]["QuickAction"]["properties"]["kind"]["anyOf"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(adapted["$schema"], fixture("turn")["$schema"]);
        assert_eq!(
            adapted["$defs"]["QuickActionKind"]["enum"],
            fixture("turn")["$defs"]["QuickActionKind"]["enum"]
        );
    }

    #[test]
    fn transformation_is_idempotent_and_leaves_schema_looking_annotations_opaque() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "ref": {"$ref": "#/$defs/Thing"},
                "annotated": {"$ref": "#/$defs/Thing", "description": "annotation"},
                "sample": {
                    "type": "string",
                    "default": {"properties": {"fake": {"$ref": "not a schema"}}},
                    "examples": [{"properties": {"also_fake": {"type": "object"}}}],
                    "enum": [{"$ref": "still data"}]
                }
            },
            "$defs": {"Thing": {"type": "object", "properties": {"value": {"type": "string"}}}}
        });
        let once = adapt_schema(schema).unwrap();
        let twice = adapt_schema(once.clone()).unwrap();
        assert_eq!(once, twice);
        assert_eq!(once["properties"]["ref"]["$ref"], "#/$defs/Thing");
        assert_eq!(
            once["properties"]["annotated"]["anyOf"][0]["$ref"],
            "#/$defs/Thing"
        );
        assert_eq!(
            once["properties"]["sample"]["default"]["properties"]["fake"]["$ref"],
            "not a schema"
        );
        assert_eq!(
            once["properties"]["sample"]["examples"][0]["properties"]["also_fake"]["type"],
            "object"
        );
        assert_eq!(
            once["properties"]["sample"]["enum"][0]["$ref"],
            "still data"
        );
        assert_eq!(
            once["$defs"]["Thing"]["required"],
            serde_json::json!(["value"])
        );
    }

    #[test]
    fn framing_matches_the_committed_discovery_capture_byte_for_byte() {
        let instructions = include_str!(
            "../../../../reviews/2026-09-26-codex-profile/requests/opening_ref_union/instructions.txt"
        );
        let prompt = include_str!(
            "../../../../reviews/2026-09-26-codex-profile/requests/opening_ref_union/prompt.txt"
        );
        let expected = include_bytes!(
            "../../../../reviews/2026-09-26-codex-profile/evidence/opening-ref-union/stdin.txt"
        );
        assert_eq!(frame_stdin(instructions, prompt), expected);
    }
}
