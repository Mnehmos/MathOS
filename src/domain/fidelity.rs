use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::evidence::EvidenceSnapshot;
use crate::domain::schemas::{ExactVersionReference, FormalizationClaimPolarity};
use crate::error::AppError;

pub const FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION: &str = "fidelity_review_request/1";
pub const FIDELITY_REVIEW_REPORT_SCHEMA_VERSION: &str = "fidelity_review_report/1";
pub const FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION: &str = "fidelity_review_request/2";
pub const FIDELITY_REVIEW_REPORT_V2_SCHEMA_VERSION: &str = "fidelity_review_report/2";
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

/// The source-level proposition that a fidelity reviewer compared with the
/// exact formal declaration.
///
/// This is deliberately distinct from [`FormalizationClaimPolarity`]. The
/// latter is immutable canonical formalization metadata; this value records
/// the independent reviewer's attestation about what the declaration means.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewedSourceRelation {
    Claim,
    LogicalNegation,
}

impl ReviewedSourceRelation {
    pub const fn matches_formalization_polarity(
        self,
        polarity: Option<FormalizationClaimPolarity>,
    ) -> bool {
        matches!(
            (self, polarity),
            (Self::Claim, Some(FormalizationClaimPolarity::Claim))
                | (
                    Self::LogicalNegation,
                    Some(FormalizationClaimPolarity::Negation)
                )
        )
    }
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

/// Polarity-aware fidelity request. Version 1 remains a separate immutable
/// contract so its canonical values and hashes cannot be reinterpreted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FidelityReviewRequestV2 {
    pub schema_version: String,
    pub source: ExactVersionReference,
    pub claim: ExactVersionReference,
    pub formalization: ExactVersionReference,
    pub reviewed_source_relation: ReviewedSourceRelation,
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
pub struct FidelityReviewReportV2 {
    pub schema_version: String,
    pub request_hash: String,
    pub request: FidelityReviewRequestV2,
    pub reviewed_source_relation: ReviewedSourceRelation,
    pub formalization_author: String,
    pub exact_theorem_type: String,
    pub declaration_hash: String,
}

/// Parse either committed fidelity request without changing the serialized
/// representation of the selected inner contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum VersionedFidelityReviewRequest {
    V1(FidelityReviewRequest),
    V2(FidelityReviewRequestV2),
}

