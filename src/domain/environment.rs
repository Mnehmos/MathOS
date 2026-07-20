use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json, to_value};

use crate::canonical::value_hash;
use crate::error::AppError;

pub const ENVIRONMENT_SCHEMA_VERSION: &str = "environment/1";
const MAX_DEPENDENCIES: usize = 256;
const MAX_IMPORTS: usize = 1_000;
const MAX_CONFIGS: usize = 32;
const MAX_TIMEOUT_SECONDS: u64 = 3_600;
const MAX_OUTPUT_BYTES: u64 = 16 * 1_048_576;
const MAX_MEMORY_BYTES: u64 = 16 * 1_073_741_824;
const MIN_MEMORY_BYTES: u64 = 64 * 1_048_576;
const MAX_CONCURRENCY: u16 = 16;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentManifest {
    pub schema_version: String,
    pub formal_system: EnvironmentFormalSystem,
    pub lean_toolchain: String,
    pub dependencies: Vec<DependencyRevision>,
    pub import_manifest: Vec<String>,
    pub project_configuration_hashes: BTreeMap<String, String>,
    pub platform: EnvironmentPlatform,
    pub trust_profile: TrustProfile,
    pub verifier_command: VerifierCommandTemplate,
    pub resource_limits: ResourceLimits,
    pub network_access: bool,
    pub working_directory_policy: WorkingDirectoryPolicy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnvironmentSnapshot {
    pub environment_hash: String,
    pub manifest: EnvironmentManifest,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentFormalSystem {
    Lean4,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyRevision {
    pub name: String,
    pub revision: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentPlatform {
    LinuxX86_64,
    WindowsX86_64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustProfile {
    Local,
    Publication,
}

impl TrustProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Publication => "publication",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierCommandTemplate {
    pub executable: VerifierExecutable,
    pub arguments: Vec<VerifierArgument>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifierExecutable {
    Lean,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum VerifierArgument {
    #[serde(rename = "{module_path}")]
    ModulePath,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    pub timeout_seconds: u64,
    pub max_output_bytes: u64,
    pub max_memory_bytes: Option<u64>,
    pub concurrency: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingDirectoryPolicy {
    TemporaryWorkspace,
}

impl EnvironmentManifest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != ENVIRONMENT_SCHEMA_VERSION {
            return Err(environment_error(
                format!(
                    "environment schema must be `{ENVIRONMENT_SCHEMA_VERSION}`, received `{}`",
                    self.schema_version
                ),
                "Use the committed environment schema version.",
            ));
        }
        validate_toolchain(&self.lean_toolchain)?;
        validate_dependencies(&self.dependencies)?;
        validate_imports(&self.import_manifest)?;
        validate_configuration_hashes(&self.project_configuration_hashes)?;
        if self.verifier_command.arguments != [VerifierArgument::ModulePath] {
            return Err(environment_error(
                "verifier command arguments must be exactly [`{module_path}`]",
                "Use the typed Lean module-path command template.",
            ));
        }
        validate_resource_limits(&self.resource_limits)?;
        if self.network_access {
            return Err(environment_error(
                "authoritative environment manifests cannot enable network access",
                "Fetch pinned dependencies before verification and set network_access to false.",
            ));
        }
        Ok(())
    }

    pub fn canonical_value(&self) -> Result<Value, AppError> {
        self.validate()?;
        to_value(self).map_err(|error| {
            AppError::new(
                "MCL_ENVIRONMENT_SERIALIZATION_FAILED",
                error.to_string(),
                false,
                "Report this deterministic environment serialization defect.",
            )
        })
    }

    pub fn environment_hash(&self) -> Result<String, AppError> {
        value_hash(&self.canonical_value()?)
    }
}

pub fn environment_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/environment/1",
        "title": "MathOS Lean Environment Manifest v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "formal_system", "lean_toolchain", "dependencies", "import_manifest", "project_configuration_hashes", "platform", "trust_profile", "verifier_command", "resource_limits", "network_access", "working_directory_policy"],
        "properties": {
            "schema_version": {"const": ENVIRONMENT_SCHEMA_VERSION},
            "formal_system": {"enum": ["lean4"]},
            "lean_toolchain": {"type": "string", "pattern": "^leanprover/lean4:v[0-9]+\\.[0-9]+\\.[0-9]+$"},
            "dependencies": {"type": "array", "maxItems": MAX_DEPENDENCIES, "items": {"type": "object", "additionalProperties": false, "required": ["name", "revision"], "properties": {"name": {"type": "string", "minLength": 1, "maxLength": 128}, "revision": {"type": "string", "pattern": "^([0-9a-f]{40}|[0-9a-f]{64})$"}}}},
            "import_manifest": {"type": "array", "maxItems": MAX_IMPORTS, "items": {"type": "string", "minLength": 1, "maxLength": 256}},
            "project_configuration_hashes": {"type": "object", "minProperties": 1, "maxProperties": MAX_CONFIGS, "additionalProperties": {"type": "string", "pattern": "^[0-9a-f]{64}$"}},
            "platform": {"enum": ["linux_x86_64", "windows_x86_64"]},
            "trust_profile": {"enum": ["local", "publication"]},
            "verifier_command": {"type": "object", "additionalProperties": false, "required": ["executable", "arguments"], "properties": {"executable": {"const": "lean"}, "arguments": {"const": ["{module_path}"]}}},
            "resource_limits": {"type": "object", "additionalProperties": false, "required": ["timeout_seconds", "max_output_bytes", "max_memory_bytes", "concurrency"], "properties": {"timeout_seconds": {"type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT_SECONDS}, "max_output_bytes": {"type": "integer", "minimum": 1, "maximum": MAX_OUTPUT_BYTES}, "max_memory_bytes": {"type": ["integer", "null"], "minimum": MIN_MEMORY_BYTES, "maximum": MAX_MEMORY_BYTES}, "concurrency": {"type": "integer", "minimum": 1, "maximum": MAX_CONCURRENCY}}},
            "network_access": {"const": false},
            "working_directory_policy": {"const": "temporary_workspace"}
        }
    })
}

fn validate_toolchain(toolchain: &str) -> Result<(), AppError> {
    let Some(version) = toolchain.strip_prefix("leanprover/lean4:v") else {
        return Err(environment_error(
            "Lean toolchain must use the pinned `leanprover/lean4:vX.Y.Z` form",
            "Pin an exact Lean release without channels or local paths.",
        ));
    };
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(environment_error(
            "Lean toolchain version must contain exactly three numeric components",
            "Use an exact release such as `leanprover/lean4:v4.32.0`.",
        ));
    }
    Ok(())
}

fn validate_dependencies(dependencies: &[DependencyRevision]) -> Result<(), AppError> {
    if dependencies.len() > MAX_DEPENDENCIES {
        return Err(environment_error(
            "environment dependency manifest exceeds its bounded size",
            "Remove unrelated dependencies from the verifier environment.",
        ));
    }
    let mut previous = None;
    let mut names = BTreeSet::new();
    for dependency in dependencies {
        if dependency.name.trim().is_empty()
            || dependency.name.len() > 128
            || !dependency
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(environment_error(
                format!("unsafe dependency name `{}`", dependency.name),
                "Use a short dependency identifier without paths or shell characters.",
            ));
        }
        validate_revision(&dependency.revision, &dependency.name)?;
        if !names.insert(dependency.name.as_str()) {
            return Err(environment_error(
                format!("duplicate dependency `{}`", dependency.name),
                "Record each dependency exactly once.",
            ));
        }
        if previous.is_some_and(|name| name > dependency.name.as_str()) {
            return Err(environment_error(
                "dependencies must be sorted by name",
                "Sort dependencies lexicographically to preserve one canonical manifest.",
            ));
        }
        previous = Some(dependency.name.as_str());
    }
    Ok(())
}

fn validate_revision(revision: &str, name: &str) -> Result<(), AppError> {
    if !matches!(revision.len(), 40 | 64)
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(environment_error(
            format!("dependency `{name}` does not use a pinned lowercase revision hash"),
            "Pin each dependency to a 40- or 64-character lowercase hexadecimal revision.",
        ));
    }
    Ok(())
}

fn validate_imports(imports: &[String]) -> Result<(), AppError> {
    if imports.len() > MAX_IMPORTS {
        return Err(environment_error(
            "import manifest exceeds its bounded size",
            "Keep only direct verifier imports in this environment manifest.",
        ));
    }
    let mut previous = None;
    let mut unique = BTreeSet::new();
    for import in imports {
        let valid = import.len() <= 256
            && import.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            });
        if !valid {
            return Err(environment_error(
                format!("unsafe Lean import `{import}`"),
                "Use a dotted Lean module name without paths, whitespace, or shell characters.",
            ));
        }
        if !unique.insert(import.as_str()) {
            return Err(environment_error(
                format!("duplicate Lean import `{import}`"),
                "Record each import exactly once.",
            ));
        }
        if previous.is_some_and(|name| name > import.as_str()) {
            return Err(environment_error(
                "imports must be sorted by module name",
                "Sort imports lexicographically to preserve one canonical manifest.",
            ));
        }
        previous = Some(import.as_str());
    }
    Ok(())
}

fn validate_configuration_hashes(configs: &BTreeMap<String, String>) -> Result<(), AppError> {
    if configs.is_empty() || configs.len() > MAX_CONFIGS {
        return Err(environment_error(
            "project configuration hashes must contain between 1 and 32 entries",
            "Hash each verifier-relevant project configuration file.",
        ));
    }
    for (name, hash) in configs {
        if name.trim().is_empty()
            || name.len() > 128
            || name.contains('/')
            || name.contains('\\')
            || name == "."
            || name == ".."
        {
            return Err(environment_error(
                format!("unsafe project configuration name `{name}`"),
                "Use a relative configuration filename without directory components.",
            ));
        }
        validate_revision(hash, name)?;
        if hash.len() != 64 {
            return Err(environment_error(
                format!("project configuration `{name}` must use SHA-256"),
                "Record the 64-character lowercase SHA-256 content hash.",
            ));
        }
    }
    Ok(())
}

fn validate_resource_limits(limits: &ResourceLimits) -> Result<(), AppError> {
    let memory_valid = limits
        .max_memory_bytes
        .is_none_or(|bytes| (MIN_MEMORY_BYTES..=MAX_MEMORY_BYTES).contains(&bytes));
    if !(1..=MAX_TIMEOUT_SECONDS).contains(&limits.timeout_seconds)
        || !(1..=MAX_OUTPUT_BYTES).contains(&limits.max_output_bytes)
        || !memory_valid
        || !(1..=MAX_CONCURRENCY).contains(&limits.concurrency)
    {
        return Err(environment_error(
            "environment resource limits are zero or outside the reviewed bounds",
            "Use timeout 1..3600 seconds, output 1..16777216 bytes, optional memory 64 MiB..16 GiB, and concurrency 1..16.",
        ));
    }
    Ok(())
}

fn environment_error(message: impl Into<String>, action: impl Into<String>) -> AppError {
    AppError::new("MCL_ENVIRONMENT_INVALID", message, false, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> EnvironmentManifest {
        EnvironmentManifest {
            schema_version: ENVIRONMENT_SCHEMA_VERSION.to_owned(),
            formal_system: EnvironmentFormalSystem::Lean4,
            lean_toolchain: "leanprover/lean4:v4.32.0".to_owned(),
            dependencies: vec![DependencyRevision {
                name: "mathlib".to_owned(),
                revision: "1".repeat(40),
            }],
            import_manifest: vec!["Mathlib".to_owned(), "Mathlib.Data.Nat.Prime".to_owned()],
            project_configuration_hashes: BTreeMap::from([
                ("lake-manifest.json".to_owned(), "2".repeat(64)),
                ("lean-toolchain".to_owned(), "3".repeat(64)),
            ]),
            platform: EnvironmentPlatform::LinuxX86_64,
            trust_profile: TrustProfile::Local,
            verifier_command: VerifierCommandTemplate {
                executable: VerifierExecutable::Lean,
                arguments: vec![VerifierArgument::ModulePath],
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
    fn exact_environment_changes_produce_new_hashes() {
        let original = manifest();
        let original_hash = original.environment_hash().expect("environment hash");
        let mut changed = original.clone();
        changed.resource_limits.timeout_seconds += 1;
        assert_ne!(
            original_hash,
            changed.environment_hash().expect("changed hash")
        );
    }

    #[test]
    fn committed_schema_and_golden_environment_identity_are_stable() {
        let committed_schema: Value = serde_json::from_str(include_str!(
            "../../schemas/environment/environment-1.schema.json"
        ))
        .expect("committed environment schema");
        assert_eq!(committed_schema, environment_schema());

        let fixture: EnvironmentManifest = serde_json::from_str(include_str!(
            "../../fixtures/environment/lean-4.32-local.json"
        ))
        .expect("golden environment fixture");
        let expected = include_str!("../../fixtures/environment/lean-4.32-local.sha256").trim();
        assert_eq!(fixture.environment_hash().expect("fixture hash"), expected);
        assert_eq!(fixture, manifest());

        let no_imports: EnvironmentManifest = serde_json::from_str(include_str!(
            "../../fixtures/environment/lean-4.32-no-imports-local.json"
        ))
        .expect("no-import publication candidate environment fixture");
        let expected_no_imports =
            include_str!("../../fixtures/environment/lean-4.32-no-imports-local.sha256").trim();
        no_imports.validate().expect("no-import fixture validates");
        assert_eq!(
            no_imports.environment_hash().expect("no-import hash"),
            expected_no_imports
        );
        assert!(no_imports.dependencies.is_empty());
        assert!(no_imports.import_manifest.is_empty());
        assert_eq!(no_imports.trust_profile, TrustProfile::Local);
    }

    #[test]
    fn unpinned_unsafe_and_unbounded_manifests_fail_closed() {
        let mut unpinned = manifest();
        unpinned.dependencies[0].revision = "main".to_owned();
        assert_eq!(
            unpinned.validate().expect_err("unpinned dependency").code,
            "MCL_ENVIRONMENT_INVALID"
        );

        let mut networked = manifest();
        networked.network_access = true;
        assert_eq!(
            networked.validate().expect_err("network access").code,
            "MCL_ENVIRONMENT_INVALID"
        );

        let mut unbounded = manifest();
        unbounded.resource_limits.timeout_seconds = 0;
        assert_eq!(
            unbounded.validate().expect_err("zero timeout").code,
            "MCL_ENVIRONMENT_INVALID"
        );

        let mut path = manifest();
        path.import_manifest = vec!["../../Secret".to_owned()];
        assert_eq!(
            path.validate().expect_err("path import").code,
            "MCL_ENVIRONMENT_INVALID"
        );

        let mut unsorted = manifest();
        unsorted.import_manifest.reverse();
        assert_eq!(
            unsorted.validate().expect_err("unsorted imports").code,
            "MCL_ENVIRONMENT_INVALID"
        );
    }

    #[test]
    fn unknown_manifest_fields_are_rejected_before_hashing() {
        let mut value = to_value(manifest()).expect("manifest value");
        value["machine_name"] = json!("hidden-local-state");
        assert!(serde_json::from_value::<EnvironmentManifest>(value).is_err());
    }
}
