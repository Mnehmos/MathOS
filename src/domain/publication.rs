use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::schemas::ExactVersionReference;
use crate::error::AppError;

pub const PUBLICATION_POLICY_SCHEMA_VERSION: &str = "publication_policy/1";
pub const PUBLICATION_REQUEST_SCHEMA_VERSION: &str = "publication_request/1";
pub const PUBLICATION_RETAINED_CLOSURE_SCHEMA_VERSION: &str = "publication_retained_closure/1";
pub const PUBLICATION_REPORT_SCHEMA_VERSION: &str = "publication_report/1";
pub const PUBLICATION_STAGE_SCHEMA_VERSION: &str = "publication_stage/1";
pub const PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION: &str =
    "publication_attestation_verification/1";
pub const MAX_PUBLICATION_VERIFIED_TIMESTAMPS: u32 = 8;
const MAX_AXIOMS: usize = 256;
const MAX_ARTIFACTS: usize = 256;
const MAX_STAGE_INPUT_BYTES: u64 = 16 * 1_048_576;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationPolicy {
    pub schema_version: String,
    pub policy_name: String,
    pub repository: String,
    pub repository_id: u64,
    pub repository_owner_id: u64,
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

impl PublicationOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proof => "proof",
            Self::Refutation => "refutation",
        }
    }
}

impl FromStr for PublicationOutcome {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "proof" => Ok(Self::Proof),
            "refutation" => Ok(Self::Refutation),
            _ => Err(publication_error(
                "MCL_PUBLICATION_OUTCOME_INVALID",
                format!("unknown publication outcome `{value}`"),
                "Use exactly `proof` or `refutation`.",
            )),
        }
    }
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationRetainedArtifactRole {
    AuditJob,
    AuditPolicy,
    AuditReport,
    AuditStderr,
    AuditStdout,
    AxiomAuditEvidence,
    ClaimVersion,
    DiagnosticEvidence,
    EnvironmentManifest,
    FormalizationVersion,
    LeanModule,
    ProofClosureEvidence,
    ProtectedAuditStderr,
    ProtectedAuditStdout,
    ProtectedDependencyStderr,
    ProtectedDependencyStdout,
    ProtectedStderr,
    ProtectedStdout,
    PublicationPolicy,
    PublicationRequest,
    SourceVersion,
    VerifierJob,
    VerifierReport,
    VerifierStderr,
    VerifierStdout,
}

