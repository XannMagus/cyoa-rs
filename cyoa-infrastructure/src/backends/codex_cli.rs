//! Codex invocation configuration and owned request preparation.
//!
//! Owns fixed-profile request preparation, subscription auth preflight and
//! protocol decoding. The adapter accepts output only after the vendor-neutral
//! supervisor completes request delivery, child reaping and workspace cleanup.

mod adapter;
mod protocol;
pub use adapter::CodexCliBackend;

use crate::{
    backends::{
        executable::{self, ExecutableError, ResolvedExecutable},
        process::{
            EnvPolicy, MaxStderrBytes, MaxStdoutBytes, ProcessBounds, ProcessSpec, RequestWorkspace,
        },
    },
    generation::{backend_compat::codex_cli, templates::RenderedGeneration},
};
use serde_json::Value;
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexExecutable(ResolvedExecutable);

impl CodexExecutable {
    /// Resolve a configured executable against a selected PATH and base
    /// directory before any request changes the child's working directory.
    /// Empty PATH entries mean the base directory, as in Unix PATH lookup.
    pub fn resolve(
        executable: &OsStr,
        selected_path: &OsStr,
        base_directory: &Path,
    ) -> Result<Self, ConfigurationError> {
        Ok(Self(ResolvedExecutable::resolve(
            executable,
            selected_path,
            base_directory,
        )?))
    }

    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }
}

/// An optional configured model selection. A configured value is not evidence
/// of the model Codex later reports; this option was not used by step 1's live
/// probes and remains unverified as a CLI override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexModel(String);

