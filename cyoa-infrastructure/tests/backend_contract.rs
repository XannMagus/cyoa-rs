mod support;
use support::fixture_backend;

use cyoa_application::{cancellation::CancellationSource, generation::*};
use cyoa_core::text::{Brief, TransportDiagnostics};
use cyoa_infrastructure::backend::{
    Backend, BackendError, GenerationRequest, GenerationResponse, InputTokens, TokenUsage,
    normalize_input_tokens,
};
use cyoa_infrastructure::generation::scripted::ScriptedBackend;
use cyoa_infrastructure::generation::{engine::GenerationEngine, templates::GenerationTemplates};
use serde_json::json;

fn use_cases(
    responses: impl IntoIterator<Item = Result<String, BackendError>>,
) -> StoryUseCases<GenerationEngine<ScriptedBackend>> {
    StoryUseCases::new(GenerationEngine::new(
        ScriptedBackend::new(responses),
        GenerationTemplates::bundled().unwrap(),
    ))
}

/// A `GenerationRequest` with empty/placeholder content: these tests exercise
/// transport outcome mapping, not prompt/schema rendering.
fn blank_request<'a>(schema: &'a serde_json::Value) -> GenerationRequest<'a> {
    GenerationRequest {
        instructions: "",
        prompt: "",
        schema,
    }
}

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
        BackendError::Generation {
            raw_response,
            diagnostics,
            ..
        } => {
            assert_eq!(raw_response, raw);
            // `from_json` has no subprocess of its own to attach diagnostics
            // from; a real adapter attaches its captured bytes separately
            // (see the fixture-driven tests below).
            assert!(diagnostics.is_empty());
        }
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

// --- Error tests: BackendError -> GenerationFailure mapping -------------

#[test]
fn every_backend_error_variant_maps_to_its_failure_kind_with_exact_diagnostics() {
    let stdout = b"partial stdout captured before failure".to_vec();
    let stderr: Vec<u8> = vec![b'e', b'r', b'r', 0xff, 0xfe, b'!'];
    let diagnostics = TransportDiagnostics::new(stdout.clone(), stderr.clone());

    let variant_cases = [
        (
            BackendError::Cancelled {
                diagnostics: diagnostics.clone(),
            },
            FailureKind::Cancelled,
        ),
        (
            BackendError::Unavailable {
                message: "backend unreachable".into(),
                diagnostics: diagnostics.clone(),
            },
            FailureKind::Unavailable,
        ),
        (
            BackendError::Timeout {
                diagnostics: diagnostics.clone(),
            },
            FailureKind::Timeout,
        ),
        (
            BackendError::Generation {
                message: "transport failed".into(),
                raw_response: "oops".into(),
                diagnostics: diagnostics.clone(),
            },
            FailureKind::Transport,
        ),
    ];

    for (error, expected_kind) in variant_cases {
        let mut engine_use_cases = use_cases([Err(error)]);
        let source = CancellationSource::default();
        let failure = engine_use_cases
            .generate_outline(&Brief::new("A brief").unwrap(), &source.token())
            .unwrap_err();
        assert_eq!(failure.kind(), expected_kind);
        assert_eq!(failure.diagnostics().stdout(), stdout.as_slice());
        assert_eq!(failure.diagnostics().stderr(), stderr.as_slice());
    }
}

