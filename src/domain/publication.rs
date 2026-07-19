use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::schemas::ExactVersionReference;
use crate::error::AppError;

pub const PUBLICATION_POLICY_SCHEMA_VERSION: &str = "publication_policy/1";
pub const PUBLICATION_REQUEST_SCHEMA_VERSION: &str = "publication_request/1";
pub const PUBLICATION_REPORT_SCHEMA_VERSION: &str = "publication_report/1";
pub const PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION: &str =
    "publication_attestation_verification/1";
const MAX_AXIOMS: usize = 256;
const MAX_ARTIFACTS: usize = 256;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationPolicy {
    pub schema_version: String,
    pub policy_name: String,
    pub repository: String,
    pub workflow_path: String,
    pub required_source_ref: String,
    pub required_runner_environment: PublicationRunnerEnvironment,
    pub required_lean_toolchain: String,
    pub allowed_axioms: Vec<String>,
    pub requires_clean_checkout: bool,
    pub requires_dependency_closure: bool,
    pub requires_network_isolation: bool,
    pub requires_memory_limit: bool,
    pub attestation_predicate_type: String,
    pub attestation_action_sha: String,
    pub artifact_upload_action_sha: String,
    pub attestation_verifier_version: String,
    pub attestation_verifier_archive_sha256: String,
    pub attestation_verifier_binary_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationRunnerEnvironment {
    GithubHosted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationOutcome {
    Proof,
    Refutation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationClassification {
    Passed,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRequest {
    pub schema_version: String,
    pub subject: ExactVersionReference,
    pub outcome: PublicationOutcome,
    pub diagnostic_evidence_id: String,
    pub diagnostic_evidence_hash: String,
    pub proof_closure_evidence_id: String,
    pub proof_closure_evidence_hash: String,
    pub axiom_audit_evidence_id: String,
    pub axiom_audit_evidence_hash: String,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
    pub policy_hash: String,
    pub source_commit_sha: String,
    pub source_tree_sha: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationReport {
    pub schema_version: String,
    pub request_hash: String,
    pub request: PublicationRequest,
    pub classification: PublicationClassification,
    pub repository: String,
    pub workflow_path: String,
    pub source_ref: String,
    pub workflow_run_id: u64,
    pub workflow_run_attempt: u32,
    pub runner_environment: PublicationRunnerEnvironment,
    pub observed_lean_toolchain: String,
    pub observed_axioms: Vec<String>,
    pub retained_artifact_hashes: Vec<String>,
    pub clean_checkout: bool,
    pub dependency_closure_complete: bool,
    pub network_isolation_enforced: bool,
    pub memory_limit_enforced: bool,
    pub authoritative: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationAttestationVerification {
    pub schema_version: String,
    pub report_content_hash: String,
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
    pub predicate_type: String,
    pub self_hosted_runners_denied: bool,
    pub verified_attestation_count: u32,
    pub verified_timestamp_count: u32,
    pub authoritative: bool,
}

impl PublicationPolicy {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != PUBLICATION_POLICY_SCHEMA_VERSION
            || !is_identifier(&self.policy_name, 128)
            || !is_repository(&self.repository)
            || !is_workflow_path(&self.workflow_path)
            || !self.required_source_ref.starts_with("refs/heads/")
            || !is_lean_toolchain(&self.required_lean_toolchain)
            || !is_sorted_unique(&self.allowed_axioms, MAX_AXIOMS)
            || self.allowed_axioms.iter().any(|axiom| !is_lean_name(axiom))
            || !self.requires_clean_checkout
            || !self.requires_dependency_closure
            || !self.requires_network_isolation
            || !self.requires_memory_limit
            || self.attestation_predicate_type != "https://slsa.dev/provenance/v1"
            || !is_git_sha(&self.attestation_action_sha)
            || !is_git_sha(&self.artifact_upload_action_sha)
            || !is_semver(&self.attestation_verifier_version)
            || !is_hash(&self.attestation_verifier_archive_sha256)
            || !is_hash(&self.attestation_verifier_binary_sha256)
        {
            return Err(publication_error(
                "MCL_PUBLICATION_POLICY_INVALID",
                "publication policy does not satisfy the closed authority contract",
                "Use the committed protected-workflow policy and pinned action identities.",
            ));
        }
        Ok(())
    }

    pub fn policy_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        hash_serializable(self, "MCL_PUBLICATION_POLICY_INVALID")
    }
}

impl PublicationRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != PUBLICATION_REQUEST_SCHEMA_VERSION
            || uuid::Uuid::parse_str(&self.subject.object_id).is_err()
            || !is_hash(&self.subject.version_hash)
            || uuid::Uuid::parse_str(&self.diagnostic_evidence_id).is_err()
            || uuid::Uuid::parse_str(&self.proof_closure_evidence_id).is_err()
            || uuid::Uuid::parse_str(&self.axiom_audit_evidence_id).is_err()
            || !is_hash(&self.diagnostic_evidence_hash)
            || !is_hash(&self.proof_closure_evidence_hash)
            || !is_hash(&self.axiom_audit_evidence_hash)
            || !is_hash(&self.environment_hash)
            || !is_hash(&self.module_artifact_hash)
            || !is_lean_name(&self.declaration_name)
            || self.declaration_name.len() > 256
            || !is_hash(&self.policy_hash)
            || !is_git_sha(&self.source_commit_sha)
            || !is_git_sha(&self.source_tree_sha)
        {
            return Err(publication_error(
                "MCL_PUBLICATION_REQUEST_INVALID",
                "publication request does not bind one closed exact verification input",
                "Use exact canonical evidence, formalization, environment, artifact, policy, commit, and tree identities.",
            ));
        }
        Ok(())
    }

    pub fn request_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        hash_serializable(self, "MCL_PUBLICATION_REQUEST_INVALID")
    }
}

impl PublicationReport {
    pub fn validate_candidate(&self, policy: &PublicationPolicy) -> Result<(), AppError> {
        self.request.validate()?;
        policy.validate()?;
        let expected_unallowed_axiom = self
            .observed_axioms
            .iter()
            .any(|axiom| policy.allowed_axioms.binary_search(axiom).is_err());
        let passed = self.classification != PublicationClassification::Passed
            || (self.clean_checkout
                && self.dependency_closure_complete
                && self.network_isolation_enforced
                && self.memory_limit_enforced
                && !expected_unallowed_axiom);
        if self.schema_version != PUBLICATION_REPORT_SCHEMA_VERSION
            || self.request_hash != self.request.request_hash()?
            || self.request.policy_hash != policy.policy_hash()?
            || self.repository != policy.repository
            || self.workflow_path != policy.workflow_path
            || self.source_ref != policy.required_source_ref
            || self.runner_environment != policy.required_runner_environment
            || self.observed_lean_toolchain != policy.required_lean_toolchain
            || self.workflow_run_id == 0
            || self.workflow_run_attempt == 0
            || !is_sorted_unique(&self.observed_axioms, MAX_AXIOMS)
            || self
                .observed_axioms
                .iter()
                .any(|axiom| !is_lean_name(axiom))
            || !is_sorted_unique(&self.retained_artifact_hashes, MAX_ARTIFACTS)
            || self.retained_artifact_hashes.is_empty()
            || self
                .retained_artifact_hashes
                .iter()
                .any(|hash| !is_hash(hash))
            || self.authoritative
            || !passed
        {
            return Err(publication_error(
                "MCL_PUBLICATION_REPORT_INVALID",
                "publication report is inconsistent or attempts to self-assert authority",
                "Produce a non-authoritative candidate in the protected workflow, then verify its external attestation before promotion.",
            ));
        }
        Ok(())
    }

    pub fn report_hash(&self, policy: &PublicationPolicy) -> Result<String, AppError> {
        self.validate_candidate(policy)?;
        hash_serializable(self, "MCL_PUBLICATION_REPORT_INVALID")
    }
}

impl PublicationAttestationVerification {
    pub fn validate(
        &self,
        report: &PublicationReport,
        policy: &PublicationPolicy,
    ) -> Result<(), AppError> {
        report.validate_candidate(policy)?;
        let expected_certificate_identity = format!(
            "https://github.com/{}/{}@{}",
            policy.repository, policy.workflow_path, policy.required_source_ref
        );
        let expected_signer_workflow = format!("{}/{}", policy.repository, policy.workflow_path);
        if self.schema_version != PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION
            || self.report_content_hash != report.report_hash(policy)?
            || !is_hash(&self.report_artifact_hash)
            || !is_hash(&self.attestation_bundle_hash)
            || !is_hash(&self.raw_verification_hash)
            || self.verifier_name != "gh"
            || self.verifier_version != policy.attestation_verifier_version
            || self.verifier_binary_sha256 != policy.attestation_verifier_binary_sha256
            || self.repository != policy.repository
            || self.signer_workflow != expected_signer_workflow
            || self.certificate_identity != expected_certificate_identity
            || self.source_ref != policy.required_source_ref
            || self.source_commit_sha != report.request.source_commit_sha
            || self.predicate_type != policy.attestation_predicate_type
            || !self.self_hosted_runners_denied
            || self.verified_attestation_count == 0
            || self.verified_timestamp_count == 0
            || self.authoritative
        {
            return Err(publication_error(
                "MCL_PUBLICATION_ATTESTATION_INVALID",
                "attestation verification does not bind the exact publication candidate and protected workflow policy",
                "Re-verify the exact retained report and bundle with the pinned verifier and closed policy constraints.",
            ));
        }
        Ok(())
    }
}

pub fn publication_policy_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/publication/policy/1",
        "title": "MathOS Publication Policy v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "policy_name", "repository", "workflow_path", "required_source_ref", "required_runner_environment", "required_lean_toolchain", "allowed_axioms", "requires_clean_checkout", "requires_dependency_closure", "requires_network_isolation", "requires_memory_limit", "attestation_predicate_type", "attestation_action_sha", "artifact_upload_action_sha", "attestation_verifier_version", "attestation_verifier_archive_sha256", "attestation_verifier_binary_sha256"],
        "properties": {
            "schema_version": {"const": PUBLICATION_POLICY_SCHEMA_VERSION},
            "policy_name": {"type": "string", "minLength": 1, "maxLength": 128},
            "repository": {"const": "Mnehmos/MathOS"},
            "workflow_path": {"const": ".github/workflows/publication.yml"},
            "required_source_ref": {"const": "refs/heads/main"},
            "required_runner_environment": {"const": "github_hosted"},
            "required_lean_toolchain": {"pattern": "^leanprover/lean4:v[0-9]+\\.[0-9]+\\.[0-9]+$"},
            "allowed_axioms": {"type": "array", "maxItems": MAX_AXIOMS, "items": {"type": "string", "minLength": 1, "maxLength": 256}},
            "requires_clean_checkout": {"const": true},
            "requires_dependency_closure": {"const": true},
            "requires_network_isolation": {"const": true},
            "requires_memory_limit": {"const": true},
            "attestation_predicate_type": {"const": "https://slsa.dev/provenance/v1"},
            "attestation_action_sha": {"type": "string", "pattern": "^[0-9a-f]{40}$"},
            "artifact_upload_action_sha": {"type": "string", "pattern": "^[0-9a-f]{40}$"},
            "attestation_verifier_version": {"type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$"},
            "attestation_verifier_archive_sha256": hash_schema(64),
            "attestation_verifier_binary_sha256": hash_schema(64)
        }
    })
}

