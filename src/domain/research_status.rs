use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::fidelity::{
    FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION, FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION,
    ReviewedSourceRelation,
};
use crate::domain::schemas::ExactVersionReference;
use crate::error::AppError;

pub const CLAIM_RESEARCH_STATUS_SCHEMA_VERSION: &str = "claim_research_status/1";
pub const MAX_CLAIM_RESEARCH_STATUS_ITEMS: usize = 256;

/// Complete research-status vocabulary committed by SPEC section 10.3.
///
/// The first version of the derived-status service intentionally emits only
/// the subset whose evidence semantics are implemented. Keeping the complete
/// vocabulary here prevents callers from inventing incompatible spellings for
/// later evidence-backed states.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchStatus {
    NotStarted,
    Active,
    Open,
    ConditionallyResolved,
    Proved,
    Disproved,
    Malformed,
    Ambiguous,
    Superseded,
}

impl ResearchStatus {
    pub const ALL: [Self; 9] = [
        Self::NotStarted,
        Self::Active,
        Self::Open,
        Self::ConditionallyResolved,
        Self::Proved,
        Self::Disproved,
        Self::Malformed,
        Self::Ambiguous,
        Self::Superseded,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Active => "active",
            Self::Open => "open",
            Self::ConditionallyResolved => "conditionally_resolved",
            Self::Proved => "proved",
            Self::Disproved => "disproved",
            Self::Malformed => "malformed",
            Self::Ambiguous => "ambiguous",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimResearchStatusWitnessKind {
    Proof,
    Refutation,
}

impl ClaimResearchStatusWitnessKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proof => "proof",
            Self::Refutation => "refutation",
        }
    }

    pub const fn reviewed_source_relation(self) -> ReviewedSourceRelation {
        match self {
            Self::Proof => ReviewedSourceRelation::Claim,
            Self::Refutation => ReviewedSourceRelation::LogicalNegation,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimResearchStatusNonqualificationReason {
    SourceVersionNotCurrent,
    NoCurrentVerifiedFidelity,
    FidelityRelationUnbound,
    FidelityRelationMismatch,
    NoCurrentAuthoritativeEvidence,
    AuthorityKindMismatch,
    SourceAmbiguityUnresolved,
    SourceAmbiguityPreserved,
}

impl ClaimResearchStatusNonqualificationReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceVersionNotCurrent => "source_version_not_current",
            Self::NoCurrentVerifiedFidelity => "no_current_verified_fidelity",
            Self::FidelityRelationUnbound => "fidelity_relation_unbound",
            Self::FidelityRelationMismatch => "fidelity_relation_mismatch",
            Self::NoCurrentAuthoritativeEvidence => "no_current_authoritative_evidence",
            Self::AuthorityKindMismatch => "authority_kind_mismatch",
            Self::SourceAmbiguityUnresolved => "source_ambiguity_unresolved",
            Self::SourceAmbiguityPreserved => "source_ambiguity_preserved",
        }
    }
}

/// Complete audit locator for one independently revalidated status witness.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimResearchStatusWitness {
    pub formalization: ExactVersionReference,
    pub kind: ClaimResearchStatusWitnessKind,
    pub reviewed_source_relation: ReviewedSourceRelation,
    pub fidelity_request_schema_version: String,
    pub fidelity_evidence_id: String,
    pub fidelity_evidence_hash: String,
    pub fidelity_report_artifact_hash: String,
    pub authority_evidence_id: String,
    pub authority_evidence_hash: String,
    pub publication_receipt_hash: String,
}

/// Why one current formalization did not produce a qualifying witness.
/// Missing or corrupt canonical/CAS data is not represented here: derivation
/// must fail closed before a response is built in those cases.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimResearchStatusNonqualification {
    pub formalization: ExactVersionReference,
    pub reason: ClaimResearchStatusNonqualificationReason,
    pub fidelity_evidence_id: Option<String>,
    pub authority_evidence_id: Option<String>,
}

/// Live, read-only derivation for one exact claim version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimResearchStatusSnapshot {
    pub schema_version: String,
    pub claim: ExactVersionReference,
    pub status: ResearchStatus,
    pub witnesses: Vec<ClaimResearchStatusWitness>,
    pub nonqualifications: Vec<ClaimResearchStatusNonqualification>,
}

