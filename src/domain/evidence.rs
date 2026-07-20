use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::schemas::ExactVersionReference;
use crate::error::AppError;

pub const EVIDENCE_SCHEMA_VERSION: &str = "evidence/1";
pub const AUTHORITATIVE_EVIDENCE_SCHEMA_VERSION: &str = "evidence/2";
pub const PUBLICATION_AUTHORITY_BINDING_SCHEMA_VERSION: &str = "publication_authority_binding/1";

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationAuthorityBinding {
    pub schema_version: String,
    pub ingestion_receipt_hash: String,
    pub stage_hash: String,
    pub report_artifact_hash: String,
    pub retained_closure_artifact_hash: String,
    pub attestation_bundle_artifact_hash: String,
    pub raw_verification_hash: String,
    pub publication_request_hash: String,
    pub publication_policy_hash: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_authority: Option<PublicationAuthorityBinding>,
}

impl PublicationAuthorityBinding {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != PUBLICATION_AUTHORITY_BINDING_SCHEMA_VERSION
            || !is_hash(&self.ingestion_receipt_hash)
            || !is_hash(&self.stage_hash)
            || !is_hash(&self.report_artifact_hash)
            || !is_hash(&self.retained_closure_artifact_hash)
            || !is_hash(&self.attestation_bundle_artifact_hash)
            || !is_hash(&self.raw_verification_hash)
            || !is_hash(&self.publication_request_hash)
            || !is_hash(&self.publication_policy_hash)
        {
            return Err(AppError::new(
                "MCL_PUBLICATION_AUTHORITY_BINDING_INVALID",
                "publication authority binding does not identify one exact ingested publication closure",
                false,
                "Use only the application-derived receipt, stage, report, closure, bundle, verification, request, and policy hashes.",
            ));
        }
        Ok(())
    }
}