/// Parse either committed fidelity report without adding a discriminator to
/// its canonical JSON.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum VersionedFidelityReviewReport {
    V1(FidelityReviewReport),
    V2(FidelityReviewReportV2),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FidelityReviewHistoryEntry {
    pub status: FidelityStatus,
    pub evidence: EvidenceSnapshot,
    pub report_artifact_hash: String,
    pub report: VersionedFidelityReviewReport,
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

impl FidelityReviewRequestV2 {
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
        if self.schema_version != FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION
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
                "fidelity review request does not satisfy the closed polarity-aware semantic contract",
                "Use exact versions, an explicit reviewed source relation, a compatible review level and verdict, explicit findings, and bounded canonical references.",
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

impl FidelityReviewReportV2 {
    pub fn validate(&self) -> Result<(), AppError> {
        self.request.validate()?;
        if self.schema_version != FIDELITY_REVIEW_REPORT_V2_SCHEMA_VERSION
            || self.request_hash != self.request.request_hash()?
            || self.reviewed_source_relation != self.request.reviewed_source_relation
            || self.formalization_author.trim().is_empty()
            || self.formalization_author.len() > 256
            || !bounded_nonempty(&self.exact_theorem_type)
            || !is_hash(&self.declaration_hash)
            || (self.request.verdict == FidelityVerdict::Verified
                && self.request.reviewer_identity == self.formalization_author)
        {
            return Err(fidelity_error(
                "MCL_FIDELITY_REPORT_INVALID",
                "fidelity report is inconsistent, unbounded, or violates polarity binding or reviewer role separation",
                "Bind the report to the request's reviewed source relation, use an independent reviewer for verified fidelity, and preserve the exact reviewed statement identity.",
            ));
        }
        Ok(())
    }
}

impl VersionedFidelityReviewRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::V1(request) => request.validate(),
            Self::V2(request) => request.validate(),
        }
    }

    pub fn request_hash(&self) -> Result<String, AppError> {
        match self {
            Self::V1(request) => request.request_hash(),
            Self::V2(request) => request.request_hash(),
        }
    }

    pub fn schema_version(&self) -> &str {
        match self {
            Self::V1(request) => &request.schema_version,
            Self::V2(request) => &request.schema_version,
        }
    }

    pub fn source(&self) -> &ExactVersionReference {
        match self {
            Self::V1(request) => &request.source,
            Self::V2(request) => &request.source,
        }
    }

    pub fn claim(&self) -> &ExactVersionReference {
        match self {
            Self::V1(request) => &request.claim,
            Self::V2(request) => &request.claim,
        }
    }

    pub fn formalization(&self) -> &ExactVersionReference {
        match self {
            Self::V1(request) => &request.formalization,
            Self::V2(request) => &request.formalization,
        }
    }

    pub const fn reviewed_source_relation(&self) -> Option<ReviewedSourceRelation> {
        match self {
            Self::V1(_) => None,
            Self::V2(request) => Some(request.reviewed_source_relation),
        }
    }

    pub const fn review_level(&self) -> FidelityReviewLevel {
        match self {
            Self::V1(request) => request.review_level,
            Self::V2(request) => request.review_level,
        }
    }

    pub const fn verdict(&self) -> FidelityVerdict {
        match self {
            Self::V1(request) => request.verdict,
            Self::V2(request) => request.verdict,
        }
    }

    pub fn reviewer_identity(&self) -> &str {
        match self {
            Self::V1(request) => &request.reviewer_identity,
            Self::V2(request) => &request.reviewer_identity,
        }
    }

    pub fn findings(&self) -> &[String] {
        match self {
            Self::V1(request) => &request.findings,
            Self::V2(request) => &request.findings,
        }
    }

    pub const fn ambiguity_disposition(&self) -> AmbiguityDisposition {
        match self {
            Self::V1(request) => request.ambiguity_disposition,
            Self::V2(request) => request.ambiguity_disposition,
        }
    }

    pub fn definition_mappings(&self) -> &[DefinitionMapping] {
        match self {
            Self::V1(request) => &request.definition_mappings,
            Self::V2(request) => &request.definition_mappings,
        }
    }

    pub fn supporting_artifact_hashes(&self) -> &[String] {
        match self {
            Self::V1(request) => &request.supporting_artifact_hashes,
            Self::V2(request) => &request.supporting_artifact_hashes,
        }
    }

    pub fn producing_run_id(&self) -> &str {
        match self {
            Self::V1(request) => &request.producing_run_id,
            Self::V2(request) => &request.producing_run_id,
        }
    }

    pub fn supersedes_evidence_id(&self) -> Option<&str> {
        match self {
            Self::V1(request) => request.supersedes_evidence_id.as_deref(),
            Self::V2(request) => request.supersedes_evidence_id.as_deref(),
        }
    }
}

