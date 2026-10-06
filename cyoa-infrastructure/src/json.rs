//! Strict JSON parsing shared by adapters and the save codec.
//!
//! `serde_json::Value` silently keeps the last of two equal object keys, so a
//! document can mean different things to different consumers. Strict parsing
//! rejects every duplicate key, in every object, instead of picking one.
//! Nesting stays bounded by serde_json's own recursion limit, which reports a
//! syntax error long before the stack is at risk.

use std::cell::Cell;
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub(crate) enum StrictJsonError {
    #[error(transparent)]
    Syntax(serde_json::Error),
    #[error("duplicate object key {0:?}")]
    DuplicateKey(String),
}

/// Parses one complete JSON document, rejecting duplicate keys anywhere.
pub(crate) fn strict_value(bytes: &[u8]) -> Result<Value, StrictJsonError> {
    let duplicate = Cell::new(None);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let parsed = Strict(&duplicate)
        .deserialize(&mut deserializer)
        .and_then(|value| deserializer.end().map(|()| value));
    parsed.map_err(|error| match duplicate.take() {
        Some(key) => StrictJsonError::DuplicateKey(key),
        None => StrictJsonError::Syntax(error),
    })
}

/// The first duplicate key is recorded out of band, so callers can tell it
/// from a syntax error without parsing error messages.
#[derive(Clone, Copy)]
struct Strict<'a>(&'a Cell<Option<String>>);
impl<'de> DeserializeSeed<'de> for Strict<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Strict<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("JSON with unique object keys")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut values = vec![];
        while let Some(value) = a.next_element_seed(self)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = a.next_key::<String>()? {
            if values.contains_key(&key) {
                let error = de::Error::custom(format!("duplicate object key {key:?}"));
                self.0.set(Some(key));
                return Err(error);
            }
            let value = a.next_value_seed(self)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

/// Validates one complete document's syntax and key uniqueness without building
/// a value tree, so a caller can then deserialize the same bytes directly into
/// its own types. Only each object's own keys are retained while it is read.
pub(crate) fn reject_duplicate_keys(bytes: &[u8]) -> Result<(), StrictJsonError> {
    let duplicate = Cell::new(None);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let checked = Unique(&duplicate)
        .deserialize(&mut deserializer)
        .and_then(|()| deserializer.end());
    checked.map_err(|error| match duplicate.take() {
        Some(key) => StrictJsonError::DuplicateKey(key),
        None => StrictJsonError::Syntax(error),
    })
}
#[derive(Clone, Copy)]
struct Unique<'a>(&'a Cell<Option<String>>);
impl<'de> DeserializeSeed<'de> for Unique<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Unique<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("JSON with unique object keys")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_none<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        while a.next_element_seed(self)?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        let mut seen = std::collections::HashSet::new();
        while let Some(key) = a.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                let error = de::Error::custom(format!("duplicate object key {key:?}"));
                self.0.set(Some(key));
                return Err(error);
            }
            a.next_value_seed(self)?;
        }
        Ok(())
    }
}

