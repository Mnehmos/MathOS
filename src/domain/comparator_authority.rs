use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::comparator_run::{
    COMPARATOR_RUN_COMMAND_PROFILE, COMPARATOR_RUN_COMPARATOR_COMMIT,
    COMPARATOR_RUN_COMPARATOR_TREE, COMPARATOR_RUN_GO_TOOLCHAIN, COMPARATOR_RUN_JOB,
    COMPARATOR_RUN_LANDRUN_COMMIT, COMPARATOR_RUN_LANDRUN_TREE, COMPARATOR_RUN_LEAN_TOOLCHAIN,
    COMPARATOR_RUN_LEAN4EXPORT_COMMIT, COMPARATOR_RUN_LEAN4EXPORT_TREE,
    COMPARATOR_RUN_REPORT_SCHEMA_VERSION, COMPARATOR_RUN_REPOSITORY, COMPARATOR_RUN_SOURCE_REF,
    COMPARATOR_RUN_WORKFLOW_PATH,
};
use super::schemas::ExactVersionReference;
use crate::canonical::{canonical_json, value_hash};
use crate::error::AppError;

pub const COMPARATOR_AUTHORITY_POLICY_SCHEMA_VERSION: &str = "comparator_authority_policy/1";
pub const COMPARATOR_AUTHORITY_STAGE_SCHEMA_VERSION: &str = "comparator_authority_stage/1";
pub const COMPARATOR_ATTESTATION_VERIFICATION_SCHEMA_VERSION: &str =
    "comparator_attestation_verification/1";
pub const COMPARATOR_AUTHORITY_BINDING_SCHEMA_VERSION: &str = "comparator_authority_binding/1";
pub const COMPARATOR_AUTHORITY_STATUS_SCHEMA_VERSION: &str = "comparator_authority_status/1";
pub const COMPARATOR_AUTHORITY_EVIDENCE_SCHEMA_VERSION: &str = "evidence/3";
pub const COMPARATOR_AUTHORITY_POLICY_HASH: &str =
    "3d0bf9b5bf1aba8ba9e1461f6c1105ff6e40f5cb3e34552fc26c24baede779b7";
pub const MAX_COMPARATOR_AUTHORITY_ARTIFACTS: usize = 256;
pub const MAX_COMPARATOR_AUTHORITY_MEMBER_BYTES: u64 = 512 * 1_048_576;
pub const MAX_COMPARATOR_AUTHORITY_TOTAL_BYTES: u64 = 1_024 * 1_048_576;
pub const MAX_COMPARATOR_ATTESTATION_BUNDLE_BYTES: u64 = 512 * 1_024;

