use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::schemas::ExactVersionReference;
use crate::error::AppError;

pub const EVIDENCE_SCHEMA_VERSION: &str = "evidence/1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    LeanElaboration,
    LeanKernelProof,
    LeanKernelRefutation,
    CertificateReplay,
    BoundedComputation,
    EmpiricalSearch,
    StatementFidelityReview,
    LiteratureReview,
    NoveltyReview,
    AxiomAudit,
    ProofClosureScan,
    CleanRebuild,
    ComparatorRun,
    HumanReview,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceResult {
    Accepted,
    Rejected,
    Inconclusive,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAuthorityClass {
    Diagnostic,
    Empirical,
    Reviewed,
    Authoritative,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceSnapshot {
    pub evidence_id: String,
    pub evidence_hash: String,
    pub payload: EvidencePayload,
    pub created_at: i64,
    pub created_by: String,
}

impl EvidenceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LeanElaboration => "lean_elaboration",
            Self::LeanKernelProof => "lean_kernel_proof",
            Self::LeanKernelRefutation => "lean_kernel_refutation",
            Self::CertificateReplay => "certificate_replay",
            Self::BoundedComputation => "bounded_computation",
            Self::EmpiricalSearch => "empirical_search",
            Self::StatementFidelityReview => "statement_fidelity_review",
            Self::LiteratureReview => "literature_review",
            Self::NoveltyReview => "novelty_review",
            Self::AxiomAudit => "axiom_audit",
            Self::ProofClosureScan => "proof_closure_scan",
            Self::CleanRebuild => "clean_rebuild",
            Self::ComparatorRun => "comparator_run",
            Self::HumanReview => "human_review",
        }
    }
}

impl EvidenceResult {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Inconclusive => "inconclusive",
            Self::Failed => "failed",
        }
    }
}

impl EvidenceAuthorityClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Diagnostic => "diagnostic",
            Self::Empirical => "empirical",
            Self::Reviewed => "reviewed",
            Self::Authoritative => "authoritative",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePayload {
    pub schema_version: String,
    pub subject: ExactVersionReference,
    pub evidence_kind: EvidenceKind,
    pub result: EvidenceResult,
    pub authority_class: EvidenceAuthorityClass,
    pub producing_run_id: Option<String>,
    pub producing_job_id: Option<String>,
    pub artifact_hashes: Vec<String>,
    pub verifier_or_reviewer_identity: String,
    pub environment_hash: Option<String>,
    pub supersedes_evidence_id: Option<String>,
    pub stale: bool,
    pub stale_reason: Option<String>,
}

impl EvidencePayload {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != EVIDENCE_SCHEMA_VERSION
            || self.verifier_or_reviewer_identity.trim().is_empty()
            || self.verifier_or_reviewer_identity.len() > 256
            || self.artifact_hashes.len() > 256
            || self
                .artifact_hashes
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self.artifact_hashes.iter().any(|hash| !is_hash(hash))
            || self
                .environment_hash
                .as_deref()
                .is_some_and(|hash| !is_hash(hash))
            || self.stale != self.stale_reason.is_some()
            || self
                .stale_reason
                .as_deref()
                .is_some_and(|reason| reason.trim().is_empty() || reason.len() > 4_096)
            || (self.producing_run_id.is_none() && self.producing_job_id.is_none())
            || (self.evidence_kind == EvidenceKind::LeanElaboration
                && (self.authority_class != EvidenceAuthorityClass::Diagnostic
                    || self.producing_job_id.is_none()
                    || self.environment_hash.is_none()))
            || (matches!(
                self.evidence_kind,
                EvidenceKind::ProofClosureScan | EvidenceKind::AxiomAudit
            ) && (self.authority_class != EvidenceAuthorityClass::Diagnostic
                || self.producing_job_id.is_none()
                || self.environment_hash.is_none()))
            || (self.evidence_kind == EvidenceKind::StatementFidelityReview
                && (self.authority_class != EvidenceAuthorityClass::Reviewed
                    || self.producing_run_id.is_none()
                    || self.producing_job_id.is_some()
                    || self.environment_hash.is_some()))
            || self
                .producing_run_id
                .as_deref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
            || self
                .producing_job_id
                .as_deref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
            || self
                .supersedes_evidence_id
                .as_deref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_INVALID",
                "evidence does not satisfy the closed canonical contract",
                false,
                "Use exact bounded references and consistent staleness metadata.",
            ));
        }
        Ok(())
    }

    pub fn evidence_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            AppError::new(
                "MCL_EVIDENCE_INVALID",
                error.to_string(),
                false,
                "Report this deterministic evidence serialization defect.",
            )
        })?)
    }
}