impl EvidencePayload {
    pub fn validate(&self) -> Result<(), AppError> {
        let is_v1 = self.schema_version == EVIDENCE_SCHEMA_VERSION;
        let is_v2 = self.schema_version == AUTHORITATIVE_EVIDENCE_SCHEMA_VERSION;
        if (!is_v1 && !is_v2)
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
            || (is_v1 && self.producing_run_id.is_none() && self.producing_job_id.is_none())
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
            || (is_v1
                && (matches!(
                    self.evidence_kind,
                    EvidenceKind::LeanKernelProof | EvidenceKind::LeanKernelRefutation
                ) || self.authority_class == EvidenceAuthorityClass::Authoritative
                    || self.publication_authority.is_some()))
            || (is_v2
                && (!matches!(
                    self.evidence_kind,
                    EvidenceKind::LeanKernelProof | EvidenceKind::LeanKernelRefutation
                ) || self.result != EvidenceResult::Accepted
                    || self.authority_class != EvidenceAuthorityClass::Authoritative
                    || self.producing_run_id.is_some()
                    || self.producing_job_id.is_some()
                    || self.environment_hash.is_none()
                    || self.supersedes_evidence_id.is_some()
                    || self.stale
                    || self.stale_reason.is_some()
                    || self.publication_authority.is_none()))
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_INVALID",
                "evidence does not satisfy the closed canonical contract",
                false,
                "Use exact bounded references and consistent staleness metadata.",
            ));
        }
        if let Some(binding) = &self.publication_authority {
            binding.validate()?;
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

pub fn authoritative_evidence_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/evidence/2",
        "title": "MathOS Authoritative Evidence Payload v2",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "subject", "evidence_kind", "result", "authority_class", "producing_run_id", "producing_job_id", "artifact_hashes", "verifier_or_reviewer_identity", "environment_hash", "supersedes_evidence_id", "stale", "stale_reason", "publication_authority"],
        "properties": {
            "schema_version": {"const": AUTHORITATIVE_EVIDENCE_SCHEMA_VERSION},
            "subject": {"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}}},
            "evidence_kind": {"enum": ["lean_kernel_proof", "lean_kernel_refutation"]},
            "result": {"const": "accepted"},
            "authority_class": {"const": "authoritative"},
            "producing_run_id": {"type": "null"},
            "producing_job_id": {"type": "null"},
            "artifact_hashes": {"type": "array", "maxItems": 256, "items": {"type": "string", "pattern": "^[0-9a-f]{64}$"}},
            "verifier_or_reviewer_identity": {"type": "string", "minLength": 1, "maxLength": 256},
            "environment_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "supersedes_evidence_id": {"type": "null"},
            "stale": {"const": false},
            "stale_reason": {"type": "null"},
            "publication_authority": {
                "type": "object",
                "additionalProperties": false,
                "required": ["schema_version", "ingestion_receipt_hash", "stage_hash", "report_artifact_hash", "retained_closure_artifact_hash", "attestation_bundle_artifact_hash", "raw_verification_hash", "publication_request_hash", "publication_policy_hash"],
                "properties": {
                    "schema_version": {"const": PUBLICATION_AUTHORITY_BINDING_SCHEMA_VERSION},
                    "ingestion_receipt_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "stage_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "report_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "retained_closure_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "attestation_bundle_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "raw_verification_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "publication_request_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "publication_policy_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
                }
            }
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

    fn publication_authority_binding() -> PublicationAuthorityBinding {
        PublicationAuthorityBinding {
            schema_version: PUBLICATION_AUTHORITY_BINDING_SCHEMA_VERSION.to_owned(),
            ingestion_receipt_hash: "1".repeat(64),
            stage_hash: "2".repeat(64),
            report_artifact_hash: "3".repeat(64),
            retained_closure_artifact_hash: "4".repeat(64),
            attestation_bundle_artifact_hash: "5".repeat(64),
            raw_verification_hash: "6".repeat(64),
            publication_request_hash: "a".repeat(64),
            publication_policy_hash: "b".repeat(64),
        }
    }

    fn authoritative_payload(kind: EvidenceKind) -> EvidencePayload {
        EvidencePayload {
            schema_version: AUTHORITATIVE_EVIDENCE_SCHEMA_VERSION.to_owned(),
            subject: ExactVersionReference {
                object_id: uuid::Uuid::now_v7().to_string(),
                version_hash: "c".repeat(64),
            },
            evidence_kind: kind,
            result: EvidenceResult::Accepted,
            authority_class: EvidenceAuthorityClass::Authoritative,
            producing_run_id: None,
            producing_job_id: None,
            artifact_hashes: vec!["d".repeat(64)],
            verifier_or_reviewer_identity: "publication-authority-gate".to_owned(),
            environment_hash: Some("e".repeat(64)),
            supersedes_evidence_id: None,
            stale: false,
            stale_reason: None,
            publication_authority: Some(publication_authority_binding()),
        }
    }

    fn expect_evidence_invalid(payload: &EvidencePayload) {
        assert_eq!(
            payload
                .validate()
                .expect_err("invalid authoritative evidence must fail")
                .code,
            "MCL_EVIDENCE_INVALID"
        );
    }

    #[test]
    fn committed_schema_matches_closed_type_and_authority_is_not_inferred() {
        let committed: Value = serde_json::from_str(include_str!(
            "../../schemas/evidence/evidence-1.schema.json"
        ))
        .expect("committed evidence schema");
        assert_eq!(committed, evidence_schema());

        let legacy_value = json!({
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
        });
        let payload: EvidencePayload =
            serde_json::from_value(legacy_value.clone()).expect("diagnostic evidence decodes");
        payload.validate().expect("diagnostic evidence validates");
        assert_eq!(payload.authority_class, EvidenceAuthorityClass::Diagnostic);
        let reproduced = serde_json::to_value(&payload).expect("legacy evidence serializes");
        assert_eq!(
            crate::canonical::canonical_json(&reproduced).expect("reproduced canonical evidence"),
            crate::canonical::canonical_json(&legacy_value).expect("original canonical evidence"),
            "evidence/1 canonical bytes must not gain the optional evidence/2 field"
        );

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

    #[test]
    fn committed_authoritative_schema_matches_the_closed_rust_contract() {
        let committed: Value = serde_json::from_str(include_str!(
            "../../schemas/evidence/evidence-2.schema.json"
        ))
        .expect("committed authoritative evidence schema");
        assert_eq!(committed, authoritative_evidence_schema());

        let schema_binding = &committed["properties"]["publication_authority"];
        assert_eq!(schema_binding["required"].as_array().map(Vec::len), Some(9));
        assert_eq!(schema_binding["additionalProperties"], false);
    }

    #[test]
    fn evidence_v2_allows_only_bound_accepted_authoritative_kernel_results() {
        let proof = authoritative_payload(EvidenceKind::LeanKernelProof);
        proof.validate().expect("bound proof authority validates");
        proof
            .evidence_hash()
            .expect("bound proof authority has a canonical identity");

        let refutation = authoritative_payload(EvidenceKind::LeanKernelRefutation);
        refutation
            .validate()
            .expect("bound refutation authority validates");

        let mut wrong_result = proof.clone();
        wrong_result.result = EvidenceResult::Rejected;
        expect_evidence_invalid(&wrong_result);

        let mut wrong_kind = proof.clone();
        wrong_kind.evidence_kind = EvidenceKind::CertificateReplay;
        expect_evidence_invalid(&wrong_kind);

        let mut wrong_class = proof.clone();
        wrong_class.authority_class = EvidenceAuthorityClass::Reviewed;
        expect_evidence_invalid(&wrong_class);

        let mut run_authored = proof.clone();
        run_authored.producing_run_id = Some(uuid::Uuid::now_v7().to_string());
        expect_evidence_invalid(&run_authored);

        let mut job_authored = proof.clone();
        job_authored.producing_job_id = Some(uuid::Uuid::now_v7().to_string());
        expect_evidence_invalid(&job_authored);

        let mut environment_missing = proof.clone();
        environment_missing.environment_hash = None;
        expect_evidence_invalid(&environment_missing);

        let mut superseding = proof.clone();
        superseding.supersedes_evidence_id = Some(uuid::Uuid::now_v7().to_string());
        expect_evidence_invalid(&superseding);

        let mut stale = proof.clone();
        stale.stale = true;
        stale.stale_reason = Some("caller attempted to pre-stale authority".to_owned());
        expect_evidence_invalid(&stale);

        let mut unbound = proof;
        unbound.publication_authority = None;
        expect_evidence_invalid(&unbound);
    }

    #[test]
    fn evidence_v1_cannot_encode_kernel_authority_or_a_publication_binding() {
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
        .expect("legacy evidence decodes without the optional v2 field");

        let mut kernel = payload.clone();
        kernel.evidence_kind = EvidenceKind::LeanKernelProof;
        expect_evidence_invalid(&kernel);

        let mut authoritative = payload.clone();
        authoritative.authority_class = EvidenceAuthorityClass::Authoritative;
        expect_evidence_invalid(&authoritative);

        let mut bound = payload;
        bound.publication_authority = Some(publication_authority_binding());
        expect_evidence_invalid(&bound);
    }

    #[test]
    fn publication_authority_binding_is_closed_and_requires_exact_hashes() {
        publication_authority_binding()
            .validate()
            .expect("exact binding validates");

        let mut wrong_schema = publication_authority_binding();
        wrong_schema.schema_version = "publication_authority_binding/2".to_owned();
        assert_eq!(
            wrong_schema
                .validate()
                .expect_err("wrong binding schema fails")
                .code,
            "MCL_PUBLICATION_AUTHORITY_BINDING_INVALID"
        );

        let mut wrong_hash = authoritative_payload(EvidenceKind::LeanKernelProof);
        wrong_hash
            .publication_authority
            .as_mut()
            .expect("binding")
            .raw_verification_hash = "A".repeat(64);
        assert_eq!(
            wrong_hash
                .validate()
                .expect_err("noncanonical binding hash fails")
                .code,
            "MCL_PUBLICATION_AUTHORITY_BINDING_INVALID"
        );

        let mut unknown =
            serde_json::to_value(publication_authority_binding()).expect("binding serializes");
        unknown["caller_authoritative"] = json!(true);
        assert!(
            serde_json::from_value::<PublicationAuthorityBinding>(unknown).is_err(),
            "binding rejects caller-authored unknown fields"
        );
    }
}