impl VersionedFidelityReviewReport {
    pub fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::V1(report) => report.validate(),
            Self::V2(report) => report.validate(),
        }
    }

    pub fn schema_version(&self) -> &str {
        match self {
            Self::V1(report) => &report.schema_version,
            Self::V2(report) => &report.schema_version,
        }
    }

    pub fn request_hash(&self) -> &str {
        match self {
            Self::V1(report) => &report.request_hash,
            Self::V2(report) => &report.request_hash,
        }
    }

    pub fn request(&self) -> VersionedFidelityReviewRequest {
        match self {
            Self::V1(report) => VersionedFidelityReviewRequest::V1(report.request.clone()),
            Self::V2(report) => VersionedFidelityReviewRequest::V2(report.request.clone()),
        }
    }

    pub const fn reviewed_source_relation(&self) -> Option<ReviewedSourceRelation> {
        match self {
            // Version 1 recorded no reviewer-authored relation. Its separate
            // compatibility rule may qualify only a claim-polarity proof.
            Self::V1(_) => None,
            Self::V2(report) => Some(report.reviewed_source_relation),
        }
    }

    pub fn formalization_author(&self) -> &str {
        match self {
            Self::V1(report) => &report.formalization_author,
            Self::V2(report) => &report.formalization_author,
        }
    }

    pub fn exact_theorem_type(&self) -> &str {
        match self {
            Self::V1(report) => &report.exact_theorem_type,
            Self::V2(report) => &report.exact_theorem_type,
        }
    }

    pub fn declaration_hash(&self) -> &str {
        match self {
            Self::V1(report) => &report.declaration_hash,
            Self::V2(report) => &report.declaration_hash,
        }
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

pub fn fidelity_review_request_v2_schema() -> Value {
    let reference = exact_reference_schema();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/fidelity/review-request/2",
        "title": "MathOS Fidelity Review Request v2",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "source", "claim", "formalization", "reviewed_source_relation", "review_level", "verdict", "reviewer_identity", "findings", "ambiguity_disposition", "definition_mappings", "supporting_artifact_hashes", "producing_run_id", "supersedes_evidence_id"],
        "properties": {
            "schema_version": {"const": FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION},
            "source": reference.clone(),
            "claim": reference.clone(),
            "formalization": reference,
            "reviewed_source_relation": {"enum": ["claim", "logical_negation"]},
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

pub fn fidelity_review_report_v2_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/fidelity/review-report/2",
        "title": "MathOS Fidelity Review Report v2",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "request_hash", "request", "reviewed_source_relation", "formalization_author", "exact_theorem_type", "declaration_hash"],
        "properties": {
            "schema_version": {"const": FIDELITY_REVIEW_REPORT_V2_SCHEMA_VERSION},
            "request_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "request": {"$ref": "https://mnehmos.ai/mathos/schemas/fidelity/review-request/2"},
            "reviewed_source_relation": {"enum": ["claim", "logical_negation"]},
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

    fn request_v2(relation: ReviewedSourceRelation) -> FidelityReviewRequestV2 {
        let request = request();
        FidelityReviewRequestV2 {
            schema_version: FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION.to_owned(),
            source: request.source,
            claim: request.claim,
            formalization: request.formalization,
            reviewed_source_relation: relation,
            review_level: request.review_level,
            verdict: request.verdict,
            reviewer_identity: request.reviewer_identity,
            findings: request.findings,
            ambiguity_disposition: request.ambiguity_disposition,
            definition_mappings: request.definition_mappings,
            supporting_artifact_hashes: request.supporting_artifact_hashes,
            producing_run_id: request.producing_run_id,
            supersedes_evidence_id: request.supersedes_evidence_id,
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
    fn v2_binds_the_report_to_the_reviewed_source_relation() {
        let request = request_v2(ReviewedSourceRelation::LogicalNegation);
        let mut report = FidelityReviewReportV2 {
            schema_version: FIDELITY_REVIEW_REPORT_V2_SCHEMA_VERSION.to_owned(),
            request_hash: request.request_hash().expect("request hash"),
            request,
            reviewed_source_relation: ReviewedSourceRelation::LogicalNegation,
            formalization_author: "formalizer".to_owned(),
            exact_theorem_type: "Not source_claim".to_owned(),
            declaration_hash: "b".repeat(64),
        };
        report.validate().expect("matching relation validates");

        report.reviewed_source_relation = ReviewedSourceRelation::Claim;
        assert_eq!(
            report
                .validate()
                .expect_err("report cannot invert the request relation")
                .code,
            "MCL_FIDELITY_REPORT_INVALID"
        );
    }

    #[test]
    fn reviewed_source_relation_matches_only_the_exact_formalization_polarity() {
        assert!(
            ReviewedSourceRelation::Claim
                .matches_formalization_polarity(Some(FormalizationClaimPolarity::Claim))
        );
        assert!(
            ReviewedSourceRelation::LogicalNegation
                .matches_formalization_polarity(Some(FormalizationClaimPolarity::Negation))
        );
        assert!(
            !ReviewedSourceRelation::Claim
                .matches_formalization_polarity(Some(FormalizationClaimPolarity::Negation))
        );
        assert!(!ReviewedSourceRelation::LogicalNegation.matches_formalization_polarity(None));
    }

    #[test]
    fn versioned_wrappers_preserve_v1_json_and_hash_identity() {
        let request = request();
        let wrapped = VersionedFidelityReviewRequest::V1(request.clone());
        assert_eq!(
            serde_json::to_value(&wrapped).expect("wrapped request"),
            serde_json::to_value(&request).expect("request")
        );
        assert_eq!(
            serde_json::to_vec(&wrapped).expect("wrapped request bytes"),
            serde_json::to_vec(&request).expect("request bytes")
        );
        assert_eq!(
            wrapped.request_hash().expect("wrapped request hash"),
            request.request_hash().expect("request hash")
        );
        assert_eq!(wrapped.reviewed_source_relation(), None);

        let report = FidelityReviewReport {
            schema_version: FIDELITY_REVIEW_REPORT_SCHEMA_VERSION.to_owned(),
            request_hash: request.request_hash().expect("request hash"),
            request,
            formalization_author: "formalizer".to_owned(),
            exact_theorem_type: "True".to_owned(),
            declaration_hash: "b".repeat(64),
        };
        let wrapped = VersionedFidelityReviewReport::V1(report.clone());
        assert_eq!(
            serde_json::to_value(&wrapped).expect("wrapped report"),
            serde_json::to_value(&report).expect("report")
        );
        assert_eq!(
            serde_json::to_vec(&wrapped).expect("wrapped report bytes"),
            serde_json::to_vec(&report).expect("report bytes")
        );
        assert_eq!(wrapped.reviewed_source_relation(), None);
        wrapped.validate().expect("wrapped v1 report validates");
    }

    #[test]
    fn committed_v1_request_fixture_keeps_its_canonical_identity() {
        let request = FidelityReviewRequest {
            schema_version: FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION.to_owned(),
            source: ExactVersionReference {
                object_id: "00000000-0000-0000-0000-000000000001".to_owned(),
                version_hash: "a".repeat(64),
            },
            claim: ExactVersionReference {
                object_id: "00000000-0000-0000-0000-000000000002".to_owned(),
                version_hash: "b".repeat(64),
            },
            formalization: ExactVersionReference {
                object_id: "00000000-0000-0000-0000-000000000003".to_owned(),
                version_hash: "c".repeat(64),
            },
            review_level: FidelityReviewLevel::MathematicalStatement,
            verdict: FidelityVerdict::Verified,
            reviewer_identity: "independent-reviewer".to_owned(),
            findings: vec!["Quantifiers and conclusion match.".to_owned()],
            ambiguity_disposition: AmbiguityDisposition::NoAmbiguity,
            definition_mappings: Vec::new(),
            supporting_artifact_hashes: vec!["d".repeat(64), "e".repeat(64)],
            producing_run_id: "00000000-0000-0000-0000-000000000004".to_owned(),
            supersedes_evidence_id: None,
        };
        assert_eq!(
            request.request_hash().expect("fixture hash"),
            "86f0def84ba927514ff8d3ee56b4602ab2b3419b9a13e59e230fdc61498d0fe5"
        );
        let value = serde_json::to_value(&request).expect("fixture value");
        assert!(value.get("reviewed_source_relation").is_none());

        let report = FidelityReviewReport {
            schema_version: FIDELITY_REVIEW_REPORT_SCHEMA_VERSION.to_owned(),
            request_hash: request.request_hash().expect("fixture request hash"),
            request,
            formalization_author: "formalizer".to_owned(),
            exact_theorem_type: "True".to_owned(),
            declaration_hash: "f".repeat(64),
        };
        report.validate().expect("fixture report validates");
        let report_value = serde_json::to_value(report).expect("fixture report value");
        assert!(report_value.get("reviewed_source_relation").is_none());
        assert_eq!(
            value_hash(&report_value).expect("fixture report hash"),
            "10f9789af050f2ac435871cd8979baa57e9932ab675843ff2ca2450b28287412"
        );
    }

    #[test]
    fn versioned_parser_distinguishes_closed_v1_and_v2_contracts() {
        let v1: VersionedFidelityReviewRequest =
            serde_json::from_value(serde_json::to_value(request()).expect("v1 request value"))
                .expect("parse v1");
        assert!(matches!(v1, VersionedFidelityReviewRequest::V1(_)));

        let v2_request = request_v2(ReviewedSourceRelation::Claim);
        let mut v2_value = serde_json::to_value(&v2_request).expect("v2 request value");
        let v2: VersionedFidelityReviewRequest =
            serde_json::from_value(v2_value.clone()).expect("parse v2");
        assert!(matches!(v2, VersionedFidelityReviewRequest::V2(_)));

        v2_value["reviewed_source_relation"] = Value::String("inverse".to_owned());
        assert!(serde_json::from_value::<VersionedFidelityReviewRequest>(v2_value).is_err());
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
        let request_v2: Value = serde_json::from_str(include_str!(
            "../../schemas/fidelity/fidelity-review-request-2.schema.json"
        ))
        .expect("request v2 schema");
        let report_v2: Value = serde_json::from_str(include_str!(
            "../../schemas/fidelity/fidelity-review-report-2.schema.json"
        ))
        .expect("report v2 schema");
        assert_eq!(request, fidelity_review_request_schema());
        assert_eq!(report, fidelity_review_report_schema());
        assert_eq!(request_v2, fidelity_review_request_v2_schema());
        assert_eq!(report_v2, fidelity_review_report_v2_schema());
    }
}