pub fn evidence_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/evidence/1",
        "title": "MathOS Evidence Payload v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "subject", "evidence_kind", "result", "authority_class", "producing_run_id", "producing_job_id", "artifact_hashes", "verifier_or_reviewer_identity", "environment_hash", "supersedes_evidence_id", "stale", "stale_reason"],
        "properties": {
            "schema_version": {"const": EVIDENCE_SCHEMA_VERSION},
            "subject": {"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}}},
            "evidence_kind": {"enum": ["lean_elaboration", "lean_kernel_proof", "lean_kernel_refutation", "certificate_replay", "bounded_computation", "empirical_search", "statement_fidelity_review", "literature_review", "novelty_review", "axiom_audit", "proof_closure_scan", "clean_rebuild", "comparator_run", "human_review"]},
            "result": {"enum": ["accepted", "rejected", "inconclusive", "failed"]},
            "authority_class": {"enum": ["diagnostic", "empirical", "reviewed", "authoritative"]},
            "producing_run_id": {"type": ["string", "null"], "format": "uuid"},
            "producing_job_id": {"type": ["string", "null"], "format": "uuid"},
            "artifact_hashes": {"type": "array", "maxItems": 256, "items": {"type": "string", "pattern": "^[0-9a-f]{64}$"}},
            "verifier_or_reviewer_identity": {"type": "string", "minLength": 1, "maxLength": 256},
            "environment_hash": {"type": ["string", "null"], "pattern": "^[0-9a-f]{64}$"},
            "supersedes_evidence_id": {"type": ["string", "null"], "format": "uuid"},
            "stale": {"type": "boolean"},
            "stale_reason": {"type": ["string", "null"], "minLength": 1, "maxLength": 4096}
        }
    })
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_schema_matches_closed_type_and_authority_is_not_inferred() {
        let committed: Value = serde_json::from_str(include_str!(
            "../../schemas/evidence/evidence-1.schema.json"
        ))
        .expect("committed evidence schema");
        assert_eq!(committed, evidence_schema());

        let payload: EvidencePayload = serde_json::from_value(json!({
            "schema_version": "evidence/1",
            "subject": {"object_id": uuid::Uuid::now_v7(), "version_hash": "a".repeat(64)},
            "evidence_kind": "lean_elaboration",
            "result": "accepted",
            "authority_class": "diagnostic",
            "producing_run_id": null,
            "producing_job_id": uuid::Uuid::now_v7(),
            "artifact_hashes": ["b".repeat(64)],
            "verifier_or_reviewer_identity": "local-lean-worker",
            "environment_hash": "c".repeat(64),
            "supersedes_evidence_id": null,
            "stale": false,
            "stale_reason": null
        }))
        .expect("diagnostic evidence decodes");
        payload.validate().expect("diagnostic evidence validates");
        assert_eq!(payload.authority_class, EvidenceAuthorityClass::Diagnostic);

        let mut forged_authority = payload.clone();
        forged_authority.authority_class = EvidenceAuthorityClass::Authoritative;
        assert_eq!(
            forged_authority
                .validate()
                .expect_err("elaboration cannot promote itself to authority")
                .code,
            "MCL_EVIDENCE_INVALID"
        );
    }
}
