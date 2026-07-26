use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::domain::{
    DependencyRevision, EnvironmentManifest, LeanProjectBinding, VerifierExecutable,
};
use crate::error::AppError;

const MAX_PROJECT_ENTRIES: usize = 4_096;
const MAX_PROJECT_FILE_BYTES: u64 = 64 * 1_048_576;
const MAX_PROJECT_EXPANDED_BYTES: u64 = 256 * 1_048_576;

#[derive(Debug)]
pub struct MaterializedLeanProject {
    pub root: PathBuf,
    pub forbidden_source_token: Option<String>,
    pub forbidden_source_path: Option<String>,
}

#[derive(Deserialize)]
struct LakeManifest {
    packages: Vec<LakePackage>,
}

#[derive(Deserialize)]
struct LakePackage {
    name: String,
    rev: String,
    #[serde(rename = "type")]
    kind: String,
}

pub fn materialize_lean_project(
    archive_bytes: &[u8],
    binding: &LeanProjectBinding,
    expected_module_hash: &str,
    environment: &EnvironmentManifest,
    workspace: &Path,
) -> Result<MaterializedLeanProject, AppError> {
    binding.validate()?;
    environment.validate()?;
    if environment.verifier_command.executable != VerifierExecutable::Lake {
        return Err(project_error(
            "Lean project verification requires the closed Lake elaboration command",
            "Register the exact project environment with `lake env lean {module_path}`.",
        ));
    }
    require_fresh_real_directory(workspace)?;

    let mut archive = tar::Archive::new(Cursor::new(archive_bytes));
    let mut paths = BTreeSet::new();
    let mut regular_files = BTreeSet::new();
    let mut entry_count = 0_usize;
    let mut expanded_bytes = 0_u64;
    for entry in archive.entries().map_err(|error| {
        project_error(
            format!("cannot read project archive: {error}"),
            archive_action(),
        )
    })? {
        entry_count += 1;
        if entry_count > MAX_PROJECT_ENTRIES {
            return Err(project_error(
                "Lean project archive exceeds its entry bound",
                archive_action(),
            ));
        }
        let mut entry = entry.map_err(|error| {
            project_error(
                format!("invalid project archive entry: {error}"),
                archive_action(),
            )
        })?;
        let path = entry
            .path()
            .map_err(|error| {
                project_error(
                    format!("invalid project archive path: {error}"),
                    archive_action(),
                )
            })?
            .into_owned();
        validate_archive_path(&path, &binding.archive_root)?;
        let portable_path = portable_path(&path)?;
        if !paths.insert(portable_path.clone()) {
            return Err(project_error(
                format!("duplicate project archive path `{portable_path}`"),
                archive_action(),
            ));
        }
        let destination = workspace.join(&path);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|error| AppError::io("create project archive directory", error))?;
        } else if entry_type.is_file() {
            let size = entry.header().size().map_err(|error| {
                project_error(
                    format!("invalid project entry size: {error}"),
                    archive_action(),
                )
            })?;
            if size > MAX_PROJECT_FILE_BYTES {
                return Err(project_error(
                    format!("project archive file `{portable_path}` exceeds its size bound"),
                    archive_action(),
                ));
            }
            expanded_bytes = expanded_bytes.checked_add(size).ok_or_else(|| {
                project_error("project archive expanded size overflowed", archive_action())
            })?;
            if expanded_bytes > MAX_PROJECT_EXPANDED_BYTES {
                return Err(project_error(
                    "Lean project archive exceeds its expanded-size bound",
                    archive_action(),
                ));
            }
            let parent = destination
                .parent()
                .expect("validated archive file has parent");
            fs::create_dir_all(parent)
                .map_err(|error| AppError::io("create project file parent", error))?;
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)
                .map_err(|error| AppError::io("create extracted project file", error))?;
            let written = std::io::copy(&mut entry, &mut output)
                .map_err(|error| AppError::io("extract project file", error))?;
            if written != size {
                return Err(project_error(
                    format!("project archive file `{portable_path}` was truncated"),
                    archive_action(),
                ));
            }
            output
                .sync_all()
                .map_err(|error| AppError::io("sync extracted project file", error))?;
            regular_files.insert(portable_path);
        } else {
            return Err(project_error(
                format!(
                    "project archive entry `{portable_path}` is not a regular file or directory"
                ),
                "Rebuild the archive without links, devices, sparse entries, or metadata entries.",
            ));
        }
    }
    if entry_count == 0 {
        return Err(project_error(
            "Lean project archive is empty",
            archive_action(),
        ));
    }

    let root = workspace.join(&binding.archive_root);
    let root_metadata = fs::symlink_metadata(&root)
        .map_err(|error| AppError::io("inspect extracted project root", error))?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(project_error(
            "Lean project archive root is not one real directory",
            archive_action(),
        ));
    }

    let module_portable_path = format!("{}/{}", binding.archive_root, binding.module_path);
    if !regular_files.contains(&module_portable_path) {
        return Err(project_error(
            format!(
                "project archive omits bound module `{}`",
                binding.module_path
            ),
            archive_action(),
        ));
    }
    let module_bytes = fs::read(root.join(&binding.module_path))
        .map_err(|error| AppError::io("read bound project module", error))?;
    if format!("{:x}", Sha256::digest(&module_bytes)) != expected_module_hash {
        return Err(project_error(
            "project module bytes differ from the request's module artifact",
            "Rebuild the project archive from the exact registered Lean module.",
        ));
    }

    validate_project_configuration(&root, environment)?;
    validate_lake_manifest_dependencies(&root, environment)?;

    let mut forbidden_source_token = None;
    let mut forbidden_source_path = None;
    for path in regular_files {
        let relative = path
            .strip_prefix(&format!("{}/", binding.archive_root))
            .unwrap_or(&path);
        if !relative.ends_with(".lean") {
            continue;
        }
        let bytes = fs::read(root.join(relative))
            .map_err(|error| AppError::io("read project Lean source", error))?;
        if let Some(token) = super::scan_forbidden_source_token(&bytes)? {
            forbidden_source_token = Some(token);
            forbidden_source_path = Some(relative.to_owned());
            break;
        }
    }

    Ok(MaterializedLeanProject {
        root,
        forbidden_source_token,
        forbidden_source_path,
    })
}