impl ClaimResearchStatusWitness {
    pub fn validate(&self) -> Result<(), AppError> {
        let relation_matches_kind =
            self.reviewed_source_relation == self.kind.reviewed_source_relation();
        let v1_is_claim_proof = self.fidelity_request_schema_version
            != FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION
            || (self.kind == ClaimResearchStatusWitnessKind::Proof
                && self.reviewed_source_relation == ReviewedSourceRelation::Claim);
        if !valid_reference(&self.formalization)
            || !relation_matches_kind
            || !v1_is_claim_proof
            || !matches!(
                self.fidelity_request_schema_version.as_str(),
                FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION | FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION
            )
            || uuid::Uuid::parse_str(&self.fidelity_evidence_id).is_err()
            || !is_hash(&self.fidelity_evidence_hash)
            || !is_hash(&self.fidelity_report_artifact_hash)
            || uuid::Uuid::parse_str(&self.authority_evidence_id).is_err()
            || !is_hash(&self.authority_evidence_hash)
            || !is_hash(&self.publication_receipt_hash)
        {
            return Err(research_status_error(
                "qualifying witness is not exact, polarity-consistent, or bound to supported evidence contracts",
                "Return only independently revalidated proof or refutation witnesses with exact UUID and SHA-256 identities.",
            ));
        }
        Ok(())
    }
}

impl ClaimResearchStatusNonqualification {
    pub fn validate(&self) -> Result<(), AppError> {
        let locators_match_reason = match self.reason {
            ClaimResearchStatusNonqualificationReason::SourceVersionNotCurrent => {
                self.fidelity_evidence_id.is_none() && self.authority_evidence_id.is_none()
            }
            ClaimResearchStatusNonqualificationReason::NoCurrentVerifiedFidelity
            | ClaimResearchStatusNonqualificationReason::SourceAmbiguityUnresolved => {
                self.authority_evidence_id.is_none()
            }
            ClaimResearchStatusNonqualificationReason::FidelityRelationUnbound
            | ClaimResearchStatusNonqualificationReason::FidelityRelationMismatch
            | ClaimResearchStatusNonqualificationReason::NoCurrentAuthoritativeEvidence
            | ClaimResearchStatusNonqualificationReason::SourceAmbiguityPreserved => {
                self.fidelity_evidence_id.is_some() && self.authority_evidence_id.is_none()
            }
            ClaimResearchStatusNonqualificationReason::AuthorityKindMismatch => {
                self.fidelity_evidence_id.is_some() && self.authority_evidence_id.is_some()
            }
        };
        if !valid_reference(&self.formalization)
            || !locators_match_reason
            || self
                .fidelity_evidence_id
                .as_deref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
            || self
                .authority_evidence_id
                .as_deref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err(research_status_error(
                "nonqualification is not bound to an exact current formalization or valid evidence locator",
                "Use exact formalization versions and canonical evidence UUIDs when identifying a nonqualifying path.",
            ));
        }
        Ok(())
    }
}

impl ClaimResearchStatusSnapshot {
    /// Put response components into the contract's canonical deterministic
    /// order before validation or serialization.
    pub fn sort_components(&mut self) {
        self.witnesses.sort_by(compare_witnesses);
        self.nonqualifications.sort_by(compare_nonqualifications);
    }

