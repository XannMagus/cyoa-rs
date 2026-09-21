use cyoa_infrastructure::backend::{BackendError, GenerationResponse, InputTokens, TokenUsage};
use serde_json::json;

#[test]
fn successful_response_retains_the_exact_json_it_parsed() {
    let raw = "{ \"narrative\": \"A light in the dark.\" }\n";
    let response = GenerationResponse::from_json(raw.into(), TokenUsage::default()).unwrap();
    assert_eq!(response.raw_response(), raw);
    assert_eq!(
        response.value(),
        &json!({"narrative": "A light in the dark."})
    );
    assert_eq!(response.usage(), TokenUsage::default());
}

#[test]
fn malformed_response_is_an_error_with_original_diagnostics() {
    let raw = "{\"narrative\": \"unfinished";
    let error = GenerationResponse::from_json(raw.into(), TokenUsage::default()).unwrap_err();
    match error {
        BackendError::Generation { raw_response, .. } => assert_eq!(raw_response, raw),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn cached_tokens_cannot_exceed_total_but_unknown_is_not_zero() {
    assert!(InputTokens::new(5, Some(6)).is_err());
    assert_eq!(InputTokens::new(5, Some(5)).unwrap().cached(), Some(5));
    assert_eq!(InputTokens::new(0, Some(0)).unwrap().total(), 0);
    assert_ne!(InputTokens::new(0, None), InputTokens::new(0, Some(0)));
}