impl PublicationRetainedArtifactRole {
    pub const ALL: [Self; 25] = [
        Self::AuditJob,
        Self::AuditPolicy,
        Self::AuditReport,
        Self::AuditStderr,
        Self::AuditStdout,
        Self::AxiomAuditEvidence,
        Self::ClaimVersion,
        Self::DiagnosticEvidence,
        Self::EnvironmentManifest,
        Self::FormalizationVersion,
        Self::LeanModule,
        Self::ProofClosureEvidence,
        Self::ProtectedAuditStderr,
        Self::ProtectedAuditStdout,
        Self::ProtectedDependencyStderr,
        Self::ProtectedDependencyStdout,
        Self::ProtectedStderr,
        Self::ProtectedStdout,
        Self::PublicationPolicy,
        Self::PublicationRequest,
        Self::SourceVersion,
        Self::VerifierJob,
        Self::VerifierReport,
        Self::VerifierStderr,
        Self::VerifierStdout,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuditJob => "audit_job",
            Self::AuditPolicy => "audit_policy",
            Self::AuditReport => "audit_report",
            Self::AuditStderr => "audit_stderr",
            Self::AuditStdout => "audit_stdout",
            Self::AxiomAuditEvidence => "axiom_audit_evidence",
            Self::ClaimVersion => "claim_version",
            Self::DiagnosticEvidence => "diagnostic_evidence",
            Self::EnvironmentManifest => "environment_manifest",
            Self::FormalizationVersion => "formalization_version",
            Self::LeanModule => "lean_module",
            Self::ProofClosureEvidence => "proof_closure_evidence",
            Self::ProtectedAuditStderr => "protected_audit_stderr",
            Self::ProtectedAuditStdout => "protected_audit_stdout",
            Self::ProtectedDependencyStderr => "protected_dependency_stderr",
            Self::ProtectedDependencyStdout => "protected_dependency_stdout",
            Self::ProtectedStderr => "protected_stderr",
            Self::ProtectedStdout => "protected_stdout",
            Self::PublicationPolicy => "publication_policy",
            Self::PublicationRequest => "publication_request",
            Self::SourceVersion => "source_version",
            Self::VerifierJob => "verifier_job",
            Self::VerifierReport => "verifier_report",
            Self::VerifierStderr => "verifier_stderr",
            Self::VerifierStdout => "verifier_stdout",
        }
    }

    pub const fn expected_path(self) -> &'static str {
        match self {
            Self::AuditJob => "closure/audit-job.json",
            Self::AuditPolicy => "closure/audit-policy.json",
            Self::AuditReport => "closure/audit-report.json",
            Self::AuditStderr => "closure/audit.stderr",
            Self::AuditStdout => "closure/audit.stdout",
            Self::AxiomAuditEvidence => "closure/axiom-audit-evidence.json",
            Self::ClaimVersion => "closure/claim-version.json",
            Self::DiagnosticEvidence => "closure/diagnostic-evidence.json",
            Self::EnvironmentManifest => "closure/environment-manifest.json",
            Self::FormalizationVersion => "closure/formalization-version.json",
            Self::LeanModule => "closure/module.lean",
            Self::ProofClosureEvidence => "closure/proof-closure-evidence.json",
            Self::ProtectedAuditStderr => "closure/protected-audit.stderr",
            Self::ProtectedAuditStdout => "closure/protected-audit.stdout",
            Self::ProtectedDependencyStderr => "closure/protected-dependency.stderr",
            Self::ProtectedDependencyStdout => "closure/protected-dependency.stdout",
            Self::ProtectedStderr => "closure/protected.stderr",
            Self::ProtectedStdout => "closure/protected.stdout",
            Self::PublicationPolicy => "closure/publication-policy.json",
            Self::PublicationRequest => "closure/publication-request.json",
            Self::SourceVersion => "closure/source-version.json",
            Self::VerifierJob => "closure/verifier-job.json",
            Self::VerifierReport => "closure/verifier-report.json",
            Self::VerifierStderr => "closure/verifier.stderr",
            Self::VerifierStdout => "closure/verifier.stdout",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRetainedClosureEntry {
    pub role: PublicationRetainedArtifactRole,
    pub path: String,
    pub identity_hash: String,
    pub artifact_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRetainedClosure {
    pub schema_version: String,
    pub subject: ExactVersionReference,
    pub request_hash: String,
    pub artifacts: Vec<PublicationRetainedClosureEntry>,
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
pub struct PublicationStageArtifact {
    pub role: PublicationRetainedArtifactRole,
    pub path: String,
    pub identity_hash: String,
    pub artifact_hash: String,
    pub byte_size: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationStage {
    pub schema_version: String,
    pub report_artifact_hash: String,
    pub report_byte_size: u64,
    pub retained_closure_artifact_hash: String,
    pub retained_closure_byte_size: u64,
    pub attestation_bundle_artifact_hash: String,
    pub attestation_bundle_byte_size: u64,
    pub retained_artifacts: Vec<PublicationStageArtifact>,
    pub authoritative: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicationStageSnapshot {
    pub stage_hash: String,
    pub stage: PublicationStage,
    pub created_at: i64,
    pub created_by: String,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicationIngestionReceiptSnapshot {
    pub receipt_hash: String,
    pub stage_hash: String,
    pub verification: PublicationAttestationVerification,
    pub raw_verification_byte_size: u64,
    pub receipt_byte_size: u64,
    pub created_at: i64,
    pub created_by: String,
}

impl PublicationPolicy {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != PUBLICATION_POLICY_SCHEMA_VERSION
            || !is_identifier(&self.policy_name, 128)
            || !is_repository(&self.repository)
            || self.repository_id != 1_305_399_818
            || self.repository_owner_id != 193_347_153
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

impl PublicationRetainedClosure {
    pub fn validate(&self, request: &PublicationRequest) -> Result<(), AppError> {
        request.validate()?;
        let expected_request_hash = request.request_hash()?;
        let exact_roles = self.artifacts.len() == PublicationRetainedArtifactRole::ALL.len()
            && self
                .artifacts
                .iter()
                .zip(PublicationRetainedArtifactRole::ALL)
                .all(|(entry, role)| entry.role == role);
        let valid_entries = self.artifacts.iter().all(|entry| {
            entry.path == entry.role.expected_path()
                && is_hash(&entry.identity_hash)
                && is_hash(&entry.artifact_hash)
        });
        let binds = |role, identity_hash: &str, artifact_hash: Option<&str>| {
            self.artifacts
                .iter()
                .find(|entry| entry.role == role)
                .is_some_and(|entry| {
                    entry.identity_hash == identity_hash
                        && artifact_hash.is_none_or(|hash| entry.artifact_hash == hash)
                })
        };

        if self.schema_version != PUBLICATION_RETAINED_CLOSURE_SCHEMA_VERSION
            || self.subject != request.subject
            || self.request_hash != expected_request_hash
            || !exact_roles
            || !valid_entries
            || !binds(
                PublicationRetainedArtifactRole::PublicationRequest,
                &expected_request_hash,
                Some(&expected_request_hash),
            )
            || !binds(
                PublicationRetainedArtifactRole::FormalizationVersion,
                &request.subject.version_hash,
                None,
            )
            || !binds(
                PublicationRetainedArtifactRole::EnvironmentManifest,
                &request.environment_hash,
                None,
            )
            || !binds(
                PublicationRetainedArtifactRole::LeanModule,
                &request.module_artifact_hash,
                Some(&request.module_artifact_hash),
            )
            || !binds(
                PublicationRetainedArtifactRole::PublicationPolicy,
                &request.policy_hash,
                None,
            )
            || !binds(
                PublicationRetainedArtifactRole::DiagnosticEvidence,
                &request.diagnostic_evidence_hash,
                None,
            )
            || !binds(
                PublicationRetainedArtifactRole::ProofClosureEvidence,
                &request.proof_closure_evidence_hash,
                None,
            )
            || !binds(
                PublicationRetainedArtifactRole::AxiomAuditEvidence,
                &request.axiom_audit_evidence_hash,
                None,
            )
        {
            return Err(publication_error(
                "MCL_PUBLICATION_RETAINED_CLOSURE_INVALID",
                "retained publication closure is incomplete, unsafe, or inconsistent with its canonical request",
                "Retain exactly one safely named artifact for every required role and bind all request-controlled identities.",
            ));
        }
        Ok(())
    }

    pub fn closure_hash(&self, request: &PublicationRequest) -> Result<String, AppError> {
        self.validate(request)?;
        hash_serializable(self, "MCL_PUBLICATION_RETAINED_CLOSURE_INVALID")
    }

    pub fn report_retained_artifact_hashes(
        &self,
        request: &PublicationRequest,
    ) -> Result<Vec<String>, AppError> {
        self.validate(request)?;
        let closure_artifact_hash = self.closure_hash(request)?;
        let mut hashes = self
            .artifacts
            .iter()
            .map(|entry| entry.artifact_hash.clone())
            .collect::<Vec<_>>();
        hashes.push(closure_artifact_hash);
        hashes.sort_unstable();
        hashes.dedup();
        Ok(hashes)
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

impl PublicationStage {
    pub fn validate(&self) -> Result<(), AppError> {
        let exact_roles = self.retained_artifacts.len()
            == PublicationRetainedArtifactRole::ALL.len()
            && self
                .retained_artifacts
                .iter()
                .zip(PublicationRetainedArtifactRole::ALL)
                .all(|(entry, role)| entry.role == role);
        let valid_artifacts = self.retained_artifacts.iter().all(|entry| {
            entry.path == entry.role.expected_path()
                && is_hash(&entry.identity_hash)
                && is_hash(&entry.artifact_hash)
                && entry.byte_size <= MAX_STAGE_INPUT_BYTES
        });
        if self.schema_version != PUBLICATION_STAGE_SCHEMA_VERSION
            || !is_hash(&self.report_artifact_hash)
            || self.report_byte_size == 0
            || self.report_byte_size > MAX_STAGE_INPUT_BYTES
            || !is_hash(&self.retained_closure_artifact_hash)
            || self.retained_closure_byte_size == 0
            || self.retained_closure_byte_size > MAX_STAGE_INPUT_BYTES
            || !is_hash(&self.attestation_bundle_artifact_hash)
            || self.attestation_bundle_byte_size == 0
            || self.attestation_bundle_byte_size > MAX_STAGE_INPUT_BYTES
            || !exact_roles
            || !valid_artifacts
            || self.authoritative
        {
            return Err(publication_error(
                "MCL_PUBLICATION_STAGE_INVALID",
                "publication stage does not register one closed bounded non-authoritative candidate",
                "Stage the exact canonical report, closure, fixed retained members, and Sigstore bundle.",
            ));
        }
        Ok(())
    }

    pub fn stage_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        hash_serializable(self, "MCL_PUBLICATION_STAGE_INVALID")
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
            || self.report_artifact_hash != self.report_content_hash
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
            || self.verified_attestation_count != 1
            || !(1..=MAX_PUBLICATION_VERIFIED_TIMESTAMPS).contains(&self.verified_timestamp_count)
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

pub fn publication_stage_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/publication/stage/1",
        "title": "MathOS Publication Stage v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "report_artifact_hash", "report_byte_size", "retained_closure_artifact_hash", "retained_closure_byte_size", "attestation_bundle_artifact_hash", "attestation_bundle_byte_size", "retained_artifacts", "authoritative"],
        "properties": {
            "schema_version": {"const": PUBLICATION_STAGE_SCHEMA_VERSION},
            "report_artifact_hash": hash_schema(64),
            "report_byte_size": {"type": "integer", "minimum": 1, "maximum": MAX_STAGE_INPUT_BYTES},
            "retained_closure_artifact_hash": hash_schema(64),
            "retained_closure_byte_size": {"type": "integer", "minimum": 1, "maximum": MAX_STAGE_INPUT_BYTES},
            "attestation_bundle_artifact_hash": hash_schema(64),
            "attestation_bundle_byte_size": {"type": "integer", "minimum": 1, "maximum": MAX_STAGE_INPUT_BYTES},
            "retained_artifacts": {
                "type": "array",
                "minItems": PublicationRetainedArtifactRole::ALL.len(),
                "maxItems": PublicationRetainedArtifactRole::ALL.len(),
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role", "path", "identity_hash", "artifact_hash", "byte_size"],
                    "properties": {
                        "role": {"enum": PublicationRetainedArtifactRole::ALL.map(PublicationRetainedArtifactRole::as_str)},
                        "path": {"type": "string", "minLength": 1, "maxLength": 256},
                        "identity_hash": hash_schema(64),
                        "artifact_hash": hash_schema(64),
                        "byte_size": {"type": "integer", "minimum": 0, "maximum": MAX_STAGE_INPUT_BYTES}
                    }
                }
            },
            "authoritative": {"const": false}
        }
    })
}

pub fn committed_publication_policy() -> Result<PublicationPolicy, AppError> {
    let policy: PublicationPolicy = serde_json::from_str(include_str!(
        "../../policies/lean-publication-1.json"
    ))
    .map_err(|error| {
        publication_error(
            "MCL_PUBLICATION_POLICY_INVALID",
            format!("committed publication policy is invalid: {error}"),
            "Restore the reviewed publication policy from a verified source revision.",
        )
    })?;
    policy.validate()?;
    Ok(policy)
}

pub fn publication_policy_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/publication/policy/1",
        "title": "MathOS Publication Policy v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "policy_name", "repository", "repository_id", "repository_owner_id", "workflow_path", "required_source_ref", "required_runner_environment", "required_lean_toolchain", "allowed_axioms", "requires_clean_checkout", "requires_dependency_closure", "requires_network_isolation", "requires_memory_limit", "attestation_predicate_type", "attestation_action_sha", "artifact_upload_action_sha", "attestation_verifier_version", "attestation_verifier_archive_sha256", "attestation_verifier_binary_sha256"],
        "properties": {
            "schema_version": {"const": PUBLICATION_POLICY_SCHEMA_VERSION},
            "policy_name": {"type": "string", "minLength": 1, "maxLength": 128},
            "repository": {"const": "Mnehmos/MathOS"},
            "repository_id": {"const": 1_305_399_818_u64},
            "repository_owner_id": {"const": 193_347_153_u64},
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

pub fn publication_retained_closure_schema() -> Value {
    let roles = PublicationRetainedArtifactRole::ALL
        .into_iter()
        .map(|role| Value::String(role.as_str().to_owned()))
        .collect::<Vec<_>>();
    let paths = PublicationRetainedArtifactRole::ALL
        .into_iter()
        .map(|role| Value::String(role.expected_path().to_owned()))
        .collect::<Vec<_>>();
    let ordered_artifacts = PublicationRetainedArtifactRole::ALL
        .into_iter()
        .map(|role| {
            json!({
                "allOf": [
                    {"$ref": "#/$defs/artifact"},
                    {"properties": {
                        "role": {"const": role.as_str()},
                        "path": {"const": role.expected_path()}
                    }}
                ]
            })
        })
        .collect::<Vec<_>>();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/publication/retained-closure/1",
        "title": "MathOS Publication Retained Closure v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "subject", "request_hash", "artifacts"],
        "properties": {
            "schema_version": {"const": PUBLICATION_RETAINED_CLOSURE_SCHEMA_VERSION},
            "subject": exact_reference_schema(),
            "request_hash": hash_schema(64),
            "artifacts": {
                "type": "array",
                "minItems": PublicationRetainedArtifactRole::ALL.len(),
                "maxItems": PublicationRetainedArtifactRole::ALL.len(),
                "uniqueItems": true,
                "prefixItems": ordered_artifacts,
                "items": false
            }
        },
        "$defs": {
            "artifact": {
                "type": "object",
                "additionalProperties": false,
                "required": ["role", "path", "identity_hash", "artifact_hash"],
                "properties": {
                    "role": {"enum": roles},
                    "path": {
                        "type": "string",
                        "enum": paths
                    },
                    "identity_hash": hash_schema(64),
                    "artifact_hash": hash_schema(64)
                }
            }
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
            "verified_attestation_count": {"const": 1},
            "verified_timestamp_count": {"type": "integer", "minimum": 1, "maximum": MAX_PUBLICATION_VERIFIED_TIMESTAMPS},
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
        committed_publication_policy().expect("committed publication policy")
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

    fn retained_closure(request: &PublicationRequest) -> PublicationRetainedClosure {
        let request_hash = request.request_hash().expect("request hash");
        let artifacts = PublicationRetainedArtifactRole::ALL
            .into_iter()
            .enumerate()
            .map(|(index, role)| PublicationRetainedClosureEntry {
                role,
                path: role.expected_path().to_owned(),
                identity_hash: format!("{:064x}", index + 16),
                artifact_hash: format!("{:064x}", index + 64),
            })
            .collect::<Vec<_>>();
        let mut closure = PublicationRetainedClosure {
            schema_version: PUBLICATION_RETAINED_CLOSURE_SCHEMA_VERSION.to_owned(),
            subject: request.subject.clone(),
            request_hash: request_hash.clone(),
            artifacts,
        };
        let mut bind = |role, identity_hash: &str, artifact_hash: Option<&str>| {
            let entry = closure
                .artifacts
                .iter_mut()
                .find(|entry| entry.role == role)
                .expect("required retained role");
            entry.identity_hash = identity_hash.to_owned();
            if let Some(artifact_hash) = artifact_hash {
                entry.artifact_hash = artifact_hash.to_owned();
            }
        };
        bind(
            PublicationRetainedArtifactRole::PublicationRequest,
            &request_hash,
            Some(&request_hash),
        );
        bind(
            PublicationRetainedArtifactRole::FormalizationVersion,
            &request.subject.version_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::EnvironmentManifest,
            &request.environment_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::LeanModule,
            &request.module_artifact_hash,
            Some(&request.module_artifact_hash),
        );
        bind(
            PublicationRetainedArtifactRole::PublicationPolicy,
            &request.policy_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::DiagnosticEvidence,
            &request.diagnostic_evidence_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::ProofClosureEvidence,
            &request.proof_closure_evidence_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::AxiomAuditEvidence,
            &request.axiom_audit_evidence_hash,
            None,
        );
        closure
    }

    #[test]
    fn retained_closure_is_exact_sorted_and_allows_duplicate_artifact_hashes() {
        let request = request();
        let mut closure = retained_closure(&request);
        let duplicate_log_hash = "8".repeat(64);
        for role in [
            PublicationRetainedArtifactRole::VerifierStdout,
            PublicationRetainedArtifactRole::VerifierStderr,
        ] {
            closure
                .artifacts
                .iter_mut()
                .find(|entry| entry.role == role)
                .expect("log role")
                .artifact_hash = duplicate_log_hash.clone();
        }

        closure.validate(&request).expect("closed retained set");
        assert_eq!(
            closure.closure_hash(&request).expect("closure hash").len(),
            64
        );
        let closure_artifact_hash = closure.closure_hash(&request).expect("closure hash");
        let retained_hashes = closure
            .report_retained_artifact_hashes(&request)
            .expect("report retention set");
        assert!(retained_hashes.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(retained_hashes.contains(&duplicate_log_hash));
        assert!(retained_hashes.contains(&closure_artifact_hash));
        assert_eq!(retained_hashes.len(), closure.artifacts.len());
    }

    #[test]
    fn retained_closure_rejects_missing_duplicate_or_unsorted_roles_and_paths() {
        let request = request();
        let closure = retained_closure(&request);

        let names = PublicationRetainedArtifactRole::ALL
            .into_iter()
            .map(PublicationRetainedArtifactRole::as_str)
            .collect::<Vec<_>>();
        let mut sorted_names = names.clone();
        sorted_names.sort_unstable();
        assert_eq!(names, sorted_names);

        let mut missing = closure.clone();
        missing.artifacts.pop();
        assert_eq!(
            missing
                .validate(&request)
                .expect_err("missing role fails")
                .code,
            "MCL_PUBLICATION_RETAINED_CLOSURE_INVALID"
        );

        let mut duplicate = closure.clone();
        duplicate.artifacts[1].role = duplicate.artifacts[0].role;
        assert!(duplicate.validate(&request).is_err());

        let mut unsorted = closure.clone();
        unsorted.artifacts.swap(0, 1);
        assert!(unsorted.validate(&request).is_err());

        let unsafe_or_aliased_paths = [
            "",
            "../escape",
            "/absolute",
            "C:/drive",
            "closure\\audit-job.json",
            "closure//audit-job.json",
            "closure/./audit-job.json",
            ".hidden",
            "closure/has space",
            "closure/AUDIT-JOB.JSON",
            "closure/audit-job.json.",
            "closure/con.json",
            "-rf",
        ];
        for unsafe_path in unsafe_or_aliased_paths {
            let mut altered = closure.clone();
            altered.artifacts[0].path = unsafe_path.to_owned();
            assert!(
                altered.validate(&request).is_err(),
                "unsafe path accepted: {unsafe_path}"
            );
        }
        let mut duplicate_path = closure.clone();
        duplicate_path.artifacts[1].path = duplicate_path.artifacts[0].path.clone();
        assert!(duplicate_path.validate(&request).is_err());
    }

    #[test]
    fn retained_closure_rejects_altered_request_bindings_and_hashes() {
        let request = request();
        let closure = retained_closure(&request);

        for role in [
            PublicationRetainedArtifactRole::PublicationRequest,
            PublicationRetainedArtifactRole::FormalizationVersion,
            PublicationRetainedArtifactRole::EnvironmentManifest,
            PublicationRetainedArtifactRole::LeanModule,
            PublicationRetainedArtifactRole::PublicationPolicy,
            PublicationRetainedArtifactRole::DiagnosticEvidence,
            PublicationRetainedArtifactRole::ProofClosureEvidence,
            PublicationRetainedArtifactRole::AxiomAuditEvidence,
        ] {
            let mut altered = closure.clone();
            altered
                .artifacts
                .iter_mut()
                .find(|entry| entry.role == role)
                .expect("bound role")
                .identity_hash = "9".repeat(64);
            assert!(
                altered.validate(&request).is_err(),
                "altered identity accepted for {}",
                role.as_str()
            );
        }

        for role in [
            PublicationRetainedArtifactRole::PublicationRequest,
            PublicationRetainedArtifactRole::LeanModule,
        ] {
            let mut altered = closure.clone();
            altered
                .artifacts
                .iter_mut()
                .find(|entry| entry.role == role)
                .expect("artifact-bound role")
                .artifact_hash = "9".repeat(64);
            assert!(
                altered.validate(&request).is_err(),
                "altered artifact accepted for {}",
                role.as_str()
            );
        }

        let mut altered_subject = closure.clone();
        altered_subject.subject.object_id = uuid::Uuid::now_v7().to_string();
        assert!(altered_subject.validate(&request).is_err());
        let mut altered_request_hash = closure.clone();
        altered_request_hash.request_hash = "9".repeat(64);
        assert!(altered_request_hash.validate(&request).is_err());

        for invalid_hash in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
            let mut invalid_identity = closure.clone();
            invalid_identity.artifacts[0].identity_hash = invalid_hash.clone();
            assert!(invalid_identity.validate(&request).is_err());
            let mut invalid_artifact = closure.clone();
            invalid_artifact.artifacts[0].artifact_hash = invalid_hash;
            assert!(invalid_artifact.validate(&request).is_err());
        }
        let retained_hashes = closure
            .report_retained_artifact_hashes(&request)
            .expect("derived retained hash set");
        assert!(retained_hashes.contains(&closure.closure_hash(&request).expect("closure hash")));
    }

    #[test]
    fn retained_closure_deserialization_denies_unknown_fields_and_roles() {
        let request = request();
        let closure = retained_closure(&request);
        let mut unknown_top = serde_json::to_value(&closure).expect("closure value");
        unknown_top
            .as_object_mut()
            .expect("closure object")
            .insert("authoritative".to_owned(), Value::Bool(true));
        assert!(serde_json::from_value::<PublicationRetainedClosure>(unknown_top).is_err());

        let mut unknown_entry = serde_json::to_value(&closure).expect("closure value");
        unknown_entry["artifacts"][0]["extra"] = Value::Bool(true);
        assert!(serde_json::from_value::<PublicationRetainedClosure>(unknown_entry).is_err());

        let mut unknown_role = serde_json::to_value(&closure).expect("closure value");
        unknown_role["artifacts"][0]["role"] = Value::String("publication_report".to_owned());
        assert!(serde_json::from_value::<PublicationRetainedClosure>(unknown_role).is_err());
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

    fn publication_stage() -> PublicationStage {
        let request = request();
        let closure = retained_closure(&request);
        PublicationStage {
            schema_version: PUBLICATION_STAGE_SCHEMA_VERSION.to_owned(),
            report_artifact_hash: "1".repeat(64),
            report_byte_size: 1_024,
            retained_closure_artifact_hash: closure.closure_hash(&request).expect("closure hash"),
            retained_closure_byte_size: 2_048,
            attestation_bundle_artifact_hash: "2".repeat(64),
            attestation_bundle_byte_size: 4_096,
            retained_artifacts: closure
                .artifacts
                .into_iter()
                .map(|entry| PublicationStageArtifact {
                    role: entry.role,
                    path: entry.path,
                    identity_hash: entry.identity_hash,
                    artifact_hash: entry.artifact_hash,
                    byte_size: 0,
                })
                .collect(),
            authoritative: false,
        }
    }

    #[test]
    fn publication_stage_is_closed_bounded_and_non_authoritative() {
        let stage = publication_stage();
        stage.validate().expect("closed stage");
        assert_eq!(stage.stage_hash().expect("stage hash").len(), 64);

        let mut authoritative = stage.clone();
        authoritative.authoritative = true;
        assert_eq!(
            authoritative
                .validate()
                .expect_err("stage cannot assert authority")
                .code,
            "MCL_PUBLICATION_STAGE_INVALID"
        );

        let mut missing = stage;
        missing.retained_artifacts.pop();
        assert_eq!(
            missing
                .validate()
                .expect_err("stage must retain every role")
                .code,
            "MCL_PUBLICATION_STAGE_INVALID"
        );
    }

    fn attestation_verification(
        report: &PublicationReport,
        policy: &PublicationPolicy,
    ) -> PublicationAttestationVerification {
        let report_hash = report.report_hash(policy).expect("report hash");
        PublicationAttestationVerification {
            schema_version: PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION.to_owned(),
            report_content_hash: report_hash.clone(),
            report_artifact_hash: report_hash,
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
            |value: &mut PublicationAttestationVerification| value.verified_attestation_count = 2,
            |value: &mut PublicationAttestationVerification| value.verified_timestamp_count = 0,
            |value: &mut PublicationAttestationVerification| value.verified_timestamp_count = 9,
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
        let retained_closure_value: Value = serde_json::from_str(include_str!(
            "../../schemas/publication/publication-retained-closure-1.schema.json"
        ))
        .expect("retained closure schema");
        let report_value: Value = serde_json::from_str(include_str!(
            "../../schemas/publication/publication-report-1.schema.json"
        ))
        .expect("report schema");
        let stage_value: Value = serde_json::from_str(include_str!(
            "../../schemas/publication/publication-stage-1.schema.json"
        ))
        .expect("stage schema");
        let attestation_value: Value = serde_json::from_str(include_str!(
            "../../schemas/publication/publication-attestation-verification-1.schema.json"
        ))
        .expect("attestation verification schema");
        assert_eq!(policy_value, publication_policy_schema());
        assert_eq!(request_value, publication_request_schema());
        assert_eq!(
            retained_closure_value,
            publication_retained_closure_schema()
        );
        assert_eq!(report_value, publication_report_schema());
        assert_eq!(stage_value, publication_stage_schema());
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

    #[test]
    fn publication_outcomes_have_one_closed_cli_vocabulary() {
        for (value, outcome) in [
            ("proof", PublicationOutcome::Proof),
            ("refutation", PublicationOutcome::Refutation),
        ] {
            assert_eq!(
                value.parse::<PublicationOutcome>().expect("known outcome"),
                outcome
            );
            assert_eq!(outcome.as_str(), value);
        }
        assert_eq!(
            "proved"
                .parse::<PublicationOutcome>()
                .expect_err("unknown outcome fails closed")
                .code,
            "MCL_PUBLICATION_OUTCOME_INVALID"
        );
    }
}