    pub fn validate(&self) -> Result<(), AppError> {
        let source_ambiguity = self.nonqualifications.iter().any(|item| {
            matches!(
                item.reason,
                ClaimResearchStatusNonqualificationReason::SourceAmbiguityUnresolved
                    | ClaimResearchStatusNonqualificationReason::SourceAmbiguityPreserved
            )
        });
        let has_proof = self
            .witnesses
            .iter()
            .any(|witness| witness.kind == ClaimResearchStatusWitnessKind::Proof);
        let has_refutation = self
            .witnesses
            .iter()
            .any(|witness| witness.kind == ClaimResearchStatusWitnessKind::Refutation);
        let status_matches_witnesses = match self.status {
            ResearchStatus::Proved => has_proof && !has_refutation && !source_ambiguity,
            ResearchStatus::Disproved => has_refutation && !has_proof && !source_ambiguity,
            ResearchStatus::Ambiguous => source_ambiguity || (has_proof && has_refutation),
            _ => self.witnesses.is_empty() && !source_ambiguity,
        };
        let empty_status_details_valid = match self.status {
            ResearchStatus::NotStarted | ResearchStatus::Superseded => {
                self.witnesses.is_empty() && self.nonqualifications.is_empty()
            }
            _ => true,
        };
        let open_has_reason =
            self.status != ResearchStatus::Open || !self.nonqualifications.is_empty();
        let same_formalization_conflict = self.witnesses.iter().enumerate().any(|(index, left)| {
            self.witnesses[index + 1..]
                .iter()
                .any(|right| left.formalization == right.formalization && left.kind != right.kind)
        });
        let duplicate_witness_formalization =
            self.witnesses.iter().enumerate().any(|(index, left)| {
                self.witnesses[index + 1..]
                    .iter()
                    .any(|right| left.formalization == right.formalization)
            });
        let duplicate_nonqualification_formalization = self
            .nonqualifications
            .iter()
            .enumerate()
            .any(|(index, left)| {
                self.nonqualifications[index + 1..]
                    .iter()
                    .any(|right| left.formalization == right.formalization)
            });
        let witness_nonqualification_overlap = self.witnesses.iter().any(|witness| {
            self.nonqualifications
                .iter()
                .any(|item| witness.formalization == item.formalization)
        });

        if self.schema_version != CLAIM_RESEARCH_STATUS_SCHEMA_VERSION
            || !valid_reference(&self.claim)
            || self.witnesses.len() > MAX_CLAIM_RESEARCH_STATUS_ITEMS
            || self.nonqualifications.len() > MAX_CLAIM_RESEARCH_STATUS_ITEMS
            || self
                .witnesses
                .iter()
                .any(|witness| witness.validate().is_err())
            || self
                .nonqualifications
                .iter()
                .any(|item| item.validate().is_err())
            || self
                .witnesses
                .windows(2)
                .any(|pair| compare_witnesses(&pair[0], &pair[1]) != Ordering::Less)
            || self
                .nonqualifications
                .windows(2)
                .any(|pair| compare_nonqualifications(&pair[0], &pair[1]) != Ordering::Less)
            || !status_matches_witnesses
            || !empty_status_details_valid
            || !open_has_reason
            || same_formalization_conflict
            || duplicate_witness_formalization
            || duplicate_nonqualification_formalization
            || witness_nonqualification_overlap
        {
            return Err(research_status_error(
                "claim research status is inconsistent, noncanonical, or contains unsupported truth witnesses",
                "Derive status from every current formalization, sort and deduplicate response details, and fail closed on inconsistent evidence.",
            ));
        }
        Ok(())
    }
}