pub const COMPARATOR_AUTHORITY_RUN_PATHS: [&str; 20] = [
    "comparator.bin",
    "comparator.stderr",
    "comparator.stdout",
    "lake-manifest.json",
    "lakefile.toml",
    "landlock-probe.stderr",
    "landlock-probe.stdout",
    "landrun.bin",
    "lean-toolchain",
    "lean4export.bin",
    "network-probe.py",
    "package-reprojection.json",
    "package/Challenge.lean",
    "package/Solution.lean",
    "package/config.json",
    "package/formalization.yaml",
    "package/verification.json",
    "report.json",
    "runner-script.sh",
    "systemd.properties",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorAuthorityToolPolicy {
    pub name: String,
    pub repository: String,
    pub commit: String,
    pub source_tree: String,
    pub build_toolchain: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorAuthorityPolicy {
    pub schema_version: String,
    pub policy_name: String,
    pub repository: String,
    pub repository_id: String,
    pub repository_owner_id: String,
    pub workflow_path: String,
    pub required_source_ref: String,
    pub required_job: String,
    pub required_report_schema_version: String,
    pub required_evidence_schema_version: String,
    pub required_command_profile: String,
    pub required_lean_toolchain: String,
    pub required_go_toolchain: String,
    pub tools: Vec<ComparatorAuthorityToolPolicy>,
    pub requires_github_hosted: bool,
    pub requires_protected_ref: bool,
    pub requires_non_root: bool,
    pub requires_strict_landlock: bool,
    pub requires_systemd_controls: bool,
    pub requires_network_isolation: bool,
    pub attestation_predicate_type: String,
    pub attestation_verifier_version: String,
    pub attestation_verifier_archive_sha256: String,
    pub attestation_verifier_binary_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparatorAuthorityArtifactSet {
    RunBundle,
    SourceRelease,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorAuthorityStageArtifact {
    pub artifact_set: ComparatorAuthorityArtifactSet,
    pub path: String,
    pub content_hash: String,
    pub byte_size: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorAuthorityStage {
    pub schema_version: String,
    pub report_artifact_hash: String,
    pub package_verification_hash: String,
    pub package_input_fingerprint: String,
    pub plan_artifact_hash: String,
    pub plan_byte_size: u64,
    pub source_release_manifest_hash: String,
    pub attestation_bundle_artifact_hash: String,
    pub attestation_bundle_byte_size: u64,
    pub policy_hash: String,
    pub source_formalization: ExactVersionReference,
    pub source_commit_sha: String,
    pub workflow_run_id: String,
    pub workflow_run_attempt: u32,
    pub artifacts: Vec<ComparatorAuthorityStageArtifact>,
    pub authoritative: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorAuthorityStageSnapshot {
    pub stage_hash: String,
    pub stage: ComparatorAuthorityStage,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorAttestationVerification {
    pub schema_version: String,
    pub stage_hash: String,
    pub report_artifact_hash: String,
    pub attestation_bundle_hash: String,
    pub raw_verification_hash: String,
    pub verifier_name: String,
    pub verifier_version: String,
    pub verifier_binary_sha256: String,
    pub repository: String,
    pub signer_workflow: String,
    pub certificate_identity: String,
    pub source_ref: String,
    pub source_commit_sha: String,
    pub workflow_run_id: String,
    pub workflow_run_attempt: u32,
    pub predicate_type: String,
    pub self_hosted_runners_denied: bool,
    pub verified_attestation_count: u32,
    pub verified_timestamp_count: u32,
    pub authoritative: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorIngestionReceiptSnapshot {
    pub receipt_hash: String,
    pub stage_hash: String,
    pub verification: ComparatorAttestationVerification,
    pub raw_verification_byte_size: u64,
    pub receipt_byte_size: u64,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorAuthorityBinding {
    pub schema_version: String,
    pub ingestion_receipt_hash: String,
    pub stage_hash: String,
    pub report_artifact_hash: String,
    pub attestation_bundle_artifact_hash: String,
    pub raw_verification_hash: String,
    pub policy_hash: String,
    pub plan_artifact_hash: String,
    pub source_release_manifest_hash: String,
    pub package_verification_hash: String,
    pub package_input_fingerprint: String,
    pub source_commit_sha: String,
    pub workflow_run_id: String,
    pub workflow_run_attempt: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparatorAuthorityCurrentness {
    Current,
    Stale,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparatorAuthorityStaleReason {
    FormalizationNotCurrent,
    PublicationAuthorityNotCurrent,
    FidelityNotCurrent,
    ReleaseBindingChanged,
    PlanBindingChanged,
    PackageBindingChanged,
    PolicyChanged,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorAuthorityStatus {
    pub schema_version: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    pub subject: ExactVersionReference,
    pub ingestion_receipt_hash: String,
    pub currentness: ComparatorAuthorityCurrentness,
    pub stale_reasons: Vec<ComparatorAuthorityStaleReason>,
    pub authoritative: bool,
}

impl ComparatorAuthorityPolicy {
    pub fn validate(&self) -> Result<(), AppError> {
        let expected_tools = [
            (
                "comparator",
                super::COMPARATOR_REPOSITORY,
                COMPARATOR_RUN_COMPARATOR_COMMIT,
                COMPARATOR_RUN_COMPARATOR_TREE,
                COMPARATOR_RUN_LEAN_TOOLCHAIN,
            ),
            (
                "lean4export",
                super::LEAN4EXPORT_REPOSITORY,
                COMPARATOR_RUN_LEAN4EXPORT_COMMIT,
                COMPARATOR_RUN_LEAN4EXPORT_TREE,
                COMPARATOR_RUN_LEAN_TOOLCHAIN,
            ),
            (
                "landrun",
                super::LANDRUN_REPOSITORY,
                COMPARATOR_RUN_LANDRUN_COMMIT,
                COMPARATOR_RUN_LANDRUN_TREE,
                COMPARATOR_RUN_GO_TOOLCHAIN,
            ),
        ];
        let tools_match = self.tools.len() == expected_tools.len()
            && self.tools.iter().zip(expected_tools).all(
                |(actual, (name, repository, commit, source_tree, build_toolchain))| {
                    actual.name == name
                        && actual.repository == repository
                        && actual.commit == commit
                        && actual.source_tree == source_tree
                        && actual.build_toolchain == build_toolchain
                },
            );
        if self.schema_version != COMPARATOR_AUTHORITY_POLICY_SCHEMA_VERSION
            || self.policy_name != "protected_comparator_authority"
            || self.repository != COMPARATOR_RUN_REPOSITORY
            || self.repository_id != super::COMPARATOR_RUN_REPOSITORY_ID
            || self.repository_owner_id != "193347153"
            || self.workflow_path != COMPARATOR_RUN_WORKFLOW_PATH
            || self.required_source_ref != COMPARATOR_RUN_SOURCE_REF
            || self.required_job != COMPARATOR_RUN_JOB
            || self.required_report_schema_version != COMPARATOR_RUN_REPORT_SCHEMA_VERSION
            || self.required_evidence_schema_version != COMPARATOR_AUTHORITY_EVIDENCE_SCHEMA_VERSION
            || self.required_command_profile != COMPARATOR_RUN_COMMAND_PROFILE
            || self.required_lean_toolchain != COMPARATOR_RUN_LEAN_TOOLCHAIN
            || self.required_go_toolchain != COMPARATOR_RUN_GO_TOOLCHAIN
            || !tools_match
            || !self.requires_github_hosted
            || !self.requires_protected_ref
            || !self.requires_non_root
            || !self.requires_strict_landlock
            || !self.requires_systemd_controls
            || !self.requires_network_isolation
            || self.attestation_predicate_type != "https://slsa.dev/provenance/v1"
            || self.attestation_verifier_version != "2.96.0"
            || !is_hash(&self.attestation_verifier_archive_sha256)
            || !is_hash(&self.attestation_verifier_binary_sha256)
        {
            return Err(authority_error(
                "MCL_COMPARATOR_AUTHORITY_POLICY_INVALID",
                "Comparator authority policy does not match the reviewed protected boundary",
                "Restore the committed Comparator authority policy from a verified revision.",
            ));
        }
        Ok(())
    }

    pub fn policy_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            authority_error(
                "MCL_COMPARATOR_AUTHORITY_POLICY_INVALID",
                error.to_string(),
                "Report this deterministic Comparator policy serialization defect.",
            )
        })?)
    }
}

impl ComparatorAuthorityStageArtifact {
    fn validate(&self) -> Result<(), AppError> {
        if !safe_relative_path(&self.path)
            || !is_hash(&self.content_hash)
            || self.byte_size > MAX_COMPARATOR_AUTHORITY_MEMBER_BYTES
        {
            return Err(authority_error(
                "MCL_COMPARATOR_STAGE_INVALID",
                "Comparator stage member has an unsafe path, invalid hash, or excessive size",
                "Stage only bounded regular files from the exact run bundle and frozen release.",
            ));
        }
        Ok(())
    }
}

impl ComparatorAuthorityStage {
    pub fn validate(&self) -> Result<(), AppError> {
        let ordered = self.artifacts.windows(2).all(|pair| {
            (&pair[0].artifact_set, pair[0].path.as_str())
                < (&pair[1].artifact_set, pair[1].path.as_str())
        });
        let run_paths = self
            .artifacts
            .iter()
            .filter(|entry| entry.artifact_set == ComparatorAuthorityArtifactSet::RunBundle)
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();
        let release_paths = self
            .artifacts
            .iter()
            .filter(|entry| entry.artifact_set == ComparatorAuthorityArtifactSet::SourceRelease)
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();
        let total_bytes = self
            .artifacts
            .iter()
            .try_fold(0_u64, |total, entry| total.checked_add(entry.byte_size));
        let bound_hash = |set, path: &str, expected: &str| {
            self.artifacts.iter().any(|entry| {
                entry.artifact_set == set && entry.path == path && entry.content_hash == expected
            })
        };
        if self.schema_version != COMPARATOR_AUTHORITY_STAGE_SCHEMA_VERSION
            || !is_hash(&self.report_artifact_hash)
            || !is_hash(&self.package_verification_hash)
            || !is_hash(&self.package_input_fingerprint)
            || !is_hash(&self.plan_artifact_hash)
            || self.plan_byte_size == 0
            || self.plan_byte_size > MAX_COMPARATOR_AUTHORITY_MEMBER_BYTES
            || !is_hash(&self.source_release_manifest_hash)
            || !is_hash(&self.attestation_bundle_artifact_hash)
            || self.attestation_bundle_byte_size == 0
            || self.attestation_bundle_byte_size > MAX_COMPARATOR_ATTESTATION_BUNDLE_BYTES
            || !is_hash(&self.policy_hash)
            || uuid::Uuid::parse_str(&self.source_formalization.object_id).is_err()
            || !is_hash(&self.source_formalization.version_hash)
            || !is_git_sha(&self.source_commit_sha)
            || !decimal(&self.workflow_run_id)
            || self.workflow_run_attempt == 0
            || self.artifacts.len() > MAX_COMPARATOR_AUTHORITY_ARTIFACTS
            || !ordered
            || run_paths != COMPARATOR_AUTHORITY_RUN_PATHS
            || !release_paths.contains(&"manifest.json")
            || !bound_hash(
                ComparatorAuthorityArtifactSet::RunBundle,
                "report.json",
                &self.report_artifact_hash,
            )
            || !bound_hash(
                ComparatorAuthorityArtifactSet::RunBundle,
                "package/verification.json",
                &self.package_verification_hash,
            )
            || !bound_hash(
                ComparatorAuthorityArtifactSet::SourceRelease,
                "manifest.json",
                &self.source_release_manifest_hash,
            )
            || self.artifacts.iter().any(|entry| entry.validate().is_err())
            || total_bytes.is_none_or(|total| total > MAX_COMPARATOR_AUTHORITY_TOTAL_BYTES)
            || self.authoritative
        {
            return Err(authority_error(
                "MCL_COMPARATOR_STAGE_INVALID",
                "Comparator stage does not bind one exact non-authoritative run and release closure",
                "Rebuild the stage from the exact protected run, plan, frozen release, policy, and bundle.",
            ));
        }
        Ok(())
    }

    pub fn stage_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            authority_error(
                "MCL_COMPARATOR_STAGE_INVALID",
                error.to_string(),
                "Report this deterministic Comparator stage serialization defect.",
            )
        })?)
    }
}

impl ComparatorAttestationVerification {
    pub fn validate(
        &self,
        stage: &ComparatorAuthorityStageSnapshot,
        policy: &ComparatorAuthorityPolicy,
    ) -> Result<(), AppError> {
        policy.validate()?;
        let expected_workflow = format!("{}/{}", policy.repository, policy.workflow_path);
        let expected_identity = format!(
            "https://github.com/{}/{}@{}",
            policy.repository, policy.workflow_path, policy.required_source_ref
        );
        if self.schema_version != COMPARATOR_ATTESTATION_VERIFICATION_SCHEMA_VERSION
            || self.stage_hash != stage.stage_hash
            || self.report_artifact_hash != stage.stage.report_artifact_hash
            || self.attestation_bundle_hash != stage.stage.attestation_bundle_artifact_hash
            || !is_hash(&self.raw_verification_hash)
            || self.verifier_name != "gh"
            || self.verifier_version != policy.attestation_verifier_version
            || self.verifier_binary_sha256 != policy.attestation_verifier_binary_sha256
            || self.repository != policy.repository
            || self.signer_workflow != expected_workflow
            || self.certificate_identity != expected_identity
            || self.source_ref != policy.required_source_ref
            || self.source_commit_sha != stage.stage.source_commit_sha
            || self.workflow_run_id != stage.stage.workflow_run_id
            || self.workflow_run_attempt != stage.stage.workflow_run_attempt
            || self.predicate_type != policy.attestation_predicate_type
            || !self.self_hosted_runners_denied
            || self.verified_attestation_count != 1
            || !(1..=8).contains(&self.verified_timestamp_count)
            || self.authoritative
        {
            return Err(authority_error(
                "MCL_COMPARATOR_ATTESTATION_INVALID",
                "Comparator attestation verification does not bind the exact protected stage",
                "Use only the constrained output from the pinned verifier for this stage.",
            ));
        }
        Ok(())
    }

    pub fn receipt_bytes(&self) -> Result<Vec<u8>, AppError> {
        canonical_json(&serde_json::to_value(self).map_err(|error| {
            authority_error(
                "MCL_COMPARATOR_RECEIPT_INVALID",
                error.to_string(),
                "Report this deterministic Comparator receipt serialization defect.",
            )
        })?)
    }
}

impl ComparatorAuthorityBinding {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != COMPARATOR_AUTHORITY_BINDING_SCHEMA_VERSION
            || !is_hash(&self.ingestion_receipt_hash)
            || !is_hash(&self.stage_hash)
            || !is_hash(&self.report_artifact_hash)
            || !is_hash(&self.attestation_bundle_artifact_hash)
            || !is_hash(&self.raw_verification_hash)
            || !is_hash(&self.policy_hash)
            || !is_hash(&self.plan_artifact_hash)
            || !is_hash(&self.source_release_manifest_hash)
            || !is_hash(&self.package_verification_hash)
            || !is_hash(&self.package_input_fingerprint)
            || !is_git_sha(&self.source_commit_sha)
            || !decimal(&self.workflow_run_id)
            || self.workflow_run_attempt == 0
        {
            return Err(authority_error(
                "MCL_COMPARATOR_AUTHORITY_BINDING_INVALID",
                "Comparator authority binding does not identify one exact ingested closure",
                "Use only the application-derived stage, receipt, report, bundle, policy, plan, release, and package bindings.",
            ));
        }
        Ok(())
    }
}

impl ComparatorAuthorityStatus {
    pub fn validate(&self) -> Result<(), AppError> {
        let stale = self.currentness == ComparatorAuthorityCurrentness::Stale;
        if self.schema_version != COMPARATOR_AUTHORITY_STATUS_SCHEMA_VERSION
            || uuid::Uuid::parse_str(&self.evidence_id).is_err()
            || !is_hash(&self.evidence_hash)
            || uuid::Uuid::parse_str(&self.subject.object_id).is_err()
            || !is_hash(&self.subject.version_hash)
            || !is_hash(&self.ingestion_receipt_hash)
            || stale != !self.stale_reasons.is_empty()
            || self.stale_reasons.windows(2).any(|pair| pair[0] >= pair[1])
            || !self.authoritative
        {
            return Err(authority_error(
                "MCL_COMPARATOR_AUTHORITY_STATUS_INVALID",
                "Comparator authority status is not a closed deterministic currentness result",
                "Recompute currentness from the exact retained Comparator authority chain.",
            ));
        }
        Ok(())
    }
}

pub fn committed_comparator_authority_policy() -> Result<ComparatorAuthorityPolicy, AppError> {
    let policy: ComparatorAuthorityPolicy = serde_json::from_str(include_str!(
        "../../policies/comparator-authority-1.json"
    ))
    .map_err(|error| {
        authority_error(
            "MCL_COMPARATOR_AUTHORITY_POLICY_INVALID",
            format!("committed Comparator authority policy is invalid: {error}"),
            "Restore the reviewed Comparator authority policy from a verified revision.",
        )
    })?;
    policy.validate()?;
    if policy.policy_hash()? != COMPARATOR_AUTHORITY_POLICY_HASH {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_POLICY_INVALID",
            "committed Comparator authority policy changed without updating its reviewed identity",
            "Review the policy change and update every closed Store and SQL policy pin together.",
        ));
    }
    Ok(policy)
}

pub fn comparator_authority_policy_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/comparator/authority-policy/1",
        "title": "MathOS Comparator Authority Policy v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "policy_name", "repository", "repository_id", "repository_owner_id", "workflow_path", "required_source_ref", "required_job", "required_report_schema_version", "required_evidence_schema_version", "required_command_profile", "required_lean_toolchain", "required_go_toolchain", "tools", "requires_github_hosted", "requires_protected_ref", "requires_non_root", "requires_strict_landlock", "requires_systemd_controls", "requires_network_isolation", "attestation_predicate_type", "attestation_verifier_version", "attestation_verifier_archive_sha256", "attestation_verifier_binary_sha256"],
        "properties": {
            "schema_version": {"const": COMPARATOR_AUTHORITY_POLICY_SCHEMA_VERSION},
            "policy_name": {"const": "protected_comparator_authority"},
            "repository": {"const": COMPARATOR_RUN_REPOSITORY},
            "repository_id": {"const": super::COMPARATOR_RUN_REPOSITORY_ID},
            "repository_owner_id": {"const": "193347153"},
            "workflow_path": {"const": COMPARATOR_RUN_WORKFLOW_PATH},
            "required_source_ref": {"const": COMPARATOR_RUN_SOURCE_REF},
            "required_job": {"const": COMPARATOR_RUN_JOB},
            "required_report_schema_version": {"const": COMPARATOR_RUN_REPORT_SCHEMA_VERSION},
            "required_evidence_schema_version": {"const": COMPARATOR_AUTHORITY_EVIDENCE_SCHEMA_VERSION},
            "required_command_profile": {"const": COMPARATOR_RUN_COMMAND_PROFILE},
            "required_lean_toolchain": {"const": COMPARATOR_RUN_LEAN_TOOLCHAIN},
            "required_go_toolchain": {"const": COMPARATOR_RUN_GO_TOOLCHAIN},
            "tools": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "object", "additionalProperties": false, "required": ["name", "repository", "commit", "source_tree", "build_toolchain"], "properties": {"name": {"type": "string"}, "repository": {"type": "string"}, "commit": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "source_tree": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "build_toolchain": {"type": "string"}}}},
            "requires_github_hosted": {"const": true},
            "requires_protected_ref": {"const": true},
            "requires_non_root": {"const": true},
            "requires_strict_landlock": {"const": true},
            "requires_systemd_controls": {"const": true},
            "requires_network_isolation": {"const": true},
            "attestation_predicate_type": {"const": "https://slsa.dev/provenance/v1"},
            "attestation_verifier_version": {"const": "2.96.0"},
            "attestation_verifier_archive_sha256": hash_schema(),
            "attestation_verifier_binary_sha256": hash_schema()
        }
    })
}

