//! Claude invocation configuration and owned request preparation.
//!
//! Owns the fixed-profile `claude -p` invocation frozen in
//! `reference/01-claude-cli.md` ("Supported profile (CLI 2.1.286)"): argv, an
//! explicit HOME/PATH environment, the prompt on stdin and an empty request
//! workspace as the child's cwd. It never launches anything itself.

use crate::{
    backends::{
        executable::{self, ExecutableError, ResolvedExecutable},
        process::{
            EnvPolicy, MaxStderrBytes, MaxStdoutBytes, ProcessBounds, ProcessSpec, RequestWorkspace,
        },
    },
    generation::{backend_compat::claude_cli, templates::RenderedGeneration},
};
use serde_json::Value;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeExecutable(ResolvedExecutable);

impl ClaudeExecutable {
    /// Resolve against a selected PATH and base directory before the request
    /// workspace becomes the child's cwd; see `ResolvedExecutable::resolve`.
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

/// An optional configured model selection (`--model`). Production passes none
/// by default, so the account default applies; the override has never been run
/// live and a configured value is not evidence of the model that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeModel(String);

impl ClaudeModel {
    pub fn new(value: impl Into<String>) -> Result<Self, ConfigurationError> {
        let value = value.into();
        executable::validate_string_value("model", &value)?;
        if value.starts_with('-') {
            return Err(ConfigurationError::InvalidValue {
                field: "model",
                reason: "must not begin with an option prefix",
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("invalid Claude {field}: {reason}")]
    InvalidValue {
        field: &'static str,
        reason: &'static str,
    },
    #[error("Claude executable {0:?} was not found in the selected PATH")]
    ExecutableNotFound(OsString),
    #[error("could not resolve Claude executable {path}: {source}")]
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
pub struct ClaudeInvocationConfig {
    executable: ClaudeExecutable,
    home: PathBuf,
    selected_path: OsString,
    config_dir: Option<PathBuf>,
    model: Option<ClaudeModel>,
    bounds: ProcessBounds,
}

impl ClaudeInvocationConfig {
    pub fn new(
        executable: ClaudeExecutable,
        home: PathBuf,
        selected_path: OsString,
        config_dir: Option<PathBuf>,
        model: Option<ClaudeModel>,
    ) -> Result<Self, ConfigurationError> {
        executable::validate_path("HOME", &home, true)?;
        executable::validate_os_value("PATH", &selected_path)?;
        if let Some(config_dir) = &config_dir {
            executable::validate_path("CLAUDE_CONFIG_DIR", config_dir, true)?;
        }
        Ok(Self {
            executable,
            home,
            selected_path,
            config_dir,
            model,
            // Bounds come from the 2026-10-02 live profile: world 24 s … cast
            // 39 s (60.9 s once with the advisor on 2026-09-25) against a 180 s
            // deadline; the largest stdout was the 298 KB cast, so 4 MiB leaves
            // an order of magnitude; stderr was empty in every live call.
            bounds: ProcessBounds::new(
                Duration::from_secs(180),
                MaxStdoutBytes::new(4 * 1_048_576).expect("fixed stdout bound is nonzero"),
                MaxStderrBytes::new(262_144).expect("fixed stderr bound is nonzero"),
            )
            .expect("fixed deadline and bounds are nonzero"),
        })
    }

    /// Finite transport controls only; does not expose arbitrary CLI options.
    pub fn with_bounds(mut self, bounds: ProcessBounds) -> Self {
        self.bounds = bounds;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RequestPreparationError {
    #[error("could not create Claude request workspace: {0}")]
    Workspace(#[source] std::io::Error),
    #[error("could not serialize adapted Claude schema: {0}")]
    SerializeSchema(#[source] serde_json::Error),
    /// `claude -p` has no file-based input for these values that this profile
    /// has exercised, so an argument the OS cannot pass is a preparation
    /// failure before any child exists, never a launch error.
    #[error("Claude argument {name} {problem}")]
    Argument {
        name: &'static str,
        problem: ArgumentProblem,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ArgumentProblem {
    #[error("contains a NUL byte and cannot be passed in argv")]
    ContainsNul,
    #[error("is {len} bytes; Linux accepts at most {limit} bytes per argument including its NUL")]
    TooLong { len: usize, limit: usize },
}

/// Linux `MAX_ARG_STRLEN` (32 pages): the limit for any single argv string,
/// terminating NUL included. The 2 MB `ARG_MAX` applies to the total instead.
const MAX_ARG_STRLEN: usize = 131_072;

fn check_argument(name: &'static str, value: &str) -> Result<(), RequestPreparationError> {
    let problem = if value.contains('\0') {
        ArgumentProblem::ContainsNul
    } else if value.len() + 1 > MAX_ARG_STRLEN {
        ArgumentProblem::TooLong {
            len: value.len(),
            limit: MAX_ARG_STRLEN,
        }
    } else {
        return Ok(());
    };
    Err(RequestPreparationError::Argument { name, problem })
}

/// A fully owned, fixed-profile Claude request, ready for `process::run`.
/// Keeping `ProcessSpec` private prevents callers from mutating its argv or
/// bypassing the explicit environment and resource bounds during preparation.
#[derive(Debug)]
pub struct PreparedClaudeRequest(ProcessSpec);

impl PreparedClaudeRequest {
    pub fn from_generation(
        config: &ClaudeInvocationConfig,
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
        config: &ClaudeInvocationConfig,
        instructions: &str,
        prompt: &str,
        shared_schema: &Value,
    ) -> Result<Self, RequestPreparationError> {
        let schema = serde_json::to_string(&claude_cli::adapt_schema(shared_schema.clone()))
            .map_err(RequestPreparationError::SerializeSchema)?;
        check_argument("--system-prompt", instructions)?;
        check_argument("--json-schema", &schema)?;
        let workspace = RequestWorkspace::new().map_err(RequestPreparationError::Workspace)?;
        Ok(Self(ProcessSpec {
            workspace,
            program: config.executable.as_path().to_path_buf(),
            args: fixed_arguments(config.model.as_ref(), instructions, &schema),
            env: explicit_environment(config),
            stdin: prompt.as_bytes().to_vec(),
            bounds: config.bounds,
        }))
    }

    /// Transfer the complete request and workspace to the existing supervisor.
    /// This only moves owned data; it does not launch a child.
    pub fn into_process_spec(self) -> ProcessSpec {
        self.0
    }
}

fn fixed_arguments(model: Option<&ClaudeModel>, instructions: &str, schema: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-p", "--safe-mode", "--tools", "", "--permission-prompts"]
        .into_iter()
        .map(OsString::from)
        .collect();
    args.extend(["none", "--no-session-persistence"].map(OsString::from));
    if let Some(model) = model {
        args.extend(["--model".into(), model.as_str().into()]);
    }
    args.extend([
        "--system-prompt".into(),
        instructions.into(),
        "--append-system-prompt".into(),
        claude_cli::ADVISOR_SUPPRESSION.into(),
        "--json-schema".into(),
        schema.into(),
    ]);
    args.extend(
        [
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
        ]
        .map(OsString::from),
    );
    args
}

fn explicit_environment(config: &ClaudeInvocationConfig) -> EnvPolicy {
    let environment = EnvPolicy::new()
        .set("HOME", config.home.as_os_str().to_owned())
        .set("PATH", config.selected_path.clone());
    match &config.config_dir {
        Some(path) => environment.set("CLAUDE_CONFIG_DIR", path.as_os_str().to_owned()),
        None => environment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::templates::GenerationTemplates;
    use cyoa_core::{limits::Limits, text::Brief};
    use std::fs;

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

    fn config(directory: &Path, model: Option<&str>) -> ClaudeInvocationConfig {
        executable_file(directory, "claude");
        let path = std::env::join_paths([directory]).unwrap();
        let executable = ClaudeExecutable::resolve(OsStr::new("claude"), &path, directory).unwrap();
        ClaudeInvocationConfig::new(
            executable,
            directory.to_path_buf(),
            path,
            Some(directory.join("claude-config")),
            model.map(|m| ClaudeModel::new(m).unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn invalid_model_home_config_dir_and_search_path_values_are_rejected() {
        for (value, expected) in [
            ("  ", "model"),
            ("bad\0model", "NUL"),
            ("--safe-mode", "option"),
        ] {
            let error = ClaudeModel::new(value).unwrap_err().to_string();
            assert!(error.contains(expected), "{value:?}: {error}");
        }
        let dir = tempfile::tempdir().unwrap();
        let exe = executable_file(dir.path(), "claude");
        let resolved =
            ClaudeExecutable::resolve(exe.as_os_str(), OsStr::new("/bin"), dir.path()).unwrap();
        let relative_home = ClaudeInvocationConfig::new(
            resolved.clone(),
            PathBuf::from("relative-home"),
            OsString::from("/bin"),
            None,
            None,
        )
        .unwrap_err();
        assert!(relative_home.to_string().contains("HOME"));
        let relative_config = ClaudeInvocationConfig::new(
            resolved.clone(),
            dir.path().to_path_buf(),
            OsString::from("/bin"),
            Some(PathBuf::from("relative-config")),
            None,
        )
        .unwrap_err();
        assert!(relative_config.to_string().contains("CLAUDE_CONFIG_DIR"));
        let blank_path = ClaudeInvocationConfig::new(
            resolved,
            dir.path().to_path_buf(),
            OsString::from("  "),
            None,
            None,
        )
        .unwrap_err();
        assert!(blank_path.to_string().contains("PATH"));
        let missing = ClaudeExecutable::resolve(
            OsStr::new("missing-claude"),
            OsStr::new("/definitely/not/a/path"),
            dir.path(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            missing.contains("Claude executable") && missing.contains("not found"),
            "{missing}"
        );
    }

    #[test]
    fn prepared_request_has_exact_argv_environment_stdin_and_empty_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path(), None);
        let instructions = "Keep \"quoted\" text, CRLF\r\nand Unicode 雪 unchanged; {{ player_input }} is literal.";
        let prompt = "Tell the story: \"go\"\r\nλ".repeat(10_000);
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {"narrative": {"type": "string"}, "summary": {"$ref": "#/$defs/S"}},
            "$defs": {"S": {"type": "object"}}
        });
        let request =
            PreparedClaudeRequest::prepare_parts(&config, instructions, &prompt, &schema).unwrap();
        let spec = &request.0;
        // Root key order is not a contract: serde_json's preserve_order `remove`
        // is a swap-remove, so dropping `$schema` moves the last root key first.
        // The compact form of the adapter's own output is what must be sent.
        let compact = serde_json::to_string(&claude_cli::adapt_schema(schema.clone())).unwrap();
        assert!(
            !compact.contains(": ") && !compact.contains(", "),
            "not compact: {compact}"
        );
        let expected: Vec<OsString> = [
            "-p",
            "--safe-mode",
            "--tools",
            "",
            "--permission-prompts",
            "none",
            "--no-session-persistence",
            "--system-prompt",
            instructions,
            "--append-system-prompt",
            "Do not consult the advisor tool for this task. Answer directly.",
            "--json-schema",
            &compact,
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
        ]
        .map(OsString::from)
        .to_vec();
        assert_eq!(spec.args, expected);
        assert_eq!(spec.stdin, prompt.as_bytes());
        assert!(
            spec.args
                .iter()
                .all(|a| !a.to_string_lossy().contains("Tell the story"))
        );
        assert_eq!(
            spec.program,
            fs::canonicalize(dir.path().join("claude")).unwrap()
        );
        assert_eq!(
            spec.env.vars(),
            &[
                (OsString::from("HOME"), dir.path().as_os_str().to_owned()),
                (
                    OsString::from("PATH"),
                    std::env::join_paths([dir.path()]).unwrap()
                ),
                (
                    OsString::from("CLAUDE_CONFIG_DIR"),
                    dir.path().join("claude-config").into_os_string()
                ),
            ]
        );
        for name in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
        ] {
            assert!(
                spec.env
                    .vars()
                    .iter()
                    .all(|(key, _)| key != OsStr::new(name)),
                "{name}"
            );
        }
        assert!(!spec.args.iter().any(|a| a == "--bare" || a == "--model"));
        assert_eq!(
            fs::read_dir(spec.workspace.path()).unwrap().count(),
            0,
            "empty cwd"
        );
        assert_eq!(spec.bounds.deadline(), Duration::from_secs(180));
        assert_eq!(spec.bounds.max_stdout_bytes(), 4 * 1_048_576);
        assert_eq!(spec.bounds.max_stderr_bytes(), 262_144);

        let workspace_path = spec.workspace.path().to_path_buf();
        let transferred = request.into_process_spec();
        assert!(
            workspace_path.is_dir(),
            "workspace survives ownership transfer"
        );
        drop(transferred);
        assert!(
            !workspace_path.exists(),
            "dropping ProcessSpec releases the workspace"
        );
    }

    #[test]
    fn a_configured_model_is_inserted_before_the_prompt_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path(), Some("sonnet"));
        let schema = serde_json::json!({"type": "object"});
        let request = PreparedClaudeRequest::prepare_parts(&config, "i", "p", &schema).unwrap();
        let args = &request.0.args;
        let at = args
            .iter()
            .position(|a| a == "--model")
            .expect("model flag");
        assert_eq!(args[at + 1], "sonnet");
        assert_eq!(args[at - 1], "--no-session-persistence");
        assert_eq!(args[at + 2], "--system-prompt");
    }

    #[test]
    fn schema_argument_is_the_claude_adaptation_of_the_shared_schema_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path(), None);
        let templates = GenerationTemplates::bundled().unwrap();
        let generation = templates
            .world_request(&Brief::new("The glass desert").unwrap())
            .unwrap();
        let shared = generation.schema().clone();
        assert!(
            shared.get("$schema").is_some(),
            "shared schema keeps its root key"
        );
        let spec = PreparedClaudeRequest::from_generation(&config, &generation)
            .unwrap()
            .into_process_spec();
        let at = spec.args.iter().position(|a| a == "--json-schema").unwrap();
        let sent: Value = serde_json::from_str(&spec.args[at + 1].to_string_lossy()).unwrap();
        let mut expected = shared;
        expected.as_object_mut().unwrap().remove("$schema");
        assert_eq!(sent, expected);
        assert_eq!(
            serde_json::to_string(&sent).unwrap(),
            spec.args[at + 1].to_string_lossy(),
            "argument is the canonical compact serialization"
        );
    }

    #[test]
    fn narrative_stays_the_first_property_in_the_schema_argument() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path(), None);
        let schema = GenerationTemplates::bundled()
            .unwrap()
            .turn_schema(&Limits::default())
            .unwrap();
        let spec = PreparedClaudeRequest::prepare_parts(&config, "i", "p", &schema)
            .unwrap()
            .into_process_spec();
        let at = spec.args.iter().position(|a| a == "--json-schema").unwrap();
        let sent: Value = serde_json::from_str(&spec.args[at + 1].to_string_lossy()).unwrap();
        assert_eq!(
            sent["properties"]
                .as_object()
                .unwrap()
                .keys()
                .next()
                .map(String::as_str),
            Some("narrative"),
            "streaming depends on narrative arriving first"
        );
    }

    #[test]
    fn advisor_suppression_is_local_to_the_claude_invocation() {
        let templates = GenerationTemplates::bundled().unwrap();
        let brief = Brief::new("The glass desert").unwrap();
        let outline = templates.world_request(&brief).unwrap();
        for text in [outline.instructions().as_str(), outline.prompt().as_str()] {
            assert!(
                !text.to_lowercase().contains("advisor"),
                "shared text mentions the advisor"
            );
        }
        let schema =
            serde_json::to_string(&templates.turn_schema(&Limits::default()).unwrap()).unwrap();
        assert!(!schema.to_lowercase().contains("advisor"));
        let dir = tempfile::tempdir().unwrap();
        let spec = PreparedClaudeRequest::from_generation(&config(dir.path(), None), &outline)
            .unwrap()
            .into_process_spec();
        let at = spec
            .args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .unwrap();
        assert_eq!(spec.args[at + 1], claude_cli::ADVISOR_SUPPRESSION);
        let system = spec
            .args
            .iter()
            .position(|a| a == "--system-prompt")
            .unwrap();
        assert_eq!(
            spec.args[system + 1],
            outline.instructions().as_str(),
            "shared instructions untouched"
        );
    }

    #[test]
    fn preparation_never_launches_the_selected_executable() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("claude.launched");
        let executable = dir.path().join("claude");
        fs::write(&executable, "#!/bin/sh\n: > \"$0.launched\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let config = ClaudeInvocationConfig::new(
            ClaudeExecutable::resolve(executable.as_os_str(), OsStr::new("/bin"), dir.path())
                .unwrap(),
            dir.path().to_path_buf(),
            OsString::from("/bin"),
            None,
            None,
        )
        .unwrap();
        let schema = serde_json::json!({"type": "object"});
        let request = PreparedClaudeRequest::prepare_parts(&config, "i", "p", &schema).unwrap();
        assert!(!marker.exists());
        let workspace_path = request.0.workspace.path().to_path_buf();
        drop(request);
        assert!(!workspace_path.exists());
        // Positive control: the harmless fixture really can create its marker.
        assert!(
            std::process::Command::new(&executable)
                .status()
                .unwrap()
                .success()
        );
        assert!(marker.is_file());
    }

    #[test]
    fn public_generation_preparation_uses_one_rendered_request_for_prompt_and_schema() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path(), None);
        let generation = GenerationTemplates::bundled()
            .unwrap()
            .world_request(&Brief::new("The glass desert").unwrap())
            .unwrap();
        let spec = PreparedClaudeRequest::from_generation(&config, &generation)
            .unwrap()
            .into_process_spec();
        assert_eq!(spec.stdin, generation.prompt().as_str().as_bytes());
        let at = spec
            .args
            .iter()
            .position(|a| a == "--system-prompt")
            .unwrap();
        assert_eq!(spec.args[at + 1], generation.instructions().as_str());
        let at = spec.args.iter().position(|a| a == "--json-schema").unwrap();
        let schema: Value = serde_json::from_str(&spec.args[at + 1].to_string_lossy()).unwrap();
        assert_eq!(
            schema["required"],
            serde_json::json!(["title", "world_description"])
        );
        assert!(schema.get("$schema").is_none());
    }

    #[test]
    fn nul_and_oversize_arguments_fail_preparation_before_any_launch() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path(), None);
        let schema = serde_json::json!({"type": "object"});
        // Linux rejects any single argument of MAX_ARG_STRLEN (131072) bytes or more,
        // counting the terminating NUL; 131071 content bytes is the largest accepted.
        let largest = "x".repeat(131_071);
        PreparedClaudeRequest::prepare_parts(&config, &largest, "p", &schema)
            .expect("the largest representable argument is accepted");
        let error =
            PreparedClaudeRequest::prepare_parts(&config, &"x".repeat(131_072), "p", &schema)
                .expect_err("one byte over the single-argument limit");
        let message = error.to_string();
        assert!(
            message.contains("--system-prompt") && message.contains("131072"),
            "{message}"
        );
        let nul = PreparedClaudeRequest::prepare_parts(&config, "a\0b", "p", &schema)
            .expect_err("NUL cannot be passed in argv");
        assert!(
            nul.to_string().contains("--system-prompt") && nul.to_string().contains("NUL"),
            "{nul}"
        );
        let big_schema = serde_json::json!({"type": "object", "description": "y".repeat(131_100)});
        let error = PreparedClaudeRequest::prepare_parts(&config, "i", "p", &big_schema)
            .expect_err("the schema argument is checked too");
        assert!(error.to_string().contains("--json-schema"), "{error}");
        // A large prompt is stdin, not argv, and is unaffected by the argv limit.
        PreparedClaudeRequest::prepare_parts(&config, "i", &"p".repeat(500_000), &schema)
            .expect("stdin is not subject to MAX_ARG_STRLEN");
    }
}