pub fn claim_research_status_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/research/claim-research-status/1",
        "title": "MathOS Claim Research Status v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "claim", "status", "witnesses", "nonqualifications"],
        "properties": {
            "schema_version": {"const": CLAIM_RESEARCH_STATUS_SCHEMA_VERSION},
            "claim": {"$ref": "#/$defs/exact_version_reference"},
            "status": {"enum": ["not_started", "active", "open", "conditionally_resolved", "proved", "disproved", "malformed", "ambiguous", "superseded"]},
            "witnesses": {
                "type": "array",
                "maxItems": MAX_CLAIM_RESEARCH_STATUS_ITEMS,
                "items": {"$ref": "#/$defs/witness"}
            },
            "nonqualifications": {
                "type": "array",
                "maxItems": MAX_CLAIM_RESEARCH_STATUS_ITEMS,
                "items": {"$ref": "#/$defs/nonqualification"}
            }
        },
        "$defs": {
            "exact_version_reference": {
                "type": "object",
                "additionalProperties": false,
                "required": ["object_id", "version_hash"],
                "properties": {
                    "object_id": {"type": "string", "format": "uuid"},
                    "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
                }
            },
            "witness": {
                "type": "object",
                "additionalProperties": false,
                "required": ["formalization", "kind", "reviewed_source_relation", "fidelity_request_schema_version", "fidelity_evidence_id", "fidelity_evidence_hash", "fidelity_report_artifact_hash", "authority_evidence_id", "authority_evidence_hash", "publication_receipt_hash"],
                "properties": {
                    "formalization": {"$ref": "#/$defs/exact_version_reference"},
                    "kind": {"enum": ["proof", "refutation"]},
                    "reviewed_source_relation": {"enum": ["claim", "logical_negation"]},
                    "fidelity_request_schema_version": {"enum": [FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION, FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION]},
                    "fidelity_evidence_id": {"type": "string", "format": "uuid"},
                    "fidelity_evidence_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "fidelity_report_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "authority_evidence_id": {"type": "string", "format": "uuid"},
                    "authority_evidence_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "publication_receipt_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
                }
            },
            "nonqualification": {
                "type": "object",
                "additionalProperties": false,
                "required": ["formalization", "reason", "fidelity_evidence_id", "authority_evidence_id"],
                "properties": {
                    "formalization": {"$ref": "#/$defs/exact_version_reference"},
                    "reason": {"enum": ["source_version_not_current", "no_current_verified_fidelity", "fidelity_relation_unbound", "fidelity_relation_mismatch", "no_current_authoritative_evidence", "authority_kind_mismatch", "source_ambiguity_unresolved", "source_ambiguity_preserved"]},
                    "fidelity_evidence_id": {"type": ["string", "null"], "format": "uuid"},
                    "authority_evidence_id": {"type": ["string", "null"], "format": "uuid"}
                },
                "allOf": [
                    {
                        "if": {"properties": {"reason": {"const": "source_version_not_current"}}},
                        "then": {"properties": {
                            "fidelity_evidence_id": {"type": "null"},
                            "authority_evidence_id": {"type": "null"}
                        }}
                    },
                    {
                        "if": {"properties": {"reason": {"enum": ["fidelity_relation_unbound", "fidelity_relation_mismatch", "no_current_authoritative_evidence", "source_ambiguity_preserved"]}}},
                        "then": {"properties": {
                            "fidelity_evidence_id": {"type": "string", "format": "uuid"},
                            "authority_evidence_id": {"type": "null"}
                        }}
                    },
                    {
                        "if": {"properties": {"reason": {"const": "authority_kind_mismatch"}}},
                        "then": {"properties": {
                            "fidelity_evidence_id": {"type": "string", "format": "uuid"},
                            "authority_evidence_id": {"type": "string", "format": "uuid"}
                        }}
                    },
                    {
                        "if": {"properties": {"reason": {"enum": ["no_current_verified_fidelity", "source_ambiguity_unresolved"]}}},
                        "then": {"properties": {"authority_evidence_id": {"type": "null"}}}
                    }
                ]
            }
        }
    })
}

fn compare_witnesses(
    left: &ClaimResearchStatusWitness,
    right: &ClaimResearchStatusWitness,
) -> Ordering {
    (
        left.formalization.object_id.as_str(),
        left.formalization.version_hash.as_str(),
        left.kind.as_str(),
        left.fidelity_evidence_id.as_str(),
        left.authority_evidence_id.as_str(),
    )
        .cmp(&(
            right.formalization.object_id.as_str(),
            right.formalization.version_hash.as_str(),
            right.kind.as_str(),
            right.fidelity_evidence_id.as_str(),
            right.authority_evidence_id.as_str(),
        ))
}

fn compare_nonqualifications(
    left: &ClaimResearchStatusNonqualification,
    right: &ClaimResearchStatusNonqualification,
) -> Ordering {
    (
        left.formalization.object_id.as_str(),
        left.formalization.version_hash.as_str(),
        left.reason.as_str(),
        left.fidelity_evidence_id.as_deref(),
        left.authority_evidence_id.as_deref(),
    )
        .cmp(&(
            right.formalization.object_id.as_str(),
            right.formalization.version_hash.as_str(),
            right.reason.as_str(),
            right.fidelity_evidence_id.as_deref(),
            right.authority_evidence_id.as_deref(),
        ))
}