pub fn publication_request_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/publication/request/1",
        "title": "MathOS Publication Request v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "subject", "outcome", "diagnostic_evidence_id", "diagnostic_evidence_hash", "proof_closure_evidence_id", "proof_closure_evidence_hash", "axiom_audit_evidence_id", "axiom_audit_evidence_hash", "environment_hash", "module_artifact_hash", "declaration_name", "policy_hash", "source_commit_sha", "source_tree_sha"],
        "properties": {
            "schema_version": {"const": PUBLICATION_REQUEST_SCHEMA_VERSION},
            "subject": exact_reference_schema(),
            "outcome": {"enum": ["proof", "refutation"]},
            "diagnostic_evidence_id": {"type": "string", "format": "uuid"},
            "diagnostic_evidence_hash": hash_schema(64),
            "proof_closure_evidence_id": {"type": "string", "format": "uuid"},
            "proof_closure_evidence_hash": hash_schema(64),
            "axiom_audit_evidence_id": {"type": "string", "format": "uuid"},
            "axiom_audit_evidence_hash": hash_schema(64),
            "environment_hash": hash_schema(64),
            "module_artifact_hash": hash_schema(64),
            "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256},
            "policy_hash": hash_schema(64),
            "source_commit_sha": hash_schema(40),
            "source_tree_sha": hash_schema(40)
        }
    })
}