pub fn comparator_authority_stage_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/comparator/authority-stage/1",
        "title": "MathOS Comparator Authority Stage v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "report_artifact_hash", "package_verification_hash", "package_input_fingerprint", "plan_artifact_hash", "plan_byte_size", "source_release_manifest_hash", "attestation_bundle_artifact_hash", "attestation_bundle_byte_size", "policy_hash", "source_formalization", "source_commit_sha", "workflow_run_id", "workflow_run_attempt", "artifacts", "authoritative"],
        "properties": {
            "schema_version": {"const": COMPARATOR_AUTHORITY_STAGE_SCHEMA_VERSION},
            "report_artifact_hash": hash_schema(),
            "package_verification_hash": hash_schema(),
            "package_input_fingerprint": hash_schema(),
            "plan_artifact_hash": hash_schema(),
            "plan_byte_size": {"type": "integer", "minimum": 1, "maximum": MAX_COMPARATOR_AUTHORITY_MEMBER_BYTES},
            "source_release_manifest_hash": hash_schema(),
            "attestation_bundle_artifact_hash": hash_schema(),
            "attestation_bundle_byte_size": {"type": "integer", "minimum": 1, "maximum": MAX_COMPARATOR_ATTESTATION_BUNDLE_BYTES},
            "policy_hash": hash_schema(),
            "source_formalization": exact_ref_schema(),
            "source_commit_sha": {"type": "string", "pattern": "^[0-9a-f]{40}$"},
            "workflow_run_id": {"type": "string", "pattern": "^[1-9][0-9]*$"},
            "workflow_run_attempt": {"type": "integer", "minimum": 1},
            "artifacts": {"type": "array", "maxItems": MAX_COMPARATOR_AUTHORITY_ARTIFACTS, "items": {"type": "object", "additionalProperties": false, "required": ["artifact_set", "path", "content_hash", "byte_size"], "properties": {"artifact_set": {"enum": ["run_bundle", "source_release"]}, "path": {"type": "string", "minLength": 1, "maxLength": 512}, "content_hash": hash_schema(), "byte_size": {"type": "integer", "minimum": 0, "maximum": MAX_COMPARATOR_AUTHORITY_MEMBER_BYTES}}}},
            "authoritative": {"const": false}
        }
    })
}

