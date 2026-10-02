//! Vendor-neutral executable resolution shared by the CLI adapters. Resolves a
//! configured executable against a selected PATH and base directory before any
//! request changes the child's working directory. Knows nothing of vendors:
//! each adapter wraps `ResolvedExecutable` and names its own errors.

use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedExecutable(PathBuf);

#[derive(Debug, thiserror::Error)]
pub enum ExecutableError {
    #[error("invalid {field}: {reason}")]
    InvalidValue {
        field: &'static str,
        reason: &'static str,
    },
    #[error("executable {0:?} was not found in the selected PATH")]
    NotFound(OsString),
    #[error("could not resolve executable {path}: {source}")]
    Resolve {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl ResolvedExecutable {
    /// Empty PATH entries mean the base directory, as in Unix PATH lookup.
    pub fn resolve(
        executable: &OsStr,
        selected_path: &OsStr,
        base_directory: &Path,
    ) -> Result<Self, ExecutableError> {
        validate_os_value("executable", executable)?;
        validate_os_value("PATH", selected_path)?;
        if !base_directory.is_absolute() {
            return Err(ExecutableError::InvalidValue {
                field: "base directory",
                reason: "must be absolute for executable resolution",
            });
        }
        let input = Path::new(executable);
        let file_name = input.file_name().ok_or(ExecutableError::InvalidValue {
            field: "executable",
            reason: "must name an executable file",
        })?;
        let has_directory_component = input.as_os_str() != file_name;
        let resolved = if input.is_absolute() {
            input.to_path_buf()
        } else if has_directory_component {
            base_directory.join(input)
        } else {
            std::env::split_paths(selected_path)
                .map(|directory| {
                    if directory.is_absolute() {
                        directory.join(input)
                    } else {
                        base_directory.join(directory).join(input)
                    }
                })
                .find(|candidate| is_executable_file(candidate))
                .ok_or_else(|| ExecutableError::NotFound(executable.into()))?
        };
        let absolute = fs::canonicalize(&resolved).map_err(|source| ExecutableError::Resolve {
            path: resolved.clone(),
            source,
        })?;
        if !is_executable_file(&absolute) {
            return Err(ExecutableError::InvalidValue {
                field: "executable",
                reason: "must be a regular file executable by the current user",
            });
        }
        Ok(Self(absolute))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

// Resolve using the caller's effective credentials. This is a preparation
// check, not a guarantee against later permission changes or invalid binaries.
fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use rustix::fs::{Access, AtFlags, CWD, accessat};
        accessat(CWD, path, Access::EXEC_OK, AtFlags::EACCESS).is_ok()
    }
    #[cfg(not(unix))]
    {
        // The process supervisor remains unsupported on these platforms.
        false
    }
}

pub fn validate_os_value(field: &'static str, value: &OsStr) -> Result<(), ExecutableError> {
    let display = value.to_string_lossy();
    if display.trim().is_empty() {
        return Err(ExecutableError::InvalidValue {
            field,
            reason: "must not be blank",
        });
    }
    if display.contains('\0') {
        return Err(ExecutableError::InvalidValue {
            field,
            reason: "must not contain NUL",
        });
    }
    Ok(())
}

pub fn validate_path(
    field: &'static str,
    value: &Path,
    require_absolute: bool,
) -> Result<(), ExecutableError> {
    validate_os_value(field, value.as_os_str())?;
    if require_absolute && !value.is_absolute() {
        return Err(ExecutableError::InvalidValue {
            field,
            reason: "must be an absolute path",
        });
    }
    Ok(())
}

/// Blank/NUL checks for a plain string setting such as a model name.
pub fn validate_string_value(field: &'static str, value: &str) -> Result<(), ExecutableError> {
    if value.trim().is_empty() {
        return Err(ExecutableError::InvalidValue {
            field,
            reason: "must not be blank",
        });
    }
    if value.contains('\0') {
        return Err(ExecutableError::InvalidValue {
            field,
            reason: "must not contain NUL",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable_file(directory: &Path, name: &str) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, b"resolution must not run this file").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        path
    }

    #[test]
    fn blank_nul_missing_and_non_file_executables_are_typed_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = OsStr::new("/bin");
        for (value, expected) in [(" ", "blank"), ("a\0b", "NUL")] {
            let error = ResolvedExecutable::resolve(OsStr::new(value), path, dir.path())
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{error}");
        }
        assert!(matches!(
            ResolvedExecutable::resolve(
                OsStr::new("no-such-tool"),
                OsStr::new("/nowhere"),
                dir.path()
            ),
            Err(ExecutableError::NotFound(_))
        ));
        assert!(matches!(
            ResolvedExecutable::resolve(dir.path().as_os_str(), path, dir.path()),
            Err(ExecutableError::InvalidValue {
                field: "executable",
                ..
            })
        ));
        assert!(matches!(
            ResolvedExecutable::resolve(OsStr::new("x"), path, Path::new("relative")),
            Err(ExecutableError::InvalidValue {
                field: "base directory",
                ..
            })
        ));
    }

    #[test]
    fn names_resolve_through_the_selected_path_to_a_canonical_file() {
        let dir = tempfile::tempdir().unwrap();
        let tool = executable_file(dir.path(), "tool");
        let selected = std::env::join_paths([dir.path()]).unwrap();
        let resolved =
            ResolvedExecutable::resolve(OsStr::new("tool"), &selected, Path::new("/")).unwrap();
        assert_eq!(resolved.as_path(), fs::canonicalize(tool).unwrap());
    }

    #[test]
    fn string_and_path_validators_reject_blank_nul_and_relative_values() {
        assert!(validate_string_value("model", "  ").is_err());
        assert!(validate_string_value("model", "a\0").is_err());
        assert!(validate_string_value("model", "sonnet").is_ok());
        assert!(validate_path("HOME", Path::new("relative"), true).is_err());
        assert!(validate_path("HOME", Path::new("/abs"), true).is_ok());
    }
}