fn validate_project_configuration(
    root: &Path,
    environment: &EnvironmentManifest,
) -> Result<(), AppError> {
    for (name, expected_hash) in &environment.project_configuration_hashes {
        let path = root.join(name);
        let metadata = fs::symlink_metadata(&path).map_err(|_| {
            project_error(
                format!("project archive omits bound configuration `{name}`"),
                archive_action(),
            )
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(project_error(
                format!("project configuration `{name}` is not one regular file"),
                archive_action(),
            ));
        }
        let bytes =
            fs::read(&path).map_err(|error| AppError::io("read project configuration", error))?;
        if format!("{:x}", Sha256::digest(&bytes)) != *expected_hash {
            return Err(project_error(
                format!("project configuration `{name}` differs from its environment hash"),
                "Use the exact configuration files bound by the registered environment.",
            ));
        }
    }
    if !environment
        .project_configuration_hashes
        .contains_key("lean-toolchain")
        || !environment
            .project_configuration_hashes
            .contains_key("lake-manifest.json")
        || (!environment
            .project_configuration_hashes
            .contains_key("lakefile.lean")
            && !environment
                .project_configuration_hashes
                .contains_key("lakefile.toml"))
    {
        return Err(project_error(
            "Lake project environment does not bind its toolchain, manifest, and lakefile",
            "Register SHA-256 identities for lean-toolchain, lake-manifest.json, and the exact lakefile.",
        ));
    }
    let toolchain = fs::read_to_string(root.join("lean-toolchain"))
        .map_err(|error| AppError::io("read project lean-toolchain", error))?;
    if toolchain.trim_end_matches(['\r', '\n']) != environment.lean_toolchain {
        return Err(project_error(
            "project lean-toolchain differs from the registered environment",
            "Use the exact pinned Lean toolchain in both project and environment.",
        ));
    }
    Ok(())
}

fn validate_lake_manifest_dependencies(
    root: &Path,
    environment: &EnvironmentManifest,
) -> Result<(), AppError> {
    let bytes = fs::read(root.join("lake-manifest.json"))
        .map_err(|error| AppError::io("read project Lake manifest", error))?;
    let manifest: LakeManifest = serde_json::from_slice(&bytes).map_err(|error| {
        project_error(
            format!("project Lake manifest is invalid JSON: {error}"),
            archive_action(),
        )
    })?;
    let mut observed = BTreeMap::new();
    for package in manifest.packages {
        if package.kind != "git"
            || package.name.is_empty()
            || observed.insert(package.name, package.rev).is_some()
        {
            return Err(project_error(
                "project Lake manifest contains a non-git or duplicate dependency",
                "Use one exact git revision for every locked dependency.",
            ));
        }
    }
    let expected = environment
        .dependencies
        .iter()
        .map(|dependency: &DependencyRevision| {
            (dependency.name.clone(), dependency.revision.clone())
        })
        .collect::<BTreeMap<_, _>>();
    if observed != expected {
        return Err(project_error(
            "project Lake manifest dependencies differ from the registered environment",
            "Register the complete exact dependency lock before verification.",
        ));
    }
    Ok(())
}

fn require_fresh_real_directory(path: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io("inspect project extraction workspace", error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(project_error(
            "project extraction workspace is not one real directory",
            "Use a fresh verifier-created temporary workspace.",
        ));
    }
    if fs::read_dir(path)
        .map_err(|error| AppError::io("read project extraction workspace", error))?
        .next()
        .is_some()
    {
        return Err(project_error(
            "project extraction workspace is not empty",
            "Use a fresh verifier-created temporary workspace.",
        ));
    }
    Ok(())
}

fn validate_archive_path(path: &Path, archive_root: &str) -> Result<(), AppError> {
    let mut components = path.components();
    let Some(Component::Normal(root)) = components.next() else {
        return Err(project_error(
            "project archive path is not relative",
            archive_action(),
        ));
    };
    if root != archive_root {
        return Err(project_error(
            "project archive contains content outside its bound root",
            archive_action(),
        ));
    }
    for component in components {
        let Component::Normal(component) = component else {
            return Err(project_error(
                "project archive path contains traversal or a platform prefix",
                archive_action(),
            ));
        };
        let value = component
            .to_str()
            .ok_or_else(|| project_error("project archive path is not UTF-8", archive_action()))?;
        if value.is_empty()
            || value.len() > 128
            || matches!(
                value,
                "." | ".." | ".git" | ".lake" | super::MATHLIB_CACHE_DIRECTORY
            )
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'\'')
            })
        {
            return Err(project_error(
                format!("unsafe project archive path component `{value}`"),
                archive_action(),
            ));
        }
    }
    Ok(())
}