pub fn comparator_attestation_verification_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/comparator/attestation-verification/1",
        "title": "MathOS Comparator Attestation Verification v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "stage_hash", "report_artifact_hash", "attestation_bundle_hash", "raw_verification_hash", "verifier_name", "verifier_version", "verifier_binary_sha256", "repository", "signer_workflow", "certificate_identity", "source_ref", "source_commit_sha", "workflow_run_id", "workflow_run_attempt", "predicate_type", "self_hosted_runners_denied", "verified_attestation_count", "verified_timestamp_count", "authoritative"],
        "properties": {
            "schema_version": {"const": COMPARATOR_ATTESTATION_VERIFICATION_SCHEMA_VERSION},
            "stage_hash": hash_schema(), "report_artifact_hash": hash_schema(), "attestation_bundle_hash": hash_schema(), "raw_verification_hash": hash_schema(),
            "verifier_name": {"const": "gh"}, "verifier_version": {"const": "2.96.0"}, "verifier_binary_sha256": hash_schema(),
            "repository": {"const": COMPARATOR_RUN_REPOSITORY}, "signer_workflow": {"type": "string"}, "certificate_identity": {"type": "string"}, "source_ref": {"const": COMPARATOR_RUN_SOURCE_REF},
            "source_commit_sha": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "workflow_run_id": {"type": "string", "pattern": "^[1-9][0-9]*$"}, "workflow_run_attempt": {"type": "integer", "minimum": 1},
            "predicate_type": {"const": "https://slsa.dev/provenance/v1"}, "self_hosted_runners_denied": {"const": true}, "verified_attestation_count": {"const": 1}, "verified_timestamp_count": {"type": "integer", "minimum": 1, "maximum": 8}, "authoritative": {"const": false}
        }
    })
}