impl CodexModel {
    pub fn new(value: impl Into<String>) -> Result<Self, ConfigurationError> {
        let value = value.into();
        validate_string_value("model", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("invalid Codex {field}: {reason}")]
    InvalidValue {
        field: &'static str,
        reason: &'static str,
    },
    #[error("Codex executable {0:?} was not found in the selected PATH")]
    ExecutableNotFound(OsString),
    #[error("could not resolve Codex executable {path}: {source}")]
    ResolveExecutable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl From<ExecutableError> for ConfigurationError {
    fn from(error: ExecutableError) -> Self {
        match error {
            ExecutableError::InvalidValue { field, reason } => Self::InvalidValue { field, reason },
            ExecutableError::NotFound(name) => Self::ExecutableNotFound(name),
            ExecutableError::Resolve { path, source } => Self::ResolveExecutable { path, source },
        }
    }
}

#[derive(Debug, Clone)]
pub struct CodexInvocationConfig {
    executable: CodexExecutable,
    home: PathBuf,
    selected_path: OsString,
    codex_home: Option<PathBuf>,
    model: Option<CodexModel>,
    bounds: ProcessBounds,
}

impl CodexInvocationConfig {
    /// Finite transport controls only; does not expose arbitrary CLI options.
    pub fn with_bounds(mut self, bounds: ProcessBounds) -> Self {
        self.bounds = bounds;
        self
    }
    pub fn new(
        executable: CodexExecutable,
        home: PathBuf,
        selected_path: OsString,
        codex_home: Option<PathBuf>,
        model: Option<CodexModel>,
    ) -> Result<Self, ConfigurationError> {
        validate_path("HOME", &home, true)?;
        validate_os_value("PATH", &selected_path)?;
        if let Some(codex_home) = &codex_home {
            validate_path("CODEX_HOME", codex_home, true)?;
        }
        Ok(Self {
            executable,
            home,
            selected_path,
            codex_home,
            model,
            bounds: ProcessBounds::new(
                Duration::from_secs(180),
                MaxStdoutBytes::new(1_048_576).expect("fixed stdout bound is nonzero"),
                MaxStderrBytes::new(262_144).expect("fixed stderr bound is nonzero"),
            )
            .expect("fixed deadline and bounds are nonzero"),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RequestPreparationError {
    #[error(transparent)]
    Schema(#[from] codex_cli::PreparationError),
    #[error("could not create Codex request workspace: {0}")]
    Workspace(#[source] std::io::Error),
    #[error("could not serialize adapted Codex schema: {0}")]
    SerializeSchema(#[source] serde_json::Error),
    #[error("could not write adapted schema at {path}: {source}")]
    WriteSchema {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// A fully owned, fixed-profile Codex request, ready for `process::run`.
/// Keeping `ProcessSpec` private prevents callers from mutating its argv or
/// bypassing the explicit environment and resource bounds during preparation.
#[derive(Debug)]
pub struct PreparedCodexRequest(ProcessSpec);

impl PreparedCodexRequest {
    pub fn from_generation(
        config: &CodexInvocationConfig,
        request: &RenderedGeneration,
    ) -> Result<Self, RequestPreparationError> {
        Self::prepare_parts(
            config,
            request.instructions().as_str(),
            request.prompt().as_str(),
            request.schema(),
        )
    }

    fn prepare_parts(
        config: &CodexInvocationConfig,
        instructions: &str,
        prompt: &str,
        shared_schema: &Value,
    ) -> Result<Self, RequestPreparationError> {
        let adapted_schema = codex_cli::adapt_schema(shared_schema.clone())?;
        let workspace = RequestWorkspace::new().map_err(RequestPreparationError::Workspace)?;
        Self::build_with_workspace(config, instructions, prompt, adapted_schema, workspace)
    }

    fn build_with_workspace(
        config: &CodexInvocationConfig,
        instructions: &str,
        prompt: &str,
        adapted_schema: Value,
        workspace: RequestWorkspace,
    ) -> Result<Self, RequestPreparationError> {
        let schema_path = workspace.path().join("schema.json");
        write_schema(&schema_path, &adapted_schema)?;
        Ok(Self(ProcessSpec {
            workspace,
            program: config.executable.as_path().to_path_buf(),
            args: fixed_arguments(config.model.as_ref()),
            env: explicit_environment(config),
            stdin: codex_cli::frame_stdin(instructions, prompt),
            bounds: config.bounds,
        }))
    }

    /// Transfer the complete request and workspace to the existing supervisor.
    /// This only moves owned data; it does not launch a child.
    pub fn into_process_spec(self) -> ProcessSpec {
        self.0
    }
}

fn fixed_arguments(model: Option<&CodexModel>) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "exec",
        "-c",
        "project_doc_max_bytes=0",
        "--json",
        "--ephemeral",
        "--sandbox",
        "read-only",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--color",
        "never",
        "--output-schema",
        "schema.json",
        "-",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    if let Some(model) = model {
        args.splice(
            3..3,
            [OsString::from("--model"), OsString::from(model.as_str())],
        );
    }
    args
}

fn explicit_environment(config: &CodexInvocationConfig) -> EnvPolicy {
    let environment = EnvPolicy::new()
        .set("HOME", config.home.as_os_str().to_owned())
        .set("PATH", config.selected_path.clone());
    match &config.codex_home {
        Some(path) => environment.set("CODEX_HOME", path.as_os_str().to_owned()),
        None => environment,
    }
}

fn write_schema(path: &Path, schema: &Value) -> Result<(), RequestPreparationError> {
    let bytes =
        serde_json::to_vec_pretty(schema).map_err(RequestPreparationError::SerializeSchema)?;
    fs::write(path, bytes).map_err(|source| RequestPreparationError::WriteSchema {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_path(
    field: &'static str,
    value: &Path,
    require_absolute: bool,
) -> Result<(), ConfigurationError> {
    Ok(executable::validate_path(field, value, require_absolute)?)
}

fn validate_string_value(field: &'static str, value: &str) -> Result<(), ConfigurationError> {
    if field == "model" && !value.trim().is_empty() && value.starts_with('-') {
        return Err(ConfigurationError::InvalidValue {
            field,
            reason: "must not begin with an option prefix",
        });
    }
    Ok(executable::validate_string_value(field, value)?)
}

fn validate_os_value(field: &'static str, value: &OsStr) -> Result<(), ConfigurationError> {
    Ok(executable::validate_os_value(field, value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::templates::GenerationTemplates;
    use cyoa_core::text::Brief;

    fn executable_file(directory: &Path, name: &str) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, b"preparation must not run this file").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        path
    }

    fn config(directory: &Path) -> CodexInvocationConfig {
        let path = std::env::join_paths([directory]).unwrap();
        let executable = CodexExecutable::resolve(OsStr::new("codex"), &path, directory).unwrap();
        CodexInvocationConfig::new(
            executable,
            directory.to_path_buf(),
            path,
            Some(directory.join("codex-home")),
            Some(CodexModel::new("gpt-test-profile").unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn invalid_model_home_and_search_path_values_are_rejected() {
        assert!(
            CodexModel::new("  ")
                .unwrap_err()
                .to_string()
                .contains("model")
        );
        assert!(
            CodexModel::new("bad\0model")
                .unwrap_err()
                .to_string()
                .contains("NUL")
        );
        assert!(
            CodexModel::new("--skip-git-repo-check")
                .unwrap_err()
                .to_string()
                .contains("option")
        );
        let dir = tempfile::tempdir().unwrap();
        let exe = executable_file(dir.path(), "codex");
        let resolved =
            CodexExecutable::resolve(exe.as_os_str(), OsStr::new("/bin"), dir.path()).unwrap();
        let relative_home = CodexInvocationConfig::new(
            resolved.clone(),
            PathBuf::from("relative-home"),
            OsString::from("/bin"),
            None,
            None,
        )
        .unwrap_err();
        assert!(relative_home.to_string().contains("HOME"));
        let blank_path = CodexInvocationConfig::new(
            resolved,
            dir.path().to_path_buf(),
            OsString::from("  "),
            None,
            None,
        )
        .unwrap_err();
        assert!(blank_path.to_string().contains("PATH"));
        assert!(
            CodexExecutable::resolve(OsStr::new(" "), OsStr::new("/bin"), dir.path())
                .unwrap_err()
                .to_string()
                .contains("executable")
        );
        assert!(
            CodexExecutable::resolve(
                OsStr::new("missing-codex"),
                OsStr::new("/definitely/not/a/path"),
                dir.path()
            )
            .unwrap_err()
            .to_string()
            .contains("not found")
        );
    }

    #[test]
    #[cfg(unix)]
    fn executable_search_skips_nonexecutables_and_resolves_empty_path_entries() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let blocked = executable_file(&first, "codex");
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o600)).unwrap();
        let runnable = executable_file(&second, "codex");
        let path = std::env::join_paths([&first, &second]).unwrap();
        assert_eq!(
            CodexExecutable::resolve(OsStr::new("codex"), &path, dir.path())
                .unwrap()
                .as_path(),
            runnable
        );
        assert!(CodexExecutable::resolve(blocked.as_os_str(), &path, dir.path()).is_err());
        assert!(CodexExecutable::resolve(first.as_os_str(), &path, dir.path()).is_err());
        assert_eq!(
            CodexExecutable::resolve(OsStr::new("codex"), OsStr::new(":"), &second)
                .unwrap()
                .as_path(),
            runnable
        );
    }

    #[test]
    fn executable_resolution_handles_absolute_relative_and_selected_path_names() {
        let dir = tempfile::tempdir().unwrap();
        let absolute = executable_file(dir.path(), "codex-absolute");
        let selected_path = std::env::join_paths([dir.path()]).unwrap();
        let by_absolute =
            CodexExecutable::resolve(absolute.as_os_str(), &selected_path, Path::new("/")).unwrap();
        assert_eq!(by_absolute.as_path(), fs::canonicalize(&absolute).unwrap());

        let nested = dir.path().join("bin");
        fs::create_dir(&nested).unwrap();
        let relative_target = executable_file(&nested, "codex-relative");
        let by_relative = CodexExecutable::resolve(
            Path::new("bin/codex-relative").as_os_str(),
            OsStr::new("/bin"),
            dir.path(),
        )
        .unwrap();
        assert_eq!(
            by_relative.as_path(),
            fs::canonicalize(relative_target).unwrap()
        );

        let by_name = CodexExecutable::resolve(
            OsStr::new("codex-absolute"),
            &selected_path,
            Path::new("/unrelated/request/cwd"),
        )
        .unwrap();
        assert_eq!(by_name, by_absolute);

        let dot_relative = executable_file(dir.path(), "codex-dot-relative");
        let alternate_path = dir.path().join("alternate");
        fs::create_dir(&alternate_path).unwrap();
        executable_file(&alternate_path, "codex-dot-relative");
        let alternate_path = std::env::join_paths([alternate_path]).unwrap();
        let by_dot_relative = CodexExecutable::resolve(
            OsStr::new("./codex-dot-relative"),
            &alternate_path,
            dir.path(),
        )
        .unwrap();
        assert_eq!(
            by_dot_relative.as_path(),
            fs::canonicalize(dot_relative).unwrap()
        );
    }

    #[test]
    fn prepared_request_owns_schema_stdin_environment_and_workspace_until_transfer_drop() {
        let dir = tempfile::tempdir().unwrap();
        let executable_path = executable_file(dir.path(), "codex");
        let config = config(dir.path());
        let instructions = "Keep \"quoted\" text, CRLF\r\nand Unicode 雪 unchanged; {{ player_input }} is literal.";
        let prompt = "Tell the story: \"go\"\r\nλ".repeat(10_000);
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "narrative": {"type": "string", "description": "prose"},
                "summary": {"$ref": "#/$defs/Summary", "description": "summary"}
            },
            "required": ["narrative"],
            "$defs": {"Summary": {"type": "object", "properties": {"empty": {"type": "string", "default": ""}}}}
        });
        let expected_schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "narrative": {"type": "string", "description": "prose"},
                "summary": {"description": "summary", "anyOf": [{"$ref": "#/$defs/Summary"}]}
            },
            "required": ["narrative", "summary"],
            "$defs": {"Summary": {"type": "object", "properties": {"empty": {"type": "string", "default": ""}}, "required": ["empty"]}}
        });
        let request =
            PreparedCodexRequest::prepare_parts(&config, instructions, &prompt, &schema).unwrap();
        let workspace_path = request.0.workspace.path().to_path_buf();
        let schema_path = workspace_path.join("schema.json");
        assert_eq!(
            fs::read(&schema_path).unwrap(),
            serde_json::to_vec_pretty(&expected_schema).unwrap()
        );
        let stdin = format!(
            "{}\n{}\n",
            crate::generation::backend_compat::codex_cli::STDIN_PREFACE,
            serde_json::json!({"instructions": instructions, "prompt": prompt})
        );
        assert_eq!(request.0.stdin, stdin.as_bytes());
        assert_eq!(
            request.0.args,
            [
                "exec",
                "-c",
                "project_doc_max_bytes=0",
                "--model",
                "gpt-test-profile",
                "--json",
                "--ephemeral",
                "--sandbox",
                "read-only",
                "--ignore-user-config",
                "--ignore-rules",
                "--skip-git-repo-check",
                "--color",
                "never",
                "--output-schema",
                "schema.json",
                "-"
            ]
            .map(OsString::from)
        );
        assert_eq!(
            request.0.program,
            fs::canonicalize(executable_path).unwrap()
        );
        assert_eq!(
            request.0.env.vars(),
            &[
                (OsString::from("HOME"), dir.path().as_os_str().to_owned()),
                (
                    OsString::from("PATH"),
                    std::env::join_paths([dir.path()]).unwrap()
                ),
                (
                    OsString::from("CODEX_HOME"),
                    dir.path().join("codex-home").into_os_string()
                ),
            ]
        );
        assert!(request.0.env.vars().iter().all(|(key, _)| {
            ![
                "OPENAI_API_KEY",
                "CODEX_API_KEY",
                "OPENAI_BASE_URL",
                "CODEX_ORIGINATOR_OVERRIDE",
            ]
            .iter()
            .any(|excluded| key == OsStr::new(excluded))
        }));
        assert!(
            request
                .0
                .args
                .iter()
                .all(|arg| !arg.to_string_lossy().contains("Tell the story"))
        );
        assert_eq!(request.0.bounds.deadline(), Duration::from_secs(180));
        assert_eq!(request.0.bounds.max_stdout_bytes(), 1_048_576);
        assert_eq!(request.0.bounds.max_stderr_bytes(), 262_144);

        let transferred = request.into_process_spec();
        assert!(
            schema_path.is_file(),
            "schema must remain visible after ownership transfer"
        );
        assert_eq!(transferred.workspace.path(), workspace_path);
        drop(transferred);
        assert!(
            !workspace_path.exists(),
            "dropping ProcessSpec releases its request workspace"
        );
    }

    #[test]
    fn preparation_never_launches_the_selected_executable() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("codex.launched");
        let executable = dir.path().join("codex");
        fs::write(&executable, "#!/bin/sh\n: > \"$0.launched\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let config = CodexInvocationConfig::new(
            CodexExecutable::resolve(executable.as_os_str(), OsStr::new("/bin"), dir.path())
                .unwrap(),
            dir.path().to_path_buf(),
            OsString::from("/bin"),
            None,
            None,
        )
        .unwrap();
        let schema =
            serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}});
        let request =
            PreparedCodexRequest::prepare_parts(&config, "instructions", "prompt", &schema)
                .unwrap();
        assert!(!marker.exists());
        let workspace_path = request.0.workspace.path().to_path_buf();
        drop(request);
        assert!(!workspace_path.exists());
        // Positive control: the harmless fixture really can run and create its
        // marker. Preparation above must not do so. No vendor binary is used.
        assert!(
            std::process::Command::new(&executable)
                .status()
                .unwrap()
                .success()
        );
        assert!(marker.is_file());
    }

    #[test]
    fn failed_schema_write_is_located_and_workspace_drop_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        executable_file(dir.path(), "codex");
        let config = config(dir.path());
        let workspace = RequestWorkspace::new().unwrap();
        let workspace_path = workspace.path().to_path_buf();
        let schema_path = workspace.path().join("schema.json");
        fs::create_dir(&schema_path).unwrap();
        let error = PreparedCodexRequest::build_with_workspace(
            &config,
            "instructions",
            "prompt",
            serde_json::json!({"type": "object"}),
            workspace,
        )
        .expect_err("a directory at schema.json must not be treated as a file");
        assert!(error.to_string().contains("schema.json"), "{error}");
        assert!(
            !workspace_path.exists(),
            "preparation failure must release its workspace"
        );
    }

    #[test]
    fn public_generation_preparation_uses_one_rendered_request_for_prompt_and_schema() {
        let dir = tempfile::tempdir().unwrap();
        executable_file(dir.path(), "codex");
        let config = config(dir.path());
        let generation = GenerationTemplates::bundled()
            .unwrap()
            .world_request(&Brief::new("The glass desert").unwrap())
            .unwrap();
        let prepared = PreparedCodexRequest::from_generation(&config, &generation).unwrap();
        let spec = prepared.into_process_spec();
        let stdin = std::str::from_utf8(&spec.stdin).unwrap();
        let envelope: Value =
            serde_json::from_str(stdin.split_once('\n').unwrap().1.trim_end()).unwrap();
        assert_eq!(envelope["instructions"], generation.instructions().as_str());
        assert_eq!(envelope["prompt"], generation.prompt().as_str());
        let schema: Value =
            serde_json::from_slice(&fs::read(spec.workspace.path().join("schema.json")).unwrap())
                .unwrap();
        assert_eq!(
            schema["required"],
            serde_json::json!(["title", "world_description"])
        );
        assert!(schema.get("$schema").is_some());
        assert!(spec.args.windows(2).any(|pair| {
            pair[0] == OsStr::new("--model") && pair[1] == OsStr::new("gpt-test-profile")
        }));
    }
}