fn portable_path(path: &Path) -> Result<String, AppError> {
    let values = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_owned).ok_or_else(|| {
                project_error("project archive path is not UTF-8", archive_action())
            }),
            _ => Err(project_error(
                "project archive path is not portable",
                archive_action(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let path = values.join("/");
    if path.len() > 512 {
        return Err(project_error(
            "project archive path exceeds its length bound",
            archive_action(),
        ));
    }
    Ok(path)
}

fn archive_action() -> &'static str {
    "Create a deterministic tar containing only regular files and directories below the bound project root."
}

fn project_error(message: impl Into<String>, action: impl Into<String>) -> AppError {
    AppError::new("MCL_VERIFIER_PROJECT_INVALID", message, false, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::domain::environment::EnvironmentFormalSystem;
    use crate::domain::{
        EnvironmentPlatform, ResourceLimits, TrustProfile, VerifierArgument,
        VerifierCommandTemplate, WorkingDirectoryPolicy,
    };

    fn fixture_files() -> BTreeMap<&'static str, Vec<u8>> {
        BTreeMap::from([
            (
                "project/Final.lean",
                b"theorem Project.truth : True := by trivial\n".to_vec(),
            ),
            (
                "project/lake-manifest.json",
                format!(
                    "{{\"packages\":[{{\"name\":\"mathlib\",\"rev\":\"{}\",\"type\":\"git\"}}]}}",
                    "1".repeat(40)
                )
                .into_bytes(),
            ),
            (
                "project/lakefile.lean",
                b"import Lake\nopen Lake DSL\npackage Project\nlean_lib Project\n".to_vec(),
            ),
            (
                "project/lean-toolchain",
                b"leanprover/lean4:v4.32.0-rc1\n".to_vec(),
            ),
        ])
    }

    fn archive(files: &BTreeMap<&str, Vec<u8>>) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).expect("fixture path");
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append(&header, bytes.as_slice())
                .expect("append fixture");
        }
        builder.into_inner().expect("fixture archive")
    }

    fn archive_with_raw_path(path: &str, entry_type: tar::EntryType) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o644);
        header.set_entry_type(entry_type);
        if entry_type.is_symlink() {
            header
                .set_link_name("project/Final.lean")
                .expect("fixture link target");
        }
        let name = path.as_bytes();
        assert!(name.len() < 100);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name);
        header.set_cksum();
        builder
            .append(&header, std::io::empty())
            .expect("append raw fixture");
        builder.into_inner().expect("raw fixture archive")
    }

    fn binding() -> LeanProjectBinding {
        LeanProjectBinding {
            archive_artifact_hash: "a".repeat(64),
            archive_root: "project".to_owned(),
            module_path: "Final.lean".to_owned(),
        }
    }

    fn environment(files: &BTreeMap<&str, Vec<u8>>) -> EnvironmentManifest {
        let hash = |path: &str| {
            format!(
                "{:x}",
                Sha256::digest(files.get(path).expect("fixture configuration"))
            )
        };
        EnvironmentManifest {
            schema_version: crate::domain::environment::ENVIRONMENT_SCHEMA_VERSION.to_owned(),
            formal_system: EnvironmentFormalSystem::Lean4,
            lean_toolchain: "leanprover/lean4:v4.32.0-rc1".to_owned(),
            dependencies: vec![DependencyRevision {
                name: "mathlib".to_owned(),
                revision: "1".repeat(40),
            }],
            dependency_preparation: None,
            import_manifest: vec!["Final".to_owned()],
            project_configuration_hashes: BTreeMap::from([
                (
                    "lake-manifest.json".to_owned(),
                    hash("project/lake-manifest.json"),
                ),
                ("lakefile.lean".to_owned(), hash("project/lakefile.lean")),
                ("lean-toolchain".to_owned(), hash("project/lean-toolchain")),
            ]),
            platform: if cfg!(windows) {
                EnvironmentPlatform::WindowsX86_64
            } else {
                EnvironmentPlatform::LinuxX86_64
            },
            trust_profile: TrustProfile::Local,
            verifier_command: VerifierCommandTemplate {
                executable: VerifierExecutable::Lake,
                arguments: vec![
                    VerifierArgument::Env,
                    VerifierArgument::Lean,
                    VerifierArgument::ModulePath,
                ],
            },
            resource_limits: ResourceLimits {
                timeout_seconds: 120,
                max_output_bytes: 1_048_576,
                max_memory_bytes: None,
                concurrency: 1,
            },
            network_access: false,
            working_directory_policy: WorkingDirectoryPolicy::TemporaryWorkspace,
        }
    }

    #[test]
    fn exact_project_materializes_and_binds_module_config_and_dependencies() {
        let files = fixture_files();
        let module_hash = format!(
            "{:x}",
            Sha256::digest(files.get("project/Final.lean").expect("module"))
        );
        let workspace = tempfile::TempDir::new().expect("workspace");
        let materialized = materialize_lean_project(
            &archive(&files),
            &binding(),
            &module_hash,
            &environment(&files),
            workspace.path(),
        )
        .expect("project materializes");
        assert_eq!(materialized.root, workspace.path().join("project"));
        assert_eq!(materialized.forbidden_source_token, None);
        assert_eq!(materialized.forbidden_source_path, None);
    }

    #[test]
    fn project_rejects_changed_module_locked_dependency_and_hidden_cache() {
        let files = fixture_files();
        let module_hash = "f".repeat(64);
        let workspace = tempfile::TempDir::new().expect("workspace");
        assert_eq!(
            materialize_lean_project(
                &archive(&files),
                &binding(),
                &module_hash,
                &environment(&files),
                workspace.path(),
            )
            .expect_err("changed module rejected")
            .code,
            "MCL_VERIFIER_PROJECT_INVALID"
        );

        let mut changed_dependency = files.clone();
        changed_dependency.insert(
            "project/lake-manifest.json",
            format!(
                "{{\"packages\":[{{\"name\":\"mathlib\",\"rev\":\"{}\",\"type\":\"git\"}}]}}",
                "2".repeat(40)
            )
            .into_bytes(),
        );
        let module_hash = format!(
            "{:x}",
            Sha256::digest(
                changed_dependency
                    .get("project/Final.lean")
                    .expect("module"),
            )
        );
        let workspace = tempfile::TempDir::new().expect("workspace");
        assert_eq!(
            materialize_lean_project(
                &archive(&changed_dependency),
                &binding(),
                &module_hash,
                &environment(&changed_dependency),
                workspace.path(),
            )
            .expect_err("changed lock rejected")
            .code,
            "MCL_VERIFIER_PROJECT_INVALID"
        );

        for hidden_path in [
            "project/.lake/build.olean",
            "project/.mathlib-cache/untrusted.tar",
        ] {
            let mut hidden_cache = files.clone();
            hidden_cache.insert(hidden_path, b"untrusted cache".to_vec());
            let workspace = tempfile::TempDir::new().expect("workspace");
            assert_eq!(
                materialize_lean_project(
                    &archive(&hidden_cache),
                    &binding(),
                    &format!(
                        "{:x}",
                        Sha256::digest(hidden_cache.get("project/Final.lean").expect("module"))
                    ),
                    &environment(&files),
                    workspace.path(),
                )
                .expect_err("hidden cache rejected")
                .code,
                "MCL_VERIFIER_PROJECT_INVALID"
            );
        }
    }

    #[test]
    fn project_rejects_traversal_and_links_before_extraction() {
        let files = fixture_files();
        let environment = environment(&files);
        let module_hash = format!(
            "{:x}",
            Sha256::digest(files.get("project/Final.lean").expect("module"))
        );
        for (archive, label) in [
            (
                archive_with_raw_path("project/../escape", tar::EntryType::Regular),
                "traversal",
            ),
            (
                archive_with_raw_path("project/Final.lean", tar::EntryType::Symlink),
                "symbolic link",
            ),
        ] {
            let workspace = tempfile::TempDir::new().expect("workspace");
            assert_eq!(
                materialize_lean_project(
                    &archive,
                    &binding(),
                    &module_hash,
                    &environment,
                    workspace.path(),
                )
                .expect_err(label)
                .code,
                "MCL_VERIFIER_PROJECT_INVALID"
            );
            assert!(
                fs::read_dir(workspace.path())
                    .expect("read untouched workspace")
                    .next()
                    .is_none(),
                "{label} was rejected before extraction"
            );
        }
    }

    #[test]
    fn project_scans_every_retained_lean_source() {
        let mut files = fixture_files();
        files.insert(
            "project/BH/Unsafe.lean",
            b"axiom smuggledTruth : False\n".to_vec(),
        );
        let module_hash = format!(
            "{:x}",
            Sha256::digest(files.get("project/Final.lean").expect("module"))
        );
        let workspace = tempfile::TempDir::new().expect("workspace");
        let materialized = materialize_lean_project(
            &archive(&files),
            &binding(),
            &module_hash,
            &environment(&files),
            workspace.path(),
        )
        .expect("safe archive structure materializes");
        assert_eq!(
            materialized.forbidden_source_token.as_deref(),
            Some("axiom")
        );
        assert_eq!(
            materialized.forbidden_source_path.as_deref(),
            Some("BH/Unsafe.lean")
        );
    }
}
