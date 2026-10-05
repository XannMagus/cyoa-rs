//! No older Rust format has shipped. Only version one is currently supported.
use super::codec::SaveCodecError;
use cyoa_application::persistence::SaveFormatVersion;
use serde_json::Value;
pub(super) const CURRENT: SaveFormatVersion =
    SaveFormatVersion::from_nonzero(std::num::NonZeroU32::MIN);
pub(super) fn upgrade(value: Value) -> Result<Value, SaveCodecError> {
    dispatch(value, CURRENT, &[])
}
type Migration = fn(Value) -> Result<Value, SaveCodecError>;
/// Each step upgrades a document from the version it is keyed by to the next.
fn dispatch(
    mut value: Value,
    current: SaveFormatVersion,
    steps: &[(SaveFormatVersion, Migration)],
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
            .get()
            .checked_add(1)
            .and_then(|next| SaveFormatVersion::new(next).ok())
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
fn version(value: &Value) -> Result<SaveFormatVersion, SaveCodecError> {
    value
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .and_then(|v| SaveFormatVersion::new(v).ok())
        .ok_or_else(|| SaveCodecError::invalid("version", "required positive u32 version"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn v(version: u32) -> SaveFormatVersion {
        SaveFormatVersion::new(version).unwrap()
    }
    fn rejection(result: Result<Value, SaveCodecError>) -> String {
        result.unwrap_err().to_string()
    }
    #[test]
    fn synthetic_scaffold_rejects_missing_steps_and_incorrect_advancement() {
        // Each failure is identified, so one cannot stand in for the other.
        assert!(rejection(dispatch(json!({"version":1}), v(2), &[])).contains("no migration"));
        assert!(
            rejection(dispatch(json!({"version":1}), v(2), &[(v(1), Ok)]))
                .contains("must advance exactly one version")
        );
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
            dispatch(json!({"version":1}), v(3), &[(v(1), first), (v(2), second)]).unwrap(),
            json!({"version":3,"first":true})
        );
        assert!(rejection(upgrade(json!({"version":0}))).contains("positive"));
        assert!(rejection(upgrade(json!({"version":2}))).contains("newer than supported"));
    }
}
