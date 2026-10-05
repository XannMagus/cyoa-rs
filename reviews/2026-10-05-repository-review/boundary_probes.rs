//! Review reproductions: assertions describe the required rejection behavior.
use cyoa_application::cancellation::CancellationSource;
use cyoa_infrastructure::{
    backend::{Backend, GenerationRequest},
    backends::{
        claude_cli::{ClaudeCliBackend, ClaudeExecutable, ClaudeInvocationConfig},
        codex_cli::{CodexCliBackend, CodexExecutable, CodexInvocationConfig},
    },
};
use serde_json::json;

#[test]
fn review_codex_duplicate_terminal_type_must_be_rejected() {
    let home = tempfile::tempdir().unwrap();
    let stdout = concat!(
        "{\"type\":\"thread.started\",\"thread_id\":\"review\"}\n",
        "{\"type\":\"turn.started\"}\n",
        "{\"type\":\"item.completed\",\"item\":{\"id\":\"m\",\"type\":\"agent_message\",\"text\":\"{\\\"title\\\":\\\"Harbour\\\",\\\"world_description\\\":\\\"A quiet port\\\"}\"}}\n",
        "{\"type\":\"turn.failed\",\"error\":{\"message\":\"generation failed\"},\"type\":\"turn.completed\"}\n"
    );
    std::fs::write(
        home.path().join("cyoa-fixture.json"),
        serde_json::to_vec(&json!({
            "stdout": [{"bytes": stdout.as_bytes()}], "drain_stdin": true, "exit_code": 0
        }))
        .unwrap(),
    )
    .unwrap();
    let exe = CodexExecutable::resolve(
        env!("CARGO_BIN_EXE_subprocess_fixture").as_ref(),
        "/bin".as_ref(),
        home.path(),
    )
    .unwrap();
    let config =
        CodexInvocationConfig::new(exe, home.path().into(), "/bin".into(), None, None).unwrap();
    let token = CancellationSource::default().token();
    let mut backend = CodexCliBackend::connect(config, &token).unwrap();
    let mut emissions = 0;
    let schema = json!({});
    let result = backend.generate(
        GenerationRequest {
            instructions: "Review",
            prompt: "Review",
            schema: &schema,
        },
        &token,
        &mut |_| emissions += 1,
    );
    assert!(
        result.is_err(),
        "ambiguous Codex failure was accepted; emissions={emissions}"
    );
}

#[test]
fn review_claude_duplicate_auth_method_must_be_rejected() {
    let home = tempfile::tempdir().unwrap();
    let status = r#"{"loggedIn":true,"authMethod":"api_key","authMethod":"claude.ai","apiProvider":"firstParty"}"#;
    std::fs::write(
        home.path().join("cyoa-fixture.json"),
        serde_json::to_vec(&json!({
            "auth_status": status, "auth_stdout": true, "exit_code": 0
        }))
        .unwrap(),
    )
    .unwrap();
    let exe = ClaudeExecutable::resolve(
        env!("CARGO_BIN_EXE_subprocess_fixture").as_ref(),
        "/bin".as_ref(),
        home.path(),
    )
    .unwrap();
    let config =
        ClaudeInvocationConfig::new(exe, home.path().into(), "/bin".into(), None, None).unwrap();
    let result = ClaudeCliBackend::connect(config, &CancellationSource::default().token());
    assert!(
        result.is_err(),
        "conflicting Claude auth methods were accepted"
    );
}
