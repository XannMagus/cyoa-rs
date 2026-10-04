//! No older Rust format has shipped. Only version one is currently supported.
use super::codec::SaveCodecError;
use serde_json::Value;
pub(super) const CURRENT: u32 = 1;
pub(super) fn upgrade(value: Value) -> Result<Value, SaveCodecError> {
    dispatch(value, CURRENT, &[])
}
type Migration = fn(Value) -> Result<Value, SaveCodecError>;
fn dispatch(
    mut value: Value,
    current: u32,
    steps: &[(u32, Migration)],
) -> Result<Value, SaveCodecError> {
    let mut version = version(&value)?;
    if version > current {
        return Err(SaveCodecError::future(version, current));
    }
    while version < current {
        let step = steps
            .iter()
            .find(|(from, _)| *from == version)
            .ok_or_else(|| SaveCodecError::invalid("version", "no migration for this version"))?
            .1;
        value = step(value)?;
        let next = version
            .checked_add(1)
            .ok_or_else(|| SaveCodecError::invalid("version", "version overflow"))?;
        if self::version(&value)? != next {
            return Err(SaveCodecError::invalid(
                "version",
                "migration must advance exactly one version",
            ));
        }
        version = next;
    }
    Ok(value)
}
fn version(value: &Value) -> Result<u32, SaveCodecError> {
    value
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| SaveCodecError::invalid("version", "required positive u32 version"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn synthetic_scaffold_rejects_missing_steps_and_incorrect_advancement() {
        assert!(dispatch(json!({"version":1}), 2, &[]).is_err());
        assert!(dispatch(json!({"version":1}), 2, &[(1, |v| Ok(v))]).is_err());
    }
    #[test]
    fn synthetic_scaffold_runs_each_step_once_in_order_without_shipping_legacy_support() {
        fn first(mut v: Value) -> Result<Value, SaveCodecError> {
            v["version"] = 2.into();
            v["first"] = true.into();
            Ok(v)
        }
        fn second(mut v: Value) -> Result<Value, SaveCodecError> {
            assert_eq!(v["first"], true);
            v["version"] = 3.into();
            Ok(v)
        }
        assert_eq!(
            dispatch(json!({"version":1}), 3, &[(1, first), (2, second)]).unwrap(),
            json!({"version":3,"first":true})
        );
        assert!(upgrade(json!({"version":0})).is_err());
        assert!(upgrade(json!({"version":2})).is_err());
    }
}
