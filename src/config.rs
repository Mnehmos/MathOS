use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigFile {
    pub data_dir: PathBuf,
    pub database: PathBuf,
    pub artifacts: PathBuf,
    pub verifier: VerifierConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct VerifierConfig {
    pub lean_command: String,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
    pub concurrency: usize,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from(".mcl"),
            database: PathBuf::from("state.sqlite3"),
            artifacts: PathBuf::from("artifacts"),
            verifier: VerifierConfig::default(),
        }
    }
}

impl Default for VerifierConfig {
    fn default() -> Self {
        Self {
            lean_command: "lean".to_owned(),
            timeout_seconds: 120,
            max_output_bytes: 1_048_576,
            concurrency: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedConfig {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub database: PathBuf,
    pub artifacts: PathBuf,
    pub verifier: VerifierConfig,
}

impl ResolvedConfig {
    pub fn load(root: &Path, config_path: Option<&Path>) -> Result<Self, AppError> {
        fs::create_dir_all(root).map_err(|error| AppError::io("create root", error))?;
        let root = root
            .canonicalize()
            .map_err(|error| AppError::io("canonicalize root", error))?;
        let path = match config_path {
            Some(path) if path.is_absolute() => safe_absolute(&root, path)?,
            Some(path) => safe_join(&root, path)?,
            None => safe_join(&root, Path::new("mcl.toml"))?,
        };
        let file = if path.exists() {
            let text = fs::read_to_string(&path)
                .map_err(|error| AppError::io("read configuration", error))?;
            toml::from_str(&text).map_err(|error| {
                AppError::new(
                    "MCL_CONFIG_INVALID",
                    error.to_string(),
                    false,
                    "Correct the TOML file using mcl.toml.example as the contract.",
                )
            })?
        } else {
            ConfigFile::default()
        };
        if file.verifier.concurrency == 0 {
            return Err(AppError::new(
                "MCL_CONFIG_INVALID",
                "verifier concurrency must be at least one",
                false,
                "Set verifier.concurrency to one or greater.",
            ));
        }
        if file.verifier.lean_command != "lean" && file.verifier.lean_command != "lean.exe" {
            return Err(AppError::new(
                "MCL_CONFIG_INVALID",
                "verifier.lean_command must be the allowlisted executable `lean` or `lean.exe`",
                false,
                "Install Lean on PATH and set verifier.lean_command to the platform name.",
            ));
        }
        let data_dir = safe_join(&root, &file.data_dir)?;
        let database = safe_join(&root, &file.data_dir.join(&file.database))?;
        let artifacts = safe_join(&root, &file.data_dir.join(&file.artifacts))?;
        Ok(Self {
            root,
            config_path: path,
            data_dir,
            database,
            artifacts,
            verifier: file.verifier,
        })
    }

    pub fn write_default(&self) -> Result<(), AppError> {
        if self.config_path.exists() {
            return Ok(());
        }
        let value = ConfigFile::default();
        let text = toml::to_string_pretty(&value).map_err(|error| {
            AppError::new(
                "MCL_CONFIG_SERIALIZATION_FAILED",
                error.to_string(),
                false,
                "Report this deterministic serialization defect.",
            )
        })?;
        fs::write(&self.config_path, text)
            .map_err(|error| AppError::io("write default configuration", error))
    }
}

fn safe_join(root: &Path, relative: &Path) -> Result<PathBuf, AppError> {
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::new(
            "MCL_PATH_OUTSIDE_ROOT",
            format!("unsafe configured path: {}", relative.display()),
            false,
            "Use a relative path without parent-directory components.",
        ));
    }
    let candidate = root.join(relative);
    ensure_existing_ancestor_is_contained(root, &candidate)?;
    Ok(candidate)
}

fn safe_absolute(root: &Path, path: &Path) -> Result<PathBuf, AppError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || !path.starts_with(root)
    {
        return Err(path_outside_root(path));
    }
    ensure_existing_ancestor_is_contained(root, path)?;
    Ok(path.to_path_buf())
}

fn ensure_existing_ancestor_is_contained(root: &Path, candidate: &Path) -> Result<(), AppError> {
    let mut ancestor = candidate;
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| path_outside_root(candidate))?;
    }
    let resolved = ancestor
        .canonicalize()
        .map_err(|error| AppError::io("canonicalize configured path ancestor", error))?;
    if !resolved.starts_with(root) {
        return Err(path_outside_root(candidate));
    }
    Ok(())
}

fn path_outside_root(path: &Path) -> AppError {
    AppError::new(
        "MCL_PATH_OUTSIDE_ROOT",
        format!(
            "configured path escapes the instance root: {}",
            path.display()
        ),
        false,
        "Use a contained path without parent traversal or escaping symbolic links.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_traversal_is_rejected() {
        let result = safe_join(Path::new("/tmp/mcl"), Path::new("../escape"));
        assert_eq!(
            result.expect_err("must reject traversal").code,
            "MCL_PATH_OUTSIDE_ROOT"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::TempDir::new().expect("root");
        let outside = tempfile::TempDir::new().expect("outside");
        symlink(outside.path(), root.path().join("escape")).expect("symlink");
        let canonical_root = root.path().canonicalize().expect("canonical root");

        let result = safe_join(&canonical_root, Path::new("escape/state.sqlite3"));
        assert_eq!(
            result.expect_err("must reject symlink escape").code,
            "MCL_PATH_OUTSIDE_ROOT"
        );
    }
}