#[test]
fn nonzero_exit_with_no_stdout_maps_to_unavailable_with_exact_stderr_bytes() {
    let stderr: &[u8] = b"authentication failed\n";
    let scenario = fixture_backend::scenario(&[], &[stderr], 1);
    let mut backend = fixture_backend::FixtureBackend::new(scenario);
    let source = CancellationSource::default();
    let schema = json!({});
    let error = backend
        .generate(blank_request(&schema), &source.token(), &mut |_| {})
        .unwrap_err();
    match error {
        BackendError::Unavailable { diagnostics, .. } => {
            assert_eq!(diagnostics.stderr(), stderr);
            assert!(diagnostics.stdout().is_empty());
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn nonzero_exit_after_emitting_stdout_maps_to_generation_not_unavailable() {
    let stdout: &[u8] = b"{\"narrative\": \"partial";
    let stderr: &[u8] = b"boom";
    let scenario = fixture_backend::scenario(&[stdout], &[stderr], 1);
    let mut backend = fixture_backend::FixtureBackend::new(scenario);
    let source = CancellationSource::default();
    let schema = json!({});
    let error = backend
        .generate(blank_request(&schema), &source.token(), &mut |_| {})
        .unwrap_err();
    match error {
        BackendError::Generation {
            raw_response,
            diagnostics,
            ..
        } => {
            assert_eq!(raw_response.as_bytes(), stdout);
            assert_eq!(diagnostics.stdout(), stdout);
            assert_eq!(diagnostics.stderr(), stderr);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn cancelling_from_inside_the_streamed_callback_returns_cancelled_with_diagnostics_seen_so_far() {
    let payload: &[u8] = br#"{"narrative":"draft"}"#;
    let scenario = fixture_backend::scenario(&[payload], &[], 0);
    let mut backend = fixture_backend::FixtureBackend::new(scenario);
    let source = CancellationSource::default();
    let schema = json!({});
    let mut seen = String::new();
    let error = backend
        .generate(blank_request(&schema), &source.token(), &mut |chunk| {
            seen.push_str(chunk);
            source.cancel();
        })
        .unwrap_err();
    match error {
        BackendError::Cancelled { diagnostics } => {
            assert_eq!(diagnostics.stdout(), payload);
            assert!(diagnostics.stderr().is_empty());
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(seen.as_bytes(), payload);
}

// --- Edge tests -----------------------------------------------------------

#[test]
fn empty_stdout_with_exit_zero_does_not_construct_a_false_success() {
    let scenario = fixture_backend::scenario(&[], &[], 0);
    let mut backend = fixture_backend::FixtureBackend::new(scenario);
    let source = CancellationSource::default();
    let schema = json!({});
    let error = backend
        .generate(blank_request(&schema), &source.token(), &mut |_| {})
        .unwrap_err();
    match error {
        BackendError::Generation { raw_response, .. } => assert_eq!(raw_response, ""),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn crlf_in_diagnostics_is_preserved_without_normalization() {
    let stderr: &[u8] = b"line one\r\nline two\r\n";
    let scenario = fixture_backend::scenario(&[], &[stderr], 1);
    let mut backend = fixture_backend::FixtureBackend::new(scenario);
    let source = CancellationSource::default();
    let schema = json!({});
    let error = backend
        .generate(blank_request(&schema), &source.token(), &mut |_| {})
        .unwrap_err();
    match error {
        BackendError::Unavailable { diagnostics, .. } => {
            assert_eq!(diagnostics.stderr(), stderr)
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn non_utf8_stderr_diagnostics_survive_the_process_boundary_and_lossy_display_does_not_panic() {
    let stderr: &[u8] = &[b'e', b'r', b'r', 0xff, 0xfe, b'!'];
    let scenario = fixture_backend::scenario(&[], &[stderr], 1);
    let mut backend = fixture_backend::FixtureBackend::new(scenario);
    let source = CancellationSource::default();
    let schema = json!({});
    let error = backend
        .generate(blank_request(&schema), &source.token(), &mut |_| {})
        .unwrap_err();
    match error {
        BackendError::Unavailable { diagnostics, .. } => {
            assert_eq!(diagnostics.stderr(), stderr);
            let lossy = diagnostics.stderr_lossy();
            assert_ne!(lossy.as_bytes(), diagnostics.stderr());
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn adapter_reported_invalid_cache_accounting_normalizes_to_unknown_over_a_real_process_boundary() {
    let payload: &[u8] = br#"{"narrative":"ok"}"#;
    let scenario = fixture_backend::scenario(&[payload], &[], 0);
    let mut backend =
        fixture_backend::FixtureBackend::new(scenario).with_usage(fixture_backend::ReportedUsage {
            input_total: 5,
            input_cached: Some(6),
            output: Some(3),
        });
    let source = CancellationSource::default();
    let schema = json!({});
    let response = backend
        .generate(blank_request(&schema), &source.token(), &mut |_| {})
        .unwrap();
    let input = response.usage().input.unwrap();
    assert_eq!(input.total(), 5);
    assert_eq!(input.cached(), None);
    assert_eq!(response.usage().output, Some(3));
}

#[test]
fn normalize_input_tokens_distinguishes_unknown_from_zero_and_repairs_only_invalid_cache() {
    assert_eq!(normalize_input_tokens(0, None).cached(), None);
    assert_eq!(normalize_input_tokens(0, Some(0)).cached(), Some(0));
    assert_ne!(
        normalize_input_tokens(0, None),
        normalize_input_tokens(0, Some(0))
    );
    let repaired = normalize_input_tokens(5, Some(6));
    assert_eq!(repaired.total(), 5);
    assert_eq!(repaired.cached(), None);
    let valid = normalize_input_tokens(5, Some(5));
    assert_eq!(valid.cached(), Some(5));
}

// --- Nominal ---------------------------------------------------------------

#[test]
fn fixture_reports_the_exact_argv_and_stdin_prompt_bytes_it_received() {
    let payload: &[u8] = br#"{"narrative":"ok"}"#;
    let report_path = fixture_backend::report_path("argv-stdin");
    let scenario =
        fixture_backend::scenario_with_report(&[payload], &[], 0, true, Some(&report_path));
    let mut backend = fixture_backend::FixtureBackend::new(scenario.clone());
    let source = CancellationSource::default();
    let prompt = "line one\nline two \"quoted\" \u{1F600} caf\u{e9}\r\n";
    let request = GenerationRequest {
        instructions: "",
        prompt,
        schema: &json!({}),
    };
    backend
        .generate(request, &source.token(), &mut |_| {})
        .unwrap();

    let report = fixture_backend::read_report(&report_path);
    assert_eq!(report.argv.get(1), Some(&scenario));
    assert_eq!(report.stdin, prompt.as_bytes());
}

#[test]
fn fixture_round_trips_quotes_newlines_and_unicode_through_a_real_process_boundary() {
    let payload =
        "{\"narrative\": \"He said \\\"hello\\\"\\nnext line: \u{1F600} \u{2014} caf\u{e9}.\"}";
    let scenario = fixture_backend::scenario(&[payload.as_bytes()], &[], 0);
    let mut backend = fixture_backend::FixtureBackend::new(scenario);
    let source = CancellationSource::default();
    let schema = json!({});
    let mut seen = String::new();
    let response = backend
        .generate(blank_request(&schema), &source.token(), &mut |chunk| {
            seen.push_str(chunk)
        })
        .unwrap();
    assert_eq!(response.raw_response(), payload);
    assert_eq!(seen, payload);
    assert_eq!(
        response.value()["narrative"],
        "He said \"hello\"\nnext line: \u{1F600} \u{2014} caf\u{e9}."
    );
}