pub fn comparator_authority_status_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/comparator/authority-status/1",
        "title": "MathOS Comparator Authority Status v1",
        "type": "object", "additionalProperties": false,
        "required": ["schema_version", "evidence_id", "evidence_hash", "subject", "ingestion_receipt_hash", "currentness", "stale_reasons", "authoritative"],
        "properties": {
            "schema_version": {"const": COMPARATOR_AUTHORITY_STATUS_SCHEMA_VERSION}, "evidence_id": {"type": "string", "format": "uuid"}, "evidence_hash": hash_schema(), "subject": exact_ref_schema(), "ingestion_receipt_hash": hash_schema(), "currentness": {"enum": ["current", "stale"]}, "stale_reasons": {"type": "array", "maxItems": 7, "items": {"enum": ["formalization_not_current", "publication_authority_not_current", "fidelity_not_current", "release_binding_changed", "plan_binding_changed", "package_binding_changed", "policy_changed"]}}, "authoritative": {"const": true}
        }
    })
}

fn hash_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

fn exact_ref_schema() -> Value {
    json!({"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": hash_schema()}})
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_git_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn decimal(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) && !value.starts_with('0')
}

fn safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value.starts_with('/')
        && !value.starts_with('\\')
        && !value.contains('\\')
        && !value.contains('\0')
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn authority_error(
    code: &'static str,
    message: impl Into<String>,
    remediation: &'static str,
) -> AppError {
    AppError::new(code, message, false, remediation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage() -> ComparatorAuthorityStage {
        let mut artifacts = COMPARATOR_AUTHORITY_RUN_PATHS
            .into_iter()
            .map(|path| ComparatorAuthorityStageArtifact {
                artifact_set: ComparatorAuthorityArtifactSet::RunBundle,
                path: path.to_owned(),
                content_hash: "a".repeat(64),
                byte_size: 1,
            })
            .collect::<Vec<_>>();
        artifacts.push(ComparatorAuthorityStageArtifact {
            artifact_set: ComparatorAuthorityArtifactSet::SourceRelease,
            path: "manifest.json".to_owned(),
            content_hash: "a".repeat(64),
            byte_size: 1,
        });
        ComparatorAuthorityStage {
            schema_version: COMPARATOR_AUTHORITY_STAGE_SCHEMA_VERSION.to_owned(),
            report_artifact_hash: "a".repeat(64),
            package_verification_hash: "a".repeat(64),
            package_input_fingerprint: "b".repeat(64),
            plan_artifact_hash: "c".repeat(64),
            plan_byte_size: 1,
            source_release_manifest_hash: "a".repeat(64),
            attestation_bundle_artifact_hash: "d".repeat(64),
            attestation_bundle_byte_size: 1,
            policy_hash: "e".repeat(64),
            source_formalization: ExactVersionReference {
                object_id: "11111111-1111-4111-8111-111111111111".to_owned(),
                version_hash: "f".repeat(64),
            },
            source_commit_sha: "1".repeat(40),
            workflow_run_id: "1".to_owned(),
            workflow_run_attempt: 1,
            artifacts,
            authoritative: false,
        }
    }

    #[test]
    fn committed_policy_and_schemas_match_the_closed_contracts() {
        let policy = committed_comparator_authority_policy().expect("committed policy");
        assert!(policy.policy_hash().expect("policy hash").len() == 64);
        for (committed, generated) in [
            (
                include_str!("../../schemas/comparator/comparator-authority-policy-1.schema.json"),
                comparator_authority_policy_schema(),
            ),
            (
                include_str!("../../schemas/comparator/comparator-authority-stage-1.schema.json"),
                comparator_authority_stage_schema(),
            ),
            (
                include_str!(
                    "../../schemas/comparator/comparator-attestation-verification-1.schema.json"
                ),
                comparator_attestation_verification_schema(),
            ),
            (
                include_str!("../../schemas/comparator/comparator-authority-status-1.schema.json"),
                comparator_authority_status_schema(),
            ),
        ] {
            let committed: Value = serde_json::from_str(committed).expect("committed schema");
            assert_eq!(committed, generated);
        }
    }

    #[test]
    fn stage_and_status_are_closed_and_non_self_authorizing() {
        let exact = stage();
        exact.validate().expect("exact stage");
        assert_eq!(exact.stage_hash().expect("stage hash").len(), 64);

        let mut reordered = exact.clone();
        reordered.artifacts.swap(0, 1);
        assert_eq!(
            reordered.validate().expect_err("reordered stage").code,
            "MCL_COMPARATOR_STAGE_INVALID"
        );
        let mut forged = exact;
        forged.authoritative = true;
        assert_eq!(
            forged.validate().expect_err("self-authorizing stage").code,
            "MCL_COMPARATOR_STAGE_INVALID"
        );

        let status = ComparatorAuthorityStatus {
            schema_version: COMPARATOR_AUTHORITY_STATUS_SCHEMA_VERSION.to_owned(),
            evidence_id: "22222222-2222-4222-8222-222222222222".to_owned(),
            evidence_hash: "1".repeat(64),
            subject: ComparatorAuthorityStage::clone(&stage()).source_formalization,
            ingestion_receipt_hash: "2".repeat(64),
            currentness: ComparatorAuthorityCurrentness::Current,
            stale_reasons: Vec::new(),
            authoritative: true,
        };
        status.validate().expect("current status");
        let mut inconsistent = status;
        inconsistent
            .stale_reasons
            .push(ComparatorAuthorityStaleReason::PolicyChanged);
        assert_eq!(
            inconsistent
                .validate()
                .expect_err("current status cannot have stale reasons")
                .code,
            "MCL_COMPARATOR_AUTHORITY_STATUS_INVALID"
        );
    }
}