pub fn publication_report_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/publication/report/1",
        "title": "MathOS Publication Candidate Report v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "request_hash", "request", "classification", "repository", "workflow_path", "source_ref", "workflow_run_id", "workflow_run_attempt", "runner_environment", "observed_lean_toolchain", "observed_axioms", "retained_artifact_hashes", "clean_checkout", "dependency_closure_complete", "network_isolation_enforced", "memory_limit_enforced", "authoritative"],
        "properties": {
            "schema_version": {"const": PUBLICATION_REPORT_SCHEMA_VERSION},
            "request_hash": hash_schema(64),
            "request": {"$ref": "https://mnehmos.ai/mathos/schemas/publication/request/1"},
            "classification": {"enum": ["passed", "rejected", "failed"]},
            "repository": {"const": "Mnehmos/MathOS"},
            "workflow_path": {"const": ".github/workflows/publication.yml"},
            "source_ref": {"const": "refs/heads/main"},
            "workflow_run_id": {"type": "integer", "minimum": 1},
            "workflow_run_attempt": {"type": "integer", "minimum": 1},
            "runner_environment": {"const": "github_hosted"},
            "observed_lean_toolchain": {"type": "string", "minLength": 1, "maxLength": 128},
            "observed_axioms": {"type": "array", "maxItems": MAX_AXIOMS, "items": {"type": "string", "minLength": 1, "maxLength": 256}},
            "retained_artifact_hashes": {"type": "array", "minItems": 1, "maxItems": MAX_ARTIFACTS, "items": hash_schema(64)},
            "clean_checkout": {"type": "boolean"},
            "dependency_closure_complete": {"type": "boolean"},
            "network_isolation_enforced": {"type": "boolean"},
            "memory_limit_enforced": {"type": "boolean"},
            "authoritative": {"const": false}
        }
    })
}

