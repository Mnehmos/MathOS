use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::evidence::EvidenceSnapshot;
use crate::domain::schemas::ExactVersionReference;
use crate::error::AppError;

pub const FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION: &str = "fidelity_review_request/1";
pub const FIDELITY_REVIEW_REPORT_SCHEMA_VERSION: &str = "fidelity_review_report/1";
const MAX_TEXT: usize = 65_536;
const MAX_ITEMS: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FidelityReviewLevel {
    SurfaceSyntax,
    MathematicalStatement,
    DefinitionMapping,
    SourcePaperCorrespondence,
    BenchmarkHashAlignment,
    ExpertDomainReview,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FidelityVerdict {
    Attested,
    BenchmarkAligned,
    Verified,
    Rejected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FidelityStatus {
    Unreviewed,
    Attested,
    BenchmarkAligned,
    Verified,
    Rejected,
    Superseded,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityDisposition {
    NoAmbiguity,
    PreservedVariants,
    ResolvedFromSource,
    Unresolved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionMapping {
    pub source_term: String,
    pub formal_declaration: String,
    pub notes: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FidelityReviewRequest {
    pub schema_version: String,
    pub source: ExactVersionReference,
    pub claim: ExactVersionReference,
    pub formalization: ExactVersionReference,
    pub review_level: FidelityReviewLevel,
    pub verdict: FidelityVerdict,
    pub reviewer_identity: String,
    pub findings: Vec<String>,
    pub ambiguity_disposition: AmbiguityDisposition,
    pub definition_mappings: Vec<DefinitionMapping>,
    pub supporting_artifact_hashes: Vec<String>,
    pub producing_run_id: String,
    pub supersedes_evidence_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FidelityReviewReport {
    pub schema_version: String,
    pub request_hash: String,
    pub request: FidelityReviewRequest,
    pub formalization_author: String,
    pub exact_theorem_type: String,
    pub declaration_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FidelityReviewHistoryEntry {
    pub status: FidelityStatus,
    pub evidence: EvidenceSnapshot,
    pub report_artifact_hash: String,
    pub report: FidelityReviewReport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FidelityStatusSnapshot {
    pub formalization: ExactVersionReference,
    pub status: FidelityStatus,
    pub head_evidence_id: Option<String>,
    pub history: Vec<FidelityReviewHistoryEntry>,
}

impl FidelityReviewRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        let references = [&self.source, &self.claim, &self.formalization];
        let verdict_level_valid = match self.verdict {
            FidelityVerdict::Attested | FidelityVerdict::Rejected => true,
            FidelityVerdict::BenchmarkAligned => {
                self.review_level == FidelityReviewLevel::BenchmarkHashAlignment
            }
            FidelityVerdict::Verified => {
                self.review_level != FidelityReviewLevel::SurfaceSyntax
                    && self.review_level != FidelityReviewLevel::BenchmarkHashAlignment
                    && self.ambiguity_disposition != AmbiguityDisposition::Unresolved
            }
        };
        if self.schema_version != FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION
            || references.iter().any(|reference| {
                uuid::Uuid::parse_str(&reference.object_id).is_err()
                    || !is_hash(&reference.version_hash)
            })
            || self.reviewer_identity.trim().is_empty()
            || self.reviewer_identity.len() > 256
            || self.findings.is_empty()
            || !bounded_texts(&self.findings)
            || self.definition_mappings.len() > MAX_ITEMS
            || self.definition_mappings.iter().any(|mapping| {
                !bounded_nonempty(&mapping.source_term)
                    || !bounded_nonempty(&mapping.formal_declaration)
                    || mapping.notes.len() > MAX_TEXT
            })
            || (self.review_level == FidelityReviewLevel::DefinitionMapping
                && self.definition_mappings.is_empty())
            || self.supporting_artifact_hashes.len() > MAX_ITEMS
            || self
                .supporting_artifact_hashes
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self
                .supporting_artifact_hashes
                .iter()
                .any(|hash| !is_hash(hash))
            || uuid::Uuid::parse_str(&self.producing_run_id).is_err()
            || self
                .supersedes_evidence_id
                .as_deref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
            || !verdict_level_valid
        {
            return Err(fidelity_error(
                "MCL_FIDELITY_REQUEST_INVALID",
                "fidelity review request does not satisfy the closed semantic contract",
                "Use exact versions, a compatible review level and verdict, explicit findings, and bounded canonical references.",
            ));
        }
        Ok(())
    }

    pub fn request_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            fidelity_error(
                "MCL_FIDELITY_REQUEST_INVALID",
                error.to_string(),
                "Report this deterministic fidelity request serialization defect.",
            )
        })?)
    }
}

impl FidelityReviewReport {
    pub fn validate(&self) -> Result<(), AppError> {
        self.request.validate()?;
        if self.schema_version != FIDELITY_REVIEW_REPORT_SCHEMA_VERSION
            || self.request_hash != self.request.request_hash()?
            || self.formalization_author.trim().is_empty()
            || self.formalization_author.len() > 256
            || !bounded_nonempty(&self.exact_theorem_type)
            || !is_hash(&self.declaration_hash)
            || (self.request.verdict == FidelityVerdict::Verified
                && self.request.reviewer_identity == self.formalization_author)
        {
            return Err(fidelity_error(
                "MCL_FIDELITY_REPORT_INVALID",
                "fidelity report is inconsistent, unbounded, or violates reviewer role separation",
                "Use an independent reviewer for verified fidelity and preserve the exact reviewed statement identity.",
            ));
        }
        Ok(())
    }
}

pub fn fidelity_review_request_schema() -> Value {
    let reference = exact_reference_schema();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/fidelity/review-request/1",
        "title": "MathOS Fidelity Review Request v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "source", "claim", "formalization", "review_level", "verdict", "reviewer_identity", "findings", "ambiguity_disposition", "definition_mappings", "supporting_artifact_hashes", "producing_run_id", "supersedes_evidence_id"],
        "properties": {
            "schema_version": {"const": FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION},
            "source": reference.clone(),
            "claim": reference.clone(),
            "formalization": reference,
            "review_level": {"enum": ["surface_syntax", "mathematical_statement", "definition_mapping", "source_paper_correspondence", "benchmark_hash_alignment", "expert_domain_review"]},
            "verdict": {"enum": ["attested", "benchmark_aligned", "verified", "rejected"]},
            "reviewer_identity": {"type": "string", "minLength": 1, "maxLength": 256},
            "findings": {"type": "array", "minItems": 1, "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT}},
            "ambiguity_disposition": {"enum": ["no_ambiguity", "preserved_variants", "resolved_from_source", "unresolved"]},
            "definition_mappings": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "object", "additionalProperties": false, "required": ["source_term", "formal_declaration", "notes"], "properties": {"source_term": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT}, "formal_declaration": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT}, "notes": {"type": "string", "maxLength": MAX_TEXT}}}},
            "supporting_artifact_hashes": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "pattern": "^[0-9a-f]{64}$"}},
            "producing_run_id": {"type": "string", "format": "uuid"},
            "supersedes_evidence_id": {"type": ["string", "null"], "format": "uuid"}
        }
    })
}