/// A syntactically valid span can still be ambiguous: two equal keys in one
/// object mean different consumers read different values (`serde_json::Value`
/// silently keeps the last). Reject, never pick. Only objects selected by
/// `in_scope` (by key path) are checked; other subtrees are skipped unexamined.
/// Excessive nesting also fails here and is likewise not valid.
pub(crate) fn has_duplicate_keys_in(span: &str, in_scope: fn(&[String]) -> bool) -> bool {
    struct Unique {
        path: Vec<String>,
        in_scope: fn(&[String]) -> bool,
    }
    impl Unique {
        fn child(&self, key: &str) -> Unique {
            let mut path = self.path.clone();
            path.push(key.to_owned());
            Unique {
                path,
                in_scope: self.in_scope,
            }
        }
    }
    impl<'de> DeserializeSeed<'de> for Unique {
        type Value = ();
        fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            if (self.in_scope)(&self.path) {
                d.deserialize_any(self)
            } else {
                d.deserialize_ignored_any(de::IgnoredAny).map(|_| ())
            }
        }
    }
    impl<'de> Visitor<'de> for Unique {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("any JSON value")
        }
        fn visit_bool<E>(self, _: bool) -> Result<(), E> {
            Ok(())
        }
        fn visit_i64<E>(self, _: i64) -> Result<(), E> {
            Ok(())
        }
        fn visit_u64<E>(self, _: u64) -> Result<(), E> {
            Ok(())
        }
        fn visit_f64<E>(self, _: f64) -> Result<(), E> {
            Ok(())
        }
        fn visit_str<E>(self, _: &str) -> Result<(), E> {
            Ok(())
        }
        fn visit_unit<E>(self) -> Result<(), E> {
            Ok(())
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            while seq.next_element_seed(self.child("[]"))?.is_some() {}
            Ok(())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            let mut seen = std::collections::HashSet::new();
            while let Some(key) = map.next_key::<String>()? {
                let child = self.child(&key);
                if !seen.insert(key) {
                    return Err(de::Error::custom("duplicate key"));
                }
                map.next_value_seed(child)?;
            }
            Ok(())
        }
    }
    let mut deserializer = serde_json::Deserializer::from_str(span);
    Unique {
        path: vec![],
        in_scope,
    }
    .deserialize(&mut deserializer)
    .is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_keys_are_rejected_at_any_depth_and_never_resolved() {
        for document in [
            r#"{"type":"turn.failed","type":"turn.completed"}"#,
            r#"{"item":{"text":"a","text":"b"}}"#,
            r#"[{"ok":1},{"x":{"y":[{"k":1,"k":1}]}}]"#,
        ] {
            assert!(
                matches!(
                    strict_value(document.as_bytes()),
                    Err(StrictJsonError::DuplicateKey(_))
                ),
                "{document}"
            );
        }
    }

    #[test]
    fn syntax_errors_stay_distinct_from_duplicates() {
        for document in ["{", r#"{"a":1} trailing"#, "", r#"{"a":1,}"#] {
            assert!(
                matches!(
                    strict_value(document.as_bytes()),
                    Err(StrictJsonError::Syntax(_))
                ),
                "{document:?}"
            );
        }
    }

    #[test]
    fn unique_documents_parse_identically_to_serde_json() {
        let document = r#"{"a":[1,2.5,"x",null,true,{"b":{}}],"c":"é"}"#;
        assert_eq!(
            strict_value(document.as_bytes()).unwrap(),
            serde_json::from_str::<Value>(document).unwrap()
        );
    }

    #[test]
    fn tree_free_validation_agrees_with_strict_parsing() {
        for document in [
            r#"{"a":1,"a":2}"#,
            r#"[{"x":{"y":[{"k":1,"k":1}]}}]"#,
            r#"{"a":[1,{"b":null}],"c":"\u00e9"}"#,
            "{",
            r#"{"a":1} trailing"#,
        ] {
            let tree = strict_value(document.as_bytes()).map(|_| ());
            let free = reject_duplicate_keys(document.as_bytes());
            assert_eq!(
                format!("{tree:?}").split('(').next(),
                format!("{free:?}").split('(').next(),
                "{document}"
            );
            assert_eq!(
                matches!(tree, Err(StrictJsonError::DuplicateKey(_))),
                matches!(free, Err(StrictJsonError::DuplicateKey(_))),
                "{document}"
            );
        }
    }

    #[test]
    fn excessive_nesting_is_a_bounded_syntax_error() {
        let deep = "[".repeat(100_000) + &"]".repeat(100_000);
        assert!(matches!(
            strict_value(deep.as_bytes()),
            Err(StrictJsonError::Syntax(_))
        ));
        let modest = "[".repeat(64) + &"]".repeat(64);
        assert!(strict_value(modest.as_bytes()).is_ok());
    }
}