pub fn publication_attestation_verification_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/publication/attestation-verification/1",
        "title": "MathOS Publication Attestation Verification v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "report_content_hash", "report_artifact_hash", "attestation_bundle_hash", "raw_verification_hash", "verifier_name", "verifier_version", "verifier_binary_sha256", "repository", "signer_workflow", "certificate_identity", "source_ref", "source_commit_sha", "predicate_type", "self_hosted_runners_denied", "verified_attestation_count", "verified_timestamp_count", "authoritative"],
        "properties": {
            "schema_version": {"const": PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION},
            "report_content_hash": hash_schema(64),
            "report_artifact_hash": hash_schema(64),
            "attestation_bundle_hash": hash_schema(64),
            "raw_verification_hash": hash_schema(64),
            "verifier_name": {"const": "gh"},
            "verifier_version": {"type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$"},
            "verifier_binary_sha256": hash_schema(64),
            "repository": {"const": "Mnehmos/MathOS"},
            "signer_workflow": {"const": "Mnehmos/MathOS/.github/workflows/publication.yml"},
            "certificate_identity": {"const": "https://github.com/Mnehmos/MathOS/.github/workflows/publication.yml@refs/heads/main"},
            "source_ref": {"const": "refs/heads/main"},
            "source_commit_sha": hash_schema(40),
            "predicate_type": {"const": "https://slsa.dev/provenance/v1"},
            "self_hosted_runners_denied": {"const": true},
            "verified_attestation_count": {"type": "integer", "minimum": 1},
            "verified_timestamp_count": {"type": "integer", "minimum": 1},
            "authoritative": {"const": false}
        }
    })
}

fn hash_serializable<T: Serialize>(value: &T, code: &'static str) -> Result<String, AppError> {
    value_hash(&serde_json::to_value(value).map_err(|error| {
        publication_error(
            code,
            error.to_string(),
            "Report this deterministic publication serialization defect.",
        )
    })?)
}