fn valid_reference(reference: &ExactVersionReference) -> bool {
    uuid::Uuid::parse_str(&reference.object_id).is_ok() && is_hash(&reference.version_hash)
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn research_status_error(message: impl Into<String>, action: impl Into<String>) -> AppError {
    AppError::new("MCL_CLAIM_RESEARCH_STATUS_INVALID", message, false, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(hash: char) -> ExactVersionReference {
        ExactVersionReference {
            object_id: uuid::Uuid::now_v7().to_string(),
            version_hash: hash.to_string().repeat(64),
        }
    }

    fn witness(kind: ClaimResearchStatusWitnessKind) -> ClaimResearchStatusWitness {
        ClaimResearchStatusWitness {
            formalization: reference('b'),
            kind,
            reviewed_source_relation: kind.reviewed_source_relation(),
            fidelity_request_schema_version: if kind == ClaimResearchStatusWitnessKind::Proof {
                FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION.to_owned()
            } else {
                FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION.to_owned()
            },
            fidelity_evidence_id: uuid::Uuid::now_v7().to_string(),
            fidelity_evidence_hash: "c".repeat(64),
            fidelity_report_artifact_hash: "d".repeat(64),
            authority_evidence_id: uuid::Uuid::now_v7().to_string(),
            authority_evidence_hash: "e".repeat(64),
            publication_receipt_hash: "f".repeat(64),
        }
    }

    fn snapshot(status: ResearchStatus) -> ClaimResearchStatusSnapshot {
        ClaimResearchStatusSnapshot {
            schema_version: CLAIM_RESEARCH_STATUS_SCHEMA_VERSION.to_owned(),
            claim: reference('a'),
            status,
            witnesses: Vec::new(),
            nonqualifications: Vec::new(),
        }
    }

    #[test]
    fn complete_spec_status_vocabulary_has_stable_serialization() {
        let values = ResearchStatus::ALL
            .into_iter()
            .map(|status| serde_json::to_value(status).expect("status"))
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            [
                "not_started",
                "active",
                "open",
                "conditionally_resolved",
                "proved",
                "disproved",
                "malformed",
                "ambiguous",
                "superseded",
            ]
            .map(|value| Value::String(value.to_owned()))
        );
    }

    #[test]
    fn proof_and_refutation_require_exact_polarity_aware_witnesses() {
        let mut proved = snapshot(ResearchStatus::Proved);
        proved
            .witnesses
            .push(witness(ClaimResearchStatusWitnessKind::Proof));
        proved.sort_components();
        proved.validate().expect("v1 claim proof qualifies");

        let mut disproved = snapshot(ResearchStatus::Disproved);
        disproved
            .witnesses
            .push(witness(ClaimResearchStatusWitnessKind::Refutation));
        disproved.sort_components();
        disproved.validate().expect("v2 negation review qualifies");

        disproved.witnesses[0].fidelity_request_schema_version =
            FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION.to_owned();
        assert_eq!(
            disproved
                .validate()
                .expect_err("v1 cannot qualify a refutation")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
    }

    #[test]
    fn contradictory_variants_are_ambiguous_but_same_formalization_is_invalid() {
        let mut ambiguous = snapshot(ResearchStatus::Ambiguous);
        ambiguous
            .witnesses
            .push(witness(ClaimResearchStatusWitnessKind::Proof));
        ambiguous
            .witnesses
            .push(witness(ClaimResearchStatusWitnessKind::Refutation));
        ambiguous.sort_components();
        ambiguous
            .validate()
            .expect("different qualifying variants are ambiguous");

        ambiguous.witnesses[1].formalization = ambiguous.witnesses[0].formalization.clone();
        ambiguous.sort_components();
        assert_eq!(
            ambiguous
                .validate()
                .expect_err("one formalization cannot prove and refute")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
    }

    #[test]
    fn source_ambiguity_blocks_terminal_truth() {
        let formalization = reference('b');
        let nonqualification = ClaimResearchStatusNonqualification {
            formalization,
            reason: ClaimResearchStatusNonqualificationReason::SourceAmbiguityPreserved,
            fidelity_evidence_id: Some(uuid::Uuid::now_v7().to_string()),
            authority_evidence_id: None,
        };
        let mut status = snapshot(ResearchStatus::Ambiguous);
        status.nonqualifications.push(nonqualification.clone());
        status.sort_components();
        status.validate().expect("source ambiguity is explicit");

        status.status = ResearchStatus::Proved;
        status
            .witnesses
            .push(witness(ClaimResearchStatusWitnessKind::Proof));
        status.sort_components();
        assert_eq!(
            status
                .validate()
                .expect_err("ambiguous source cannot be terminally proved")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );

        status.status = ResearchStatus::Open;
        status.witnesses.clear();
        assert_eq!(
            status
                .validate()
                .expect_err("source ambiguity must be reported as ambiguous")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
    }

    #[test]
    fn response_components_must_be_strictly_sorted_and_unique() {
        let item = ClaimResearchStatusNonqualification {
            formalization: reference('b'),
            reason: ClaimResearchStatusNonqualificationReason::NoCurrentVerifiedFidelity,
            fidelity_evidence_id: None,
            authority_evidence_id: None,
        };
        let mut status = snapshot(ResearchStatus::Open);
        status.nonqualifications = vec![item.clone(), item];
        assert_eq!(
            status
                .validate()
                .expect_err("duplicate reason is not canonical")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
    }

    #[test]
    fn each_formalization_has_exactly_one_witness_or_nonqualification() {
        let formalization = reference('b');
        let mut status = snapshot(ResearchStatus::Proved);
        let mut proof = witness(ClaimResearchStatusWitnessKind::Proof);
        proof.formalization = formalization.clone();
        status.witnesses.push(proof);
        status
            .nonqualifications
            .push(ClaimResearchStatusNonqualification {
                formalization: formalization.clone(),
                reason: ClaimResearchStatusNonqualificationReason::NoCurrentVerifiedFidelity,
                fidelity_evidence_id: None,
                authority_evidence_id: None,
            });
        status.sort_components();
        assert_eq!(
            status
                .validate()
                .expect_err("one formalization cannot both qualify and fail")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );

        status.status = ResearchStatus::Open;
        status.witnesses.clear();
        status
            .nonqualifications
            .push(ClaimResearchStatusNonqualification {
                formalization,
                reason: ClaimResearchStatusNonqualificationReason::FidelityRelationUnbound,
                fidelity_evidence_id: Some(uuid::Uuid::now_v7().to_string()),
                authority_evidence_id: None,
            });
        status.sort_components();
        assert_eq!(
            status
                .validate()
                .expect_err("one formalization cannot have competing reasons")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
    }

    #[test]
    fn nonqualification_reason_requires_its_audit_locators() {
        let mut item = ClaimResearchStatusNonqualification {
            formalization: reference('b'),
            reason: ClaimResearchStatusNonqualificationReason::AuthorityKindMismatch,
            fidelity_evidence_id: None,
            authority_evidence_id: None,
        };
        assert_eq!(
            item.validate()
                .expect_err("kind mismatch must name both evidence records")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );

        item.fidelity_evidence_id = Some(uuid::Uuid::now_v7().to_string());
        item.authority_evidence_id = Some(uuid::Uuid::now_v7().to_string());
        item.validate().expect("both mismatch locators validate");

        item.reason = ClaimResearchStatusNonqualificationReason::NoCurrentAuthoritativeEvidence;
        assert_eq!(
            item.validate()
                .expect_err("missing authority cannot name an authority row")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
        item.authority_evidence_id = None;
        item.validate()
            .expect("verified fidelity and absent authority are explicit");

        item.reason = ClaimResearchStatusNonqualificationReason::SourceVersionNotCurrent;
        assert_eq!(
            item.validate()
                .expect_err("a stale source head must not retain a fidelity locator")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
        item.fidelity_evidence_id = None;
        item.validate()
            .expect("source currentness is derived without selecting evidence");
    }

    #[test]
    fn open_status_names_at_least_one_current_nonqualification() {
        let status = snapshot(ResearchStatus::Open);
        assert_eq!(
            status
                .validate()
                .expect_err("open with a formalization requires an explicit reason")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
    }

    #[test]
    fn committed_schema_matches_the_closed_rust_contract() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../schemas/research/claim-research-status-1.schema.json"
        ))
        .expect("claim research status schema");
        assert_eq!(schema, claim_research_status_schema());
    }
}