pub fn fidelity_review_report_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/fidelity/review-report/1",
        "title": "MathOS Fidelity Review Report v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "request_hash", "request", "formalization_author", "exact_theorem_type", "declaration_hash"],
        "properties": {
            "schema_version": {"const": FIDELITY_REVIEW_REPORT_SCHEMA_VERSION},
            "request_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "request": {"$ref": "https://mnehmos.ai/mathos/schemas/fidelity/review-request/1"},
            "formalization_author": {"type": "string", "minLength": 1, "maxLength": 256},
            "exact_theorem_type": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT},
            "declaration_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        }
    })
}

fn exact_reference_schema() -> Value {
    json!({"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}}})
}

fn bounded_nonempty(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_TEXT
}

fn bounded_texts(values: &[String]) -> bool {
    values.len() <= MAX_ITEMS && values.iter().all(|value| bounded_nonempty(value))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn fidelity_error(
    code: &'static str,
    message: impl Into<String>,
    action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> FidelityReviewRequest {
        let reference = || ExactVersionReference {
            object_id: uuid::Uuid::now_v7().to_string(),
            version_hash: "a".repeat(64),
        };
        FidelityReviewRequest {
            schema_version: FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION.to_owned(),
            source: reference(),
            claim: reference(),
            formalization: reference(),
            review_level: FidelityReviewLevel::MathematicalStatement,
            verdict: FidelityVerdict::Verified,
            reviewer_identity: "independent-reviewer".to_owned(),
            findings: vec!["Quantifiers, domain, and conclusion match the source.".to_owned()],
            ambiguity_disposition: AmbiguityDisposition::NoAmbiguity,
            definition_mappings: Vec::new(),
            supporting_artifact_hashes: Vec::new(),
            producing_run_id: uuid::Uuid::now_v7().to_string(),
            supersedes_evidence_id: None,
        }
    }

    #[test]
    fn incompatible_verdicts_and_ambiguous_verified_reviews_fail_closed() {
        let mut surface = request();
        surface.review_level = FidelityReviewLevel::SurfaceSyntax;
        assert_eq!(
            surface
                .validate()
                .expect_err("surface review cannot verify")
                .code,
            "MCL_FIDELITY_REQUEST_INVALID"
        );
        let mut ambiguous = request();
        ambiguous.ambiguity_disposition = AmbiguityDisposition::Unresolved;
        assert_eq!(
            ambiguous
                .validate()
                .expect_err("unresolved ambiguity cannot verify")
                .code,
            "MCL_FIDELITY_REQUEST_INVALID"
        );
        let mut benchmark = request();
        benchmark.verdict = FidelityVerdict::BenchmarkAligned;
        assert_eq!(
            benchmark
                .validate()
                .expect_err("benchmark verdict needs benchmark review")
                .code,
            "MCL_FIDELITY_REQUEST_INVALID"
        );
    }

    #[test]
    fn report_enforces_role_separation_and_exact_request_identity() {
        let request = request();
        let mut report = FidelityReviewReport {
            schema_version: FIDELITY_REVIEW_REPORT_SCHEMA_VERSION.to_owned(),
            request_hash: request.request_hash().expect("request hash"),
            request,
            formalization_author: "formalizer".to_owned(),
            exact_theorem_type: "True".to_owned(),
            declaration_hash: "b".repeat(64),
        };
        report.validate().expect("independent review validates");
        report.formalization_author = report.request.reviewer_identity.clone();
        assert_eq!(
            report
                .validate()
                .expect_err("self review cannot verify")
                .code,
            "MCL_FIDELITY_REPORT_INVALID"
        );
        report.request.verdict = FidelityVerdict::Attested;
        report.request_hash = report.request.request_hash().expect("attestation hash");
        report
            .validate()
            .expect("author may attest without verifying");
    }

    #[test]
    fn committed_schemas_match_the_closed_rust_contracts() {
        let request: Value = serde_json::from_str(include_str!(
            "../../schemas/fidelity/fidelity-review-request-1.schema.json"
        ))
        .expect("request schema");
        let report: Value = serde_json::from_str(include_str!(
            "../../schemas/fidelity/fidelity-review-report-1.schema.json"
        ))
        .expect("report schema");
        assert_eq!(request, fidelity_review_request_schema());
        assert_eq!(report, fidelity_review_report_schema());
    }
}
