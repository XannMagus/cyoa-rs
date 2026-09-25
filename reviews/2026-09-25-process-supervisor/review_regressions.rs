use cyoa_application::{
    cancellation::CancellationSource,
    generation::{FailureKind, StoryUseCases},
};
use cyoa_core::text::{Brief, TransportDiagnostics};
use cyoa_infrastructure::{
    backend::*,
    backends::process::*,
    generation::{engine::GenerationEngine, templates::GenerationTemplates},
};
use std::{ffi::OsString, time::Duration};

#[test]
fn malformed_domain_response_retains_transport_diagnostics() {
    struct InvalidOutline;
    impl Backend for InvalidOutline {
        fn generate(
            &mut self,
            _: GenerationRequest<'_>,
            _: &cyoa_application::cancellation::CancellationToken,
            _: &mut dyn FnMut(&str),
        ) -> Result<GenerationResponse, BackendError> {
            Ok(GenerationResponse::from_json(
                "{\"title\":\"\",\"world_description\":\"A world\"}".into(),
                TokenUsage::default(),
            )
            .unwrap()
            .with_diagnostics(TransportDiagnostics::new(b"events", b"diagnostic warning")))
        }
    }
    let mut cases = StoryUseCases::new(GenerationEngine::new(
        InvalidOutline,
        GenerationTemplates::bundled().unwrap(),
    ));
    let err = cases
        .generate_outline(
            &Brief::new("Story").unwrap(),
            &CancellationSource::default().token(),
        )
        .unwrap_err();
    assert_eq!(err.kind(), FailureKind::InvalidResponse);
    assert_eq!(err.diagnostics().stderr(), b"diagnostic warning");
}
fn spec(scenario: serde_json::Value, input: Vec<u8>, cap: usize) -> ProcessSpec {
    ProcessSpec {
        program: env!("CARGO_BIN_EXE_subprocess_fixture").into(),
        args: vec![OsString::from(scenario.to_string())],
        env: EnvPolicy::new(),
        stdin: input,
        bounds: ProcessBounds::new(
            Duration::from_secs(3),
            MaxStdoutBytes::new(cap).unwrap(),
            MaxStderrBytes::new(1024).unwrap(),
        )
        .unwrap(),
    }
}
#[test]
fn retained_stdout_respects_its_configured_bound() {
    let spec = spec(
        serde_json::json!({"stdout":[{"bytes":vec![b'x';4096]}],"stderr":[],"exit_code":0}),
        vec![],
        100,
    );
    let error = run(&spec, &CancellationSource::default().token(), &mut |_| {
        Ok(())
    })
    .unwrap_err();
    assert!(matches!(error, SupervisorError::OutputBoundExceeded { .. }));
    assert!(
        error.diagnostics().stdout().len() <= 100,
        "100-byte cap retained {} bytes",
        error.diagnostics().stdout().len()
    );
}
#[test]
fn incomplete_request_delivery_cannot_report_success() {
    let spec = spec(
        serde_json::json!({"stdout":[{"bytes":b"{\"ok\":true}\n"}],"drain_stdin":false,"exit_code":0}),
        vec![b'a'; 2 * 1024 * 1024],
        1024,
    );
    let result = run(&spec, &CancellationSource::default().token(), &mut |_| {
        Ok(())
    });
    assert!(
        result.is_err(),
        "reported success despite a request larger than the pipe and a child that never reads it"
    );
}
#[test]
fn already_signalled_cancellation_wins_even_before_its_notifier_runs() {
    let path = format!("/tmp/cyoa-review-cancel-{}.json", std::process::id());
    let source = CancellationSource::default();
    let token = source.token();
    let (release, wait_release) = std::sync::mpsc::channel();
    token.on_cancel(move || {
        wait_release.recv().unwrap();
    });
    let signal_path = path.clone();
    let canceller = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !std::path::Path::new(&signal_path).exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        source.cancel();
    });
    let spec = spec(
        serde_json::json!({"stdout":[],"report_path":path,"hang_ms":200,"exit_code":0}),
        vec![],
        1024,
    );
    let result = run(&spec, &token, &mut |_| Ok(()));
    assert!(token.is_cancelled());
    release.send(()).unwrap();
    canceller.join().unwrap();
    let _ = std::fs::remove_file(path);
    assert!(
        matches!(result, Err(SupervisorError::Cancelled { .. })),
        "cancelled token yielded {result:?}"
    );
}
