use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::schemas::ExactVersionReference;
use crate::error::AppError;

pub const COMPARATOR_PACKAGE_PLAN_SCHEMA_VERSION: &str = "comparator_package_plan/1";
pub const COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION: &str =
    "comparator_package_verification/1";
pub const COMPARATOR_FORMALIZATION_SCHEMA_VERSION: &str = "comparator_formalization/1";
pub const MAX_COMPARATOR_SOURCE_BYTES: usize = 1_048_576;
pub const MAX_COMPARATOR_FILE_BYTES: u64 = 4 * 1_048_576;

pub const COMPARATOR_REPOSITORY: &str = "https://github.com/leanprover/comparator";
pub const LEAN4EXPORT_REPOSITORY: &str = "https://github.com/leanprover/lean4export";
pub const LANDRUN_REPOSITORY: &str = "https://github.com/Zouuup/landrun";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorToolPins {
    pub comparator_repository: String,
    pub comparator_commit: String,
    pub lean4export_repository: String,
    pub lean4export_commit: String,
    pub landrun_repository: String,
    pub landrun_commit: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparatorPublicationStatus {
    Draft,
    Internal,
    Published,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorFormalizationMetadata {
    pub mathematical_source: String,
    pub theorem_scope: String,
    pub ai_involvement: String,
    pub human_operators: Vec<String>,
    pub upstream_repositories: Vec<String>,
    pub publication_status: ComparatorPublicationStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorPackagePlan {
    pub schema_version: String,
    pub source_release_manifest_hash: String,
    pub formalization: ExactVersionReference,
    pub challenge_source: String,
    pub theorem_names: Vec<String>,
    pub permitted_axioms: Vec<String>,
    pub enable_nanoda: bool,
    pub tool_pins: ComparatorToolPins,
    pub formalization_metadata: ComparatorFormalizationMetadata,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparatorPackageStatus {
    Ready,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorFileBinding {
    pub path: String,
    pub content_hash: String,
    pub byte_size: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorSourceMemberBinding {
    pub path: String,
    pub content_hash: String,
    pub byte_size: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorPackageVerification {
    pub schema_version: String,
    pub status: ComparatorPackageStatus,
    pub comparator_verified: bool,
    pub authoritative: bool,
    pub source_release_manifest_hash: String,
    pub source_formalization: ExactVersionReference,
    pub source_formalization_member: ComparatorSourceMemberBinding,
    pub theorem_source: ComparatorSourceMemberBinding,
    pub dependency_manifest: ComparatorSourceMemberBinding,
    pub challenge: ComparatorFileBinding,
    pub solution: ComparatorFileBinding,
    pub config: ComparatorFileBinding,
    pub formalization: ComparatorFileBinding,
    pub declaration_name: String,
    pub lean_toolchain: String,
    pub tool_pins: ComparatorToolPins,
    pub plan_hash: String,
    pub input_fingerprint: String,
}

impl ComparatorToolPins {
    pub fn validate(&self) -> Result<(), AppError> {
        for (label, repository, expected_repository, commit) in [
            (
                "Comparator",
                &self.comparator_repository,
                COMPARATOR_REPOSITORY,
                &self.comparator_commit,
            ),
            (
                "lean4export",
                &self.lean4export_repository,
                LEAN4EXPORT_REPOSITORY,
                &self.lean4export_commit,
            ),
            (
                "landrun",
                &self.landrun_repository,
                LANDRUN_REPOSITORY,
                &self.landrun_commit,
            ),
        ] {
            if repository != expected_repository || !is_commit(commit) {
                return Err(comparator_error(
                    "MCL_COMPARATOR_PLAN_INVALID",
                    format!(
                        "{label} pin does not use the fixed repository and a lowercase 40-character Git commit"
                    ),
                    "Use the exact reviewed upstream repository and commit, never a branch or mutable tag.",
                ));
            }
        }
        Ok(())
    }
}

impl ComparatorFormalizationMetadata {
    pub fn validate(&self) -> Result<(), AppError> {
        bounded_text("mathematical source", &self.mathematical_source, 4_096)?;
        bounded_text("theorem scope", &self.theorem_scope, 8_192)?;
        bounded_text("AI involvement", &self.ai_involvement, 4_096)?;
        validate_sorted_texts("human operators", &self.human_operators, 32, 256, false)?;
        validate_sorted_texts(
            "upstream repositories",
            &self.upstream_repositories,
            32,
            512,
            true,
        )?;
        for repository in &self.upstream_repositories {
            if !valid_github_repository(repository) {
                return Err(comparator_error(
                    "MCL_COMPARATOR_PLAN_INVALID",
                    format!("upstream repository `{repository}` is not a canonical GitHub URL"),
                    "Use https://github.com/<owner>/<repository> without a suffix or fragment.",
                ));
            }
        }
        Ok(())
    }
}

impl ComparatorPackagePlan {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != COMPARATOR_PACKAGE_PLAN_SCHEMA_VERSION {
            return Err(comparator_error(
                "MCL_COMPARATOR_PLAN_INVALID",
                "Comparator plan uses an unsupported schema version",
                "Use the committed comparator_package_plan/1 contract.",
            ));
        }
        require_hash(
            &self.source_release_manifest_hash,
            "source release manifest",
            "MCL_COMPARATOR_PLAN_INVALID",
        )?;
        if uuid::Uuid::parse_str(&self.formalization.object_id).is_err() {
            return Err(comparator_error(
                "MCL_COMPARATOR_PLAN_INVALID",
                "Comparator plan formalization object is not a UUID",
                "Use the exact formalization reference from the frozen release.",
            ));
        }
        require_hash(
            &self.formalization.version_hash,
            "formalization version",
            "MCL_COMPARATOR_PLAN_INVALID",
        )?;
        if self.challenge_source.is_empty()
            || self.challenge_source.len() > MAX_COMPARATOR_SOURCE_BYTES
            || self.challenge_source.contains('\0')
            || self.challenge_source.contains('\r')
            || !self.challenge_source.ends_with('\n')
        {
            return Err(comparator_error(
                "MCL_COMPARATOR_PLAN_INVALID",
                "Challenge source must be nonempty bounded UTF-8 with LF endings and a final newline",
                "Supply a reviewed Challenge.lean no larger than 1 MiB using LF line endings.",
            ));
        }
        validate_names("theorem names", &self.theorem_names, 1, 1)?;
        validate_names("permitted axioms", &self.permitted_axioms, 0, 64)?;
        self.tool_pins.validate()?;
        self.formalization_metadata.validate()
    }
}

impl ComparatorFileBinding {
    fn validate(&self, expected_path: &str) -> Result<(), AppError> {
        if self.path != expected_path || self.byte_size > MAX_COMPARATOR_FILE_BYTES {
            return Err(comparator_error(
                "MCL_COMPARATOR_VERIFICATION_INVALID",
                format!("Comparator binding for `{expected_path}` has the wrong path or size"),
                "Restore the exact five-file Comparator package.",
            ));
        }
        require_hash(
            &self.content_hash,
            expected_path,
            "MCL_COMPARATOR_VERIFICATION_INVALID",
        )
    }
}

impl ComparatorSourceMemberBinding {
    fn validate(&self, label: &str) -> Result<(), AppError> {
        if !is_safe_relative_path(&self.path) || self.byte_size > MAX_COMPARATOR_FILE_BYTES {
            return Err(comparator_error(
                "MCL_COMPARATOR_VERIFICATION_INVALID",
                format!("{label} source member has an unsafe path or size"),
                "Restore the exact bounded member binding from the frozen release.",
            ));
        }
        require_hash(
            &self.content_hash,
            label,
            "MCL_COMPARATOR_VERIFICATION_INVALID",
        )
    }
}

impl ComparatorPackageVerification {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION
            || self.status != ComparatorPackageStatus::Ready
            || self.comparator_verified
            || self.authoritative
            || uuid::Uuid::parse_str(&self.source_formalization.object_id).is_err()
            || !is_lean_name(&self.declaration_name)
            || self.lean_toolchain.trim().is_empty()
            || self.lean_toolchain.len() > 256
        {
            return Err(comparator_error(
                "MCL_COMPARATOR_VERIFICATION_INVALID",
                "Comparator verification metadata violates the ready-only non-authority contract",
                "Rebuild the package from the closed comparator_package_verification/1 contract.",
            ));
        }
        for (label, hash) in [
            (
                "source release manifest",
                &self.source_release_manifest_hash,
            ),
            (
                "source formalization version",
                &self.source_formalization.version_hash,
            ),
            ("Comparator plan", &self.plan_hash),
            ("Comparator input fingerprint", &self.input_fingerprint),
        ] {
            require_hash(hash, label, "MCL_COMPARATOR_VERIFICATION_INVALID")?;
        }
        self.source_formalization_member
            .validate("source formalization")?;
        self.theorem_source.validate("theorem source")?;
        self.dependency_manifest.validate("dependency manifest")?;
        self.challenge.validate("Challenge.lean")?;
        self.solution.validate("Solution.lean")?;
        self.config.validate("config.json")?;
        self.formalization.validate("formalization.yaml")?;
        self.tool_pins.validate().map_err(|error| {
            comparator_error(
                "MCL_COMPARATOR_VERIFICATION_INVALID",
                error.message,
                "Restore the exact pinned Comparator tool commits.",
            )
        })
    }
}

pub fn comparator_package_plan_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/release/comparator-package-plan/1",
        "title": "MathOS Comparator Package Plan v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "source_release_manifest_hash", "formalization", "challenge_source", "theorem_names", "permitted_axioms", "enable_nanoda", "tool_pins", "formalization_metadata"],
        "properties": {
            "schema_version": {"const": COMPARATOR_PACKAGE_PLAN_SCHEMA_VERSION},
            "source_release_manifest_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "formalization": {"$ref": "#/$defs/exact_reference"},
            "challenge_source": {"type": "string", "minLength": 1, "maxLength": MAX_COMPARATOR_SOURCE_BYTES},
            "theorem_names": {"type": "array", "minItems": 1, "maxItems": 1, "uniqueItems": true, "items": {"type": "string", "minLength": 1, "maxLength": 256, "pattern": "^[A-Za-z0-9_]+(\\.[A-Za-z0-9_]+)*$"}},
            "permitted_axioms": {"type": "array", "maxItems": 64, "uniqueItems": true, "items": {"type": "string", "minLength": 1, "maxLength": 256, "pattern": "^[A-Za-z0-9_]+(\\.[A-Za-z0-9_]+)*$"}},
            "enable_nanoda": {"type": "boolean"},
            "tool_pins": {"$ref": "#/$defs/tool_pins"},
            "formalization_metadata": {"$ref": "#/$defs/formalization_metadata"}
        },
        "$defs": {
            "exact_reference": {"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}}},
            "tool_pins": {"type": "object", "additionalProperties": false, "required": ["comparator_repository", "comparator_commit", "lean4export_repository", "lean4export_commit", "landrun_repository", "landrun_commit"], "properties": {"comparator_repository": {"const": COMPARATOR_REPOSITORY}, "comparator_commit": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "lean4export_repository": {"const": LEAN4EXPORT_REPOSITORY}, "lean4export_commit": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "landrun_repository": {"const": LANDRUN_REPOSITORY}, "landrun_commit": {"type": "string", "pattern": "^[0-9a-f]{40}$"}}},
            "formalization_metadata": {"type": "object", "additionalProperties": false, "required": ["mathematical_source", "theorem_scope", "ai_involvement", "human_operators", "upstream_repositories", "publication_status"], "properties": {"mathematical_source": {"type": "string", "minLength": 1, "maxLength": 4096}, "theorem_scope": {"type": "string", "minLength": 1, "maxLength": 8192}, "ai_involvement": {"type": "string", "minLength": 1, "maxLength": 4096}, "human_operators": {"type": "array", "minItems": 1, "maxItems": 32, "uniqueItems": true, "items": {"type": "string", "minLength": 1, "maxLength": 256}}, "upstream_repositories": {"type": "array", "maxItems": 32, "uniqueItems": true, "items": {"type": "string", "minLength": 1, "maxLength": 512, "pattern": "^https://github\\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$"}}, "publication_status": {"enum": ["draft", "internal", "published"]}}}
        }
    })
}

pub fn comparator_package_verification_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/release/comparator-package-verification/1",
        "title": "MathOS Comparator Package Verification v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "status", "comparator_verified", "authoritative", "source_release_manifest_hash", "source_formalization", "source_formalization_member", "theorem_source", "dependency_manifest", "challenge", "solution", "config", "formalization", "declaration_name", "lean_toolchain", "tool_pins", "plan_hash", "input_fingerprint"],
        "properties": {
            "schema_version": {"const": COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION},
            "status": {"const": "ready"},
            "comparator_verified": {"const": false},
            "authoritative": {"const": false},
            "source_release_manifest_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "source_formalization": {"$ref": "#/$defs/exact_reference"},
            "source_formalization_member": {"$ref": "#/$defs/source_binding"},
            "theorem_source": {"$ref": "#/$defs/source_binding"},
            "dependency_manifest": {"$ref": "#/$defs/source_binding"},
            "challenge": {"$ref": "#/$defs/file_binding"},
            "solution": {"$ref": "#/$defs/file_binding"},
            "config": {"$ref": "#/$defs/file_binding"},
            "formalization": {"$ref": "#/$defs/file_binding"},
            "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256, "pattern": "^[A-Za-z0-9_]+(\\.[A-Za-z0-9_]+)*$"},
            "lean_toolchain": {"type": "string", "minLength": 1, "maxLength": 256},
            "tool_pins": {"$ref": "#/$defs/tool_pins"},
            "plan_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "input_fingerprint": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        },
        "$defs": {
            "exact_reference": {"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}}},
            "file_binding": {"type": "object", "additionalProperties": false, "required": ["path", "content_hash", "byte_size"], "properties": {"path": {"enum": ["Challenge.lean", "Solution.lean", "config.json", "formalization.yaml"]}, "content_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}, "byte_size": {"type": "integer", "minimum": 0, "maximum": MAX_COMPARATOR_FILE_BYTES}}},
            "source_binding": {"type": "object", "additionalProperties": false, "required": ["path", "content_hash", "byte_size"], "properties": {"path": {"type": "string", "minLength": 1, "maxLength": 1024}, "content_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}, "byte_size": {"type": "integer", "minimum": 0, "maximum": MAX_COMPARATOR_FILE_BYTES}}},
            "tool_pins": {"type": "object", "additionalProperties": false, "required": ["comparator_repository", "comparator_commit", "lean4export_repository", "lean4export_commit", "landrun_repository", "landrun_commit"], "properties": {"comparator_repository": {"const": COMPARATOR_REPOSITORY}, "comparator_commit": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "lean4export_repository": {"const": LEAN4EXPORT_REPOSITORY}, "lean4export_commit": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "landrun_repository": {"const": LANDRUN_REPOSITORY}, "landrun_commit": {"type": "string", "pattern": "^[0-9a-f]{40}$"}}}
        }
    })
}

fn validate_names(
    label: &str,
    values: &[String],
    minimum: usize,
    maximum: usize,
) -> Result<(), AppError> {
    if values.len() < minimum
        || values.len() > maximum
        || values.windows(2).any(|pair| pair[0] >= pair[1])
        || values.iter().any(|value| !is_lean_name(value))
    {
        return Err(comparator_error(
            "MCL_COMPARATOR_PLAN_INVALID",
            format!("{label} must be a sorted unique bounded list of Lean names"),
            "Use dotted ASCII Lean declaration names in canonical sort order.",
        ));
    }
    Ok(())
}

fn validate_sorted_texts(
    label: &str,
    values: &[String],
    maximum_items: usize,
    maximum_bytes: usize,
    allow_empty: bool,
) -> Result<(), AppError> {
    if (!allow_empty && values.is_empty())
        || values.len() > maximum_items
        || values.windows(2).any(|pair| pair[0] >= pair[1])
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > maximum_bytes)
    {
        return Err(comparator_error(
            "MCL_COMPARATOR_PLAN_INVALID",
            format!("{label} must be a sorted unique bounded list"),
            "Supply reviewed bounded values in canonical sort order.",
        ));
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() {
        return Err(comparator_error(
            "MCL_COMPARATOR_PLAN_INVALID",
            format!("{label} contain duplicate values"),
            "Keep each value at most once.",
        ));
    }
    Ok(())
}

fn bounded_text(label: &str, value: &str, maximum: usize) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(comparator_error(
            "MCL_COMPARATOR_PLAN_INVALID",
            format!("{label} must be nonempty and no larger than {maximum} bytes"),
            "Supply explicit bounded reviewed metadata.",
        ));
    }
    Ok(())
}

fn valid_github_repository(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://github.com/") else {
        return false;
    };
    let mut segments = rest.split('/');
    matches!((segments.next(), segments.next(), segments.next()), (Some(owner), Some(repository), None) if valid_repository_segment(owner) && valid_repository_segment(repository) && !repository.ends_with(".git"))
}

fn valid_repository_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1_024
        && !value.starts_with('/')
        && !value.contains('\\')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn is_lean_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn require_hash(value: &str, label: &str, code: &'static str) -> Result<(), AppError> {
    if !is_hash(value) {
        return Err(comparator_error(
            code,
            format!("{label} is not a lowercase SHA-256 identity"),
            "Use the exact identity emitted by the trusted frozen-release or package channel.",
        ));
    }
    Ok(())
}

fn comparator_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> ComparatorPackagePlan {
        ComparatorPackagePlan {
            schema_version: COMPARATOR_PACKAGE_PLAN_SCHEMA_VERSION.to_owned(),
            source_release_manifest_hash: "a".repeat(64),
            formalization: ExactVersionReference {
                object_id: "00000000-0000-4000-8000-000000000001".to_owned(),
                version_hash: "b".repeat(64),
            },
            challenge_source: "theorem Fixture.theorem : True := by\n  sorry\n".to_owned(),
            theorem_names: vec!["Fixture.theorem".to_owned()],
            permitted_axioms: vec![
                "Classical.choice".to_owned(),
                "Quot.sound".to_owned(),
                "propext".to_owned(),
            ],
            enable_nanoda: false,
            tool_pins: ComparatorToolPins {
                comparator_repository: COMPARATOR_REPOSITORY.to_owned(),
                comparator_commit: "1".repeat(40),
                lean4export_repository: LEAN4EXPORT_REPOSITORY.to_owned(),
                lean4export_commit: "2".repeat(40),
                landrun_repository: LANDRUN_REPOSITORY.to_owned(),
                landrun_commit: "3".repeat(40),
            },
            formalization_metadata: ComparatorFormalizationMetadata {
                mathematical_source: "Synthetic fixture".to_owned(),
                theorem_scope: "The proposition True.".to_owned(),
                ai_involvement: "Test fixture generation.".to_owned(),
                human_operators: vec!["MathOS test".to_owned()],
                upstream_repositories: vec!["https://github.com/Mnehmos/MathOS".to_owned()],
                publication_status: ComparatorPublicationStatus::Internal,
            },
        }
    }

    #[test]
    fn plan_is_closed_sorted_and_exactly_pinned() {
        plan().validate().expect("valid plan");

        let mut unknown = serde_json::to_value(plan()).expect("plan JSON");
        unknown["verified"] = json!(true);
        assert!(serde_json::from_value::<ComparatorPackagePlan>(unknown).is_err());

        let mut mutable_pin = plan();
        mutable_pin.tool_pins.comparator_commit = "master".to_owned();
        assert_eq!(
            mutable_pin
                .validate()
                .expect_err("mutable pin rejected")
                .code,
            "MCL_COMPARATOR_PLAN_INVALID"
        );

        let mut duplicate = plan();
        duplicate.permitted_axioms = vec!["propext".to_owned(), "propext".to_owned()];
        assert_eq!(
            duplicate
                .validate()
                .expect_err("duplicate axiom rejected")
                .code,
            "MCL_COMPARATOR_PLAN_INVALID"
        );

        let mut unsafe_name = plan();
        unsafe_name.theorem_names = vec!["Fixture.theorem;exit".to_owned()];
        assert_eq!(
            unsafe_name
                .validate()
                .expect_err("unsafe theorem name rejected")
                .code,
            "MCL_COMPARATOR_PLAN_INVALID"
        );

        let mut oversized = plan();
        oversized.challenge_source = format!("{}\n", "x".repeat(MAX_COMPARATOR_SOURCE_BYTES));
        assert_eq!(
            oversized
                .validate()
                .expect_err("oversized challenge rejected")
                .code,
            "MCL_COMPARATOR_PLAN_INVALID"
        );

        let mut substituted_repository = plan();
        substituted_repository.tool_pins.comparator_repository =
            "https://github.com/example/comparator".to_owned();
        assert_eq!(
            substituted_repository
                .validate()
                .expect_err("repository substitution rejected")
                .code,
            "MCL_COMPARATOR_PLAN_INVALID"
        );
    }

    #[test]
    fn verification_contract_can_only_describe_ready_non_authority() {
        let binding = |path: &str| ComparatorFileBinding {
            path: path.to_owned(),
            content_hash: "c".repeat(64),
            byte_size: 1,
        };
        let source = ComparatorSourceMemberBinding {
            path: "replay/Submission.lean".to_owned(),
            content_hash: "d".repeat(64),
            byte_size: 1,
        };
        let mut verification = ComparatorPackageVerification {
            schema_version: COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION.to_owned(),
            status: ComparatorPackageStatus::Ready,
            comparator_verified: false,
            authoritative: false,
            source_release_manifest_hash: "a".repeat(64),
            source_formalization: plan().formalization,
            source_formalization_member: source.clone(),
            theorem_source: source.clone(),
            dependency_manifest: ComparatorSourceMemberBinding {
                path: "replay/environment.json".to_owned(),
                ..source
            },
            challenge: binding("Challenge.lean"),
            solution: binding("Solution.lean"),
            config: binding("config.json"),
            formalization: binding("formalization.yaml"),
            declaration_name: "Fixture.theorem".to_owned(),
            lean_toolchain: "leanprover/lean4:v4.32.0".to_owned(),
            tool_pins: plan().tool_pins,
            plan_hash: "e".repeat(64),
            input_fingerprint: "f".repeat(64),
        };
        verification.validate().expect("ready package validates");

        verification.comparator_verified = true;
        assert_eq!(
            verification
                .validate()
                .expect_err("caller verification rejected")
                .code,
            "MCL_COMPARATOR_VERIFICATION_INVALID"
        );
        verification.comparator_verified = false;
        verification.authoritative = true;
        assert_eq!(
            verification
                .validate()
                .expect_err("caller authority rejected")
                .code,
            "MCL_COMPARATOR_VERIFICATION_INVALID"
        );
    }

    #[test]
    fn committed_schemas_match_the_closed_rust_contracts() {
        let plan_schema: Value = serde_json::from_str(include_str!(
            "../../schemas/release/comparator-package-plan-1.schema.json"
        ))
        .expect("committed plan schema");
        assert_eq!(plan_schema, comparator_package_plan_schema());

        let verification_schema: Value = serde_json::from_str(include_str!(
            "../../schemas/release/comparator-package-verification-1.schema.json"
        ))
        .expect("committed verification schema");
        assert_eq!(
            verification_schema,
            comparator_package_verification_schema()
        );
    }
}