fn exact_reference_schema() -> Value {
    json!({"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": hash_schema(64)}})
}

fn hash_schema(length: usize) -> Value {
    json!({"type": "string", "pattern": format!("^[0-9a-f]{{{length}}}$")})
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(is_lower_hex)
}

fn is_git_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(is_lower_hex)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn is_identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn is_repository(value: &str) -> bool {
    value == "Mnehmos/MathOS"
}

fn is_workflow_path(value: &str) -> bool {
    value == ".github/workflows/publication.yml"
}

fn is_lean_toolchain(value: &str) -> bool {
    let Some(version) = value.strip_prefix("leanprover/lean4:v") else {
        return false;
    };
    let parts = version.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_semver(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_lean_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

fn is_sorted_unique(values: &[String], limit: usize) -> bool {
    values.len() <= limit && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn publication_error(
    code: &'static str,
    message: impl Into<String>,
    action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> PublicationPolicy {
        serde_json::from_str(include_str!("../../policies/lean-publication-1.json"))
            .expect("committed publication policy")
    }

    fn request() -> PublicationRequest {
        PublicationRequest {
            schema_version: PUBLICATION_REQUEST_SCHEMA_VERSION.to_owned(),
            subject: ExactVersionReference {
                object_id: uuid::Uuid::now_v7().to_string(),
                version_hash: "a".repeat(64),
            },
            outcome: PublicationOutcome::Proof,
            diagnostic_evidence_id: uuid::Uuid::now_v7().to_string(),
            diagnostic_evidence_hash: "b".repeat(64),
            proof_closure_evidence_id: uuid::Uuid::now_v7().to_string(),
            proof_closure_evidence_hash: "c".repeat(64),
            axiom_audit_evidence_id: uuid::Uuid::now_v7().to_string(),
            axiom_audit_evidence_hash: "d".repeat(64),
            environment_hash: "e".repeat(64),
            module_artifact_hash: "f".repeat(64),
            declaration_name: "MathOS.Publication.truth".to_owned(),
            policy_hash: policy().policy_hash().expect("policy hash"),
            source_commit_sha: "1".repeat(40),
            source_tree_sha: "2".repeat(40),
        }
    }

    fn candidate_report() -> PublicationReport {
        let request = request();
        PublicationReport {
            schema_version: PUBLICATION_REPORT_SCHEMA_VERSION.to_owned(),
            request_hash: request.request_hash().expect("request hash"),
            request,
            classification: PublicationClassification::Passed,
            repository: "Mnehmos/MathOS".to_owned(),
            workflow_path: ".github/workflows/publication.yml".to_owned(),
            source_ref: "refs/heads/main".to_owned(),
            workflow_run_id: 1,
            workflow_run_attempt: 1,
            runner_environment: PublicationRunnerEnvironment::GithubHosted,
            observed_lean_toolchain: "leanprover/lean4:v4.32.0".to_owned(),
            observed_axioms: Vec::new(),
            retained_artifact_hashes: vec!["3".repeat(64)],
            clean_checkout: true,
            dependency_closure_complete: true,
            network_isolation_enforced: true,
            memory_limit_enforced: true,
            authoritative: false,
        }
    }

    fn attestation_verification(
        report: &PublicationReport,
        policy: &PublicationPolicy,
    ) -> PublicationAttestationVerification {
        PublicationAttestationVerification {
            schema_version: PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION.to_owned(),
            report_content_hash: report.report_hash(policy).expect("report hash"),
            report_artifact_hash: "3".repeat(64),
            attestation_bundle_hash: "4".repeat(64),
            raw_verification_hash: "5".repeat(64),
            verifier_name: "gh".to_owned(),
            verifier_version: policy.attestation_verifier_version.clone(),
            verifier_binary_sha256: policy.attestation_verifier_binary_sha256.clone(),
            repository: policy.repository.clone(),
            signer_workflow: format!("{}/{}", policy.repository, policy.workflow_path),
            certificate_identity: format!(
                "https://github.com/{}/{}@{}",
                policy.repository, policy.workflow_path, policy.required_source_ref
            ),
            source_ref: policy.required_source_ref.clone(),
            source_commit_sha: report.request.source_commit_sha.clone(),
            predicate_type: policy.attestation_predicate_type.clone(),
            self_hosted_runners_denied: true,
            verified_attestation_count: 1,
            verified_timestamp_count: 1,
            authoritative: false,
        }
    }

    #[test]
    fn candidate_cannot_assert_its_own_authority_or_hide_missing_controls() {
        let policy = policy();
        let mut report = candidate_report();
        report
            .validate_candidate(&policy)
            .expect("closed candidate");
        report.authoritative = true;
        assert_eq!(
            report
                .validate_candidate(&policy)
                .expect_err("self authority fails")
                .code,
            "MCL_PUBLICATION_REPORT_INVALID"
        );
        report.authoritative = false;
        report.network_isolation_enforced = false;
        assert_eq!(
            report
                .validate_candidate(&policy)
                .expect_err("missing isolation fails")
                .code,
            "MCL_PUBLICATION_REPORT_INVALID"
        );
    }

    #[test]
    fn altered_commit_policy_and_axiom_surface_fail_closed() {
        let policy = policy();
        let mut report = candidate_report();
        report.request.source_commit_sha = "not-a-commit".to_owned();
        assert_eq!(
            report
                .validate_candidate(&policy)
                .expect_err("bad commit fails")
                .code,
            "MCL_PUBLICATION_REQUEST_INVALID"
        );
        let mut report = candidate_report();
        report.request.policy_hash = "4".repeat(64);
        report.request_hash = report.request.request_hash().expect("changed request hash");
        assert_eq!(
            report
                .validate_candidate(&policy)
                .expect_err("policy mismatch fails")
                .code,
            "MCL_PUBLICATION_REPORT_INVALID"
        );
        let mut report = candidate_report();
        report.observed_axioms = vec!["MathOS.unknownAxiom".to_owned()];
        assert_eq!(
            report
                .validate_candidate(&policy)
                .expect_err("unexpected axiom fails")
                .code,
            "MCL_PUBLICATION_REPORT_INVALID"
        );
    }

    #[test]
    fn attestation_verification_binds_report_workflow_and_pinned_verifier() {
        let policy = policy();
        let report = candidate_report();
        let verification = attestation_verification(&report, &policy);
        verification
            .validate(&report, &policy)
            .expect("closed attestation verification");

        for corrupt in [
            |value: &mut PublicationAttestationVerification| value.authoritative = true,
            |value: &mut PublicationAttestationVerification| {
                value.source_commit_sha = "6".repeat(40)
            },
            |value: &mut PublicationAttestationVerification| {
                value.verifier_binary_sha256 = "7".repeat(64)
            },
            |value: &mut PublicationAttestationVerification| value.verified_timestamp_count = 0,
        ] {
            let mut altered = verification.clone();
            corrupt(&mut altered);
            assert_eq!(
                altered
                    .validate(&report, &policy)
                    .expect_err("altered verification fails")
                    .code,
                "MCL_PUBLICATION_ATTESTATION_INVALID"
            );
        }
    }

    #[test]
    fn committed_policy_and_schemas_match_rust_contracts() {
        let policy_value: Value = serde_json::from_str(include_str!(
            "../../schemas/publication/publication-policy-1.schema.json"
        ))
        .expect("policy schema");
        let request_value: Value = serde_json::from_str(include_str!(
            "../../schemas/publication/publication-request-1.schema.json"
        ))
        .expect("request schema");
        let report_value: Value = serde_json::from_str(include_str!(
            "../../schemas/publication/publication-report-1.schema.json"
        ))
        .expect("report schema");
        let attestation_value: Value = serde_json::from_str(include_str!(
            "../../schemas/publication/publication-attestation-verification-1.schema.json"
        ))
        .expect("attestation verification schema");
        assert_eq!(policy_value, publication_policy_schema());
        assert_eq!(request_value, publication_request_schema());
        assert_eq!(report_value, publication_report_schema());
        assert_eq!(
            attestation_value,
            publication_attestation_verification_schema()
        );
        let policy = policy();
        policy.validate().expect("committed policy validates");
        assert_eq!(
            policy.policy_hash().expect("committed policy hash"),
            include_str!("../../policies/lean-publication-1.sha256").trim()
        );
    }
}
