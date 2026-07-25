use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::schemas::ExactVersionReference;
use crate::error::AppError;

pub const TRUST_TRANSITION_SCHEMA_VERSION: &str = "trust_transition/1";
pub const TRUST_STATUS_SCHEMA_VERSION: &str = "trust_status/1";
pub const PROMOTION_ASSESSMENT_SCHEMA_VERSION: &str = "promotion_assessment/1";
pub const MAX_TRUST_HISTORY: usize = 256;
const MAX_TEXT: usize = 65_536;
const MAX_REASON: usize = 4_096;
const MAX_IDENTITY: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustDimension {
    Kernel,
    Fidelity,
    Definition,
    Reuse,
    Coverage,
}

impl TrustDimension {
    pub const ALL: [Self; 5] = [
        Self::Kernel,
        Self::Fidelity,
        Self::Definition,
        Self::Reuse,
        Self::Coverage,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kernel => "kernel",
            Self::Fidelity => "fidelity",
            Self::Definition => "definition",
            Self::Reuse => "reuse",
            Self::Coverage => "coverage",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewedTrustDimension {
    Definition,
    Reuse,
    Coverage,
}

impl ReviewedTrustDimension {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Reuse => "reuse",
            Self::Coverage => "coverage",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelTrustStatus {
    Unverified,
    LeanChecked,
    KernelVerified,
    Failed,
}

impl KernelTrustStatus {
    pub const ALL: [Self; 4] = [
        Self::Unverified,
        Self::LeanChecked,
        Self::KernelVerified,
        Self::Failed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::LeanChecked => "lean_checked",
            Self::KernelVerified => "kernel_verified",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FidelityTrustStatus {
    Unreviewed,
    SourceMapped,
    ReviewerChecked,
    ExpertApproved,
    Rejected,
}

impl FidelityTrustStatus {
    pub const ALL: [Self; 5] = [
        Self::Unreviewed,
        Self::SourceMapped,
        Self::ReviewerChecked,
        Self::ExpertApproved,
        Self::Rejected,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::SourceMapped => "source_mapped",
            Self::ReviewerChecked => "reviewer_checked",
            Self::ExpertApproved => "expert_approved",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionTrustStatus {
    Ungrounded,
    SourceGrounded,
    ProjectApproved,
    UpstreamAccepted,
}

impl DefinitionTrustStatus {
    pub const ALL: [Self; 4] = [
        Self::Ungrounded,
        Self::SourceGrounded,
        Self::ProjectApproved,
        Self::UpstreamAccepted,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ungrounded => "ungrounded",
            Self::SourceGrounded => "source_grounded",
            Self::ProjectApproved => "project_approved",
            Self::UpstreamAccepted => "upstream_accepted",
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Ungrounded => 0,
            Self::SourceGrounded => 1,
            Self::ProjectApproved => 2,
            Self::UpstreamAccepted => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReuseTrustStatus {
    Experimental,
    CampaignSpecific,
    Candidate,
    ReviewReady,
    Upstreamed,
}

impl ReuseTrustStatus {
    pub const ALL: [Self; 5] = [
        Self::Experimental,
        Self::CampaignSpecific,
        Self::Candidate,
        Self::ReviewReady,
        Self::Upstreamed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Experimental => "experimental",
            Self::CampaignSpecific => "campaign_specific",
            Self::Candidate => "candidate",
            Self::ReviewReady => "review_ready",
            Self::Upstreamed => "upstreamed",
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Experimental => 0,
            Self::CampaignSpecific => 1,
            Self::Candidate => 2,
            Self::ReviewReady => 3,
            Self::Upstreamed => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageTrustStatus {
    Unknown,
    StatementOnly,
    AnalyticalCore,
    SupportingLemma,
    FiniteSpecialization,
    AsymptoticComponent,
    FullTheorem,
}

impl CoverageTrustStatus {
    pub const ALL: [Self; 7] = [
        Self::Unknown,
        Self::StatementOnly,
        Self::AnalyticalCore,
        Self::SupportingLemma,
        Self::FiniteSpecialization,
        Self::AsymptoticComponent,
        Self::FullTheorem,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::StatementOnly => "statement_only",
            Self::AnalyticalCore => "analytical_core",
            Self::SupportingLemma => "supporting_lemma",
            Self::FiniteSpecialization => "finite_specialization",
            Self::AsymptoticComponent => "asymptotic_component",
            Self::FullTheorem => "full_theorem",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewedTrustStatus {
    Ungrounded,
    SourceGrounded,
    ProjectApproved,
    UpstreamAccepted,
    Experimental,
    CampaignSpecific,
    Candidate,
    ReviewReady,
    Upstreamed,
    Unknown,
    StatementOnly,
    AnalyticalCore,
    SupportingLemma,
    FiniteSpecialization,
    AsymptoticComponent,
    FullTheorem,
}

impl ReviewedTrustStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ungrounded => "ungrounded",
            Self::SourceGrounded => "source_grounded",
            Self::ProjectApproved => "project_approved",
            Self::UpstreamAccepted => "upstream_accepted",
            Self::Experimental => "experimental",
            Self::CampaignSpecific => "campaign_specific",
            Self::Candidate => "candidate",
            Self::ReviewReady => "review_ready",
            Self::Upstreamed => "upstreamed",
            Self::Unknown => "unknown",
            Self::StatementOnly => "statement_only",
            Self::AnalyticalCore => "analytical_core",
            Self::SupportingLemma => "supporting_lemma",
            Self::FiniteSpecialization => "finite_specialization",
            Self::AsymptoticComponent => "asymptotic_component",
            Self::FullTheorem => "full_theorem",
        }
    }

    pub const fn dimension(self) -> ReviewedTrustDimension {
        match self {
            Self::Ungrounded
            | Self::SourceGrounded
            | Self::ProjectApproved
            | Self::UpstreamAccepted => ReviewedTrustDimension::Definition,
            Self::Experimental
            | Self::CampaignSpecific
            | Self::Candidate
            | Self::ReviewReady
            | Self::Upstreamed => ReviewedTrustDimension::Reuse,
            Self::Unknown
            | Self::StatementOnly
            | Self::AnalyticalCore
            | Self::SupportingLemma
            | Self::FiniteSpecialization
            | Self::AsymptoticComponent
            | Self::FullTheorem => ReviewedTrustDimension::Coverage,
        }
    }

    pub const fn initial(dimension: ReviewedTrustDimension) -> Self {
        match dimension {
            ReviewedTrustDimension::Definition => Self::Ungrounded,
            ReviewedTrustDimension::Reuse => Self::Experimental,
            ReviewedTrustDimension::Coverage => Self::Unknown,
        }
    }

    pub fn valid_transition(self, next: Self) -> bool {
        if self.dimension() != next.dimension() || self == next {
            return false;
        }
        match (self, next) {
            (Self::Ungrounded, Self::SourceGrounded)
            | (Self::SourceGrounded, Self::ProjectApproved)
            | (Self::ProjectApproved, Self::UpstreamAccepted)
            | (Self::Experimental, Self::CampaignSpecific)
            | (Self::CampaignSpecific, Self::Candidate)
            | (Self::Candidate, Self::ReviewReady)
            | (Self::ReviewReady, Self::Upstreamed) => true,
            (left, right) if left.dimension() == ReviewedTrustDimension::Definition => {
                definition_status(left)
                    .zip(definition_status(right))
                    .is_some_and(|(left, right)| right.rank() < left.rank())
            }
            (left, right) if left.dimension() == ReviewedTrustDimension::Reuse => {
                reuse_status(left)
                    .zip(reuse_status(right))
                    .is_some_and(|(left, right)| right.rank() < left.rank())
            }
            (left, right) if left.dimension() == ReviewedTrustDimension::Coverage => left != right,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustTransitionRequest {
    pub schema_version: String,
    pub formalization: ExactVersionReference,
    pub dimension: ReviewedTrustDimension,
    pub from_status: ReviewedTrustStatus,
    pub to_status: ReviewedTrustStatus,
    pub reviewer_identity: String,
    pub evidence_artifact_hashes: Vec<String>,
    pub reason: String,
    pub predecessor_transition_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustTransitionSnapshot {
    pub transition_id: String,
    pub transition_hash: String,
    pub request: TrustTransitionRequest,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustDecision {
    pub decision_id: String,
    pub decision_hash: String,
    pub from_status: String,
    pub to_status: String,
    pub decided_at: i64,
    pub reviewer_identity: String,
    pub evidence_artifact_hashes: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustAxisSnapshot<T> {
    pub status: T,
    pub head_decision_id: Option<String>,
    pub head_decision_hash: Option<String>,
    pub history: Vec<TrustDecision>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionBlocker {
    pub dimension: TrustDimension,
    pub current_status: String,
    pub accepted_statuses: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionProfile {
    Experimental,
    Publication,
    Upstream,
}

impl PromotionProfile {
    pub const ALL: [Self; 3] = [Self::Experimental, Self::Publication, Self::Upstream];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Experimental => "experimental",
            Self::Publication => "publication",
            Self::Upstream => "upstream",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionEvaluation {
    pub profile: PromotionProfile,
    pub eligible: bool,
    pub blockers: Vec<PromotionBlocker>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustStatusSnapshot {
    pub schema_version: String,
    pub formalization: ExactVersionReference,
    pub kernel: TrustAxisSnapshot<KernelTrustStatus>,
    pub fidelity: TrustAxisSnapshot<FidelityTrustStatus>,
    pub definition: TrustAxisSnapshot<DefinitionTrustStatus>,
    pub reuse: TrustAxisSnapshot<ReuseTrustStatus>,
    pub coverage: TrustAxisSnapshot<CoverageTrustStatus>,
    pub promotion: Vec<PromotionEvaluation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionAssessment {
    pub schema_version: String,
    pub profile: PromotionProfile,
    pub trust: TrustStatusSnapshot,
    pub eligible: bool,
    pub blockers: Vec<PromotionBlocker>,
}

impl TrustTransitionRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != TRUST_TRANSITION_SCHEMA_VERSION
            || !valid_reference(&self.formalization)
            || self.from_status.dimension() != self.dimension
            || self.to_status.dimension() != self.dimension
            || !self.from_status.valid_transition(self.to_status)
            || !bounded_nonempty(&self.reviewer_identity, MAX_IDENTITY)
            || !bounded_nonempty(&self.reason, MAX_REASON)
            || self.evidence_artifact_hashes.is_empty()
            || self.evidence_artifact_hashes.len() > MAX_TRUST_HISTORY
            || self
                .evidence_artifact_hashes
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self
                .evidence_artifact_hashes
                .iter()
                .any(|hash| !is_hash(hash))
            || self
                .predecessor_transition_id
                .as_deref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err(trust_error(
                "MCL_TRUST_TRANSITION_INVALID",
                "trust transition is not a closed, evidence-backed state-machine step",
                "Use one exact formalization, a valid adjacent promotion or explicit demotion, a reviewer, a reason, sorted evidence hashes, and the current predecessor.",
            ));
        }
        Ok(())
    }

    pub fn transition_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            trust_error(
                "MCL_TRUST_TRANSITION_INVALID",
                error.to_string(),
                "Report this deterministic trust-transition serialization defect.",
            )
        })?)
    }
}

impl TrustTransitionSnapshot {
    pub fn validate(&self) -> Result<(), AppError> {
        self.request.validate()?;
        if uuid::Uuid::parse_str(&self.transition_id).is_err()
            || self.transition_hash != self.request.transition_hash()?
            || self.created_at < 0
            || !bounded_nonempty(&self.created_by, MAX_IDENTITY)
        {
            return Err(trust_error(
                "MCL_TRUST_TRANSITION_INTEGRITY_FAILED",
                "stored trust transition identity or attribution is invalid",
                "Quarantine the transition history and restore the exact immutable review record.",
            ));
        }
        Ok(())
    }
}

impl TrustDecision {
    fn validate(&self, allowed: &[&str]) -> Result<(), AppError> {
        if uuid::Uuid::parse_str(&self.decision_id).is_err()
            || !is_hash(&self.decision_hash)
            || !allowed.contains(&self.from_status.as_str())
            || !allowed.contains(&self.to_status.as_str())
            || self.decided_at < 0
            || !bounded_nonempty(&self.reviewer_identity, MAX_IDENTITY)
            || !bounded_nonempty(&self.reason, MAX_REASON)
            || self.evidence_artifact_hashes.len() > MAX_TRUST_HISTORY
            || self
                .evidence_artifact_hashes
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self
                .evidence_artifact_hashes
                .iter()
                .any(|hash| !is_hash(hash))
        {
            return Err(trust_error(
                "MCL_TRUST_STATUS_INVALID",
                "trust decision history contains an invalid identity, state, attribution, reason, or evidence set",
                "Return only the exact bounded immutable decision history for this axis.",
            ));
        }
        Ok(())
    }
}

impl TrustStatusSnapshot {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != TRUST_STATUS_SCHEMA_VERSION
            || !valid_reference(&self.formalization)
        {
            return Err(trust_error(
                "MCL_TRUST_STATUS_INVALID",
                "trust status does not identify one exact supported formalization",
                "Derive trust only for an exact canonical formalization version.",
            ));
        }
        validate_axis(
            &self.kernel,
            KernelTrustStatus::Unverified,
            KernelTrustStatus::as_str,
            KernelTrustStatus::ALL
                .map(KernelTrustStatus::as_str)
                .as_slice(),
            true,
        )?;
        validate_axis(
            &self.fidelity,
            FidelityTrustStatus::Unreviewed,
            FidelityTrustStatus::as_str,
            FidelityTrustStatus::ALL
                .map(FidelityTrustStatus::as_str)
                .as_slice(),
            true,
        )?;
        validate_axis(
            &self.definition,
            DefinitionTrustStatus::Ungrounded,
            DefinitionTrustStatus::as_str,
            DefinitionTrustStatus::ALL
                .map(DefinitionTrustStatus::as_str)
                .as_slice(),
            false,
        )?;
        validate_axis(
            &self.reuse,
            ReuseTrustStatus::Experimental,
            ReuseTrustStatus::as_str,
            ReuseTrustStatus::ALL
                .map(ReuseTrustStatus::as_str)
                .as_slice(),
            false,
        )?;
        validate_axis(
            &self.coverage,
            CoverageTrustStatus::Unknown,
            CoverageTrustStatus::as_str,
            CoverageTrustStatus::ALL
                .map(CoverageTrustStatus::as_str)
                .as_slice(),
            false,
        )?;
        let expected = promotion_evaluations(self);
        if self.promotion != expected {
            return Err(trust_error(
                "MCL_TRUST_STATUS_INVALID",
                "promotion evaluations do not match the five independent trust dimensions",
                "Recompute every promotion profile from the exact trust snapshot.",
            ));
        }
        Ok(())
    }

    pub fn status_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            trust_error(
                "MCL_TRUST_STATUS_INVALID",
                error.to_string(),
                "Report this deterministic trust-status serialization defect.",
            )
        })?)
    }

    pub fn assessment(&self, profile: PromotionProfile) -> Result<PromotionAssessment, AppError> {
        self.validate()?;
        let evaluation = self
            .promotion
            .iter()
            .find(|evaluation| evaluation.profile == profile)
            .expect("all promotion profiles are validated");
        let assessment = PromotionAssessment {
            schema_version: PROMOTION_ASSESSMENT_SCHEMA_VERSION.to_owned(),
            profile,
            trust: self.clone(),
            eligible: evaluation.eligible,
            blockers: evaluation.blockers.clone(),
        };
        assessment.validate()?;
        Ok(assessment)
    }
}

impl PromotionAssessment {
    pub fn validate(&self) -> Result<(), AppError> {
        self.trust.validate()?;
        let expected = self
            .trust
            .promotion
            .iter()
            .find(|evaluation| evaluation.profile == self.profile)
            .expect("trust validation supplies every profile");
        if self.schema_version != PROMOTION_ASSESSMENT_SCHEMA_VERSION
            || self.eligible != expected.eligible
            || self.blockers != expected.blockers
        {
            return Err(trust_error(
                "MCL_PROMOTION_ASSESSMENT_INVALID",
                "promotion assessment differs from its independent trust dimensions and profile policy",
                "Recompute the assessment from the exact validated trust snapshot.",
            ));
        }
        Ok(())
    }

    pub fn assessment_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        value_hash(&serde_json::to_value(self).map_err(|error| {
            trust_error(
                "MCL_PROMOTION_ASSESSMENT_INVALID",
                error.to_string(),
                "Report this deterministic promotion-assessment serialization defect.",
            )
        })?)
    }
}

pub fn promotion_evaluations(snapshot: &TrustStatusSnapshot) -> Vec<PromotionEvaluation> {
    PromotionProfile::ALL
        .into_iter()
        .map(|profile| promotion_evaluation(snapshot, profile))
        .collect()
}

pub fn promotion_evaluation(
    snapshot: &TrustStatusSnapshot,
    profile: PromotionProfile,
) -> PromotionEvaluation {
    let requirements: &[(TrustDimension, &str, &[&str])] = match profile {
        PromotionProfile::Experimental => &[
            (
                TrustDimension::Kernel,
                snapshot.kernel.status.as_str(),
                &["lean_checked", "kernel_verified"],
            ),
            (
                TrustDimension::Fidelity,
                snapshot.fidelity.status.as_str(),
                &[
                    "unreviewed",
                    "source_mapped",
                    "reviewer_checked",
                    "expert_approved",
                ],
            ),
        ],
        PromotionProfile::Publication => &[
            (
                TrustDimension::Kernel,
                snapshot.kernel.status.as_str(),
                &["kernel_verified"],
            ),
            (
                TrustDimension::Fidelity,
                snapshot.fidelity.status.as_str(),
                &["reviewer_checked", "expert_approved"],
            ),
            (
                TrustDimension::Definition,
                snapshot.definition.status.as_str(),
                &["source_grounded", "project_approved", "upstream_accepted"],
            ),
            (
                TrustDimension::Reuse,
                snapshot.reuse.status.as_str(),
                &["candidate", "review_ready", "upstreamed"],
            ),
            (
                TrustDimension::Coverage,
                snapshot.coverage.status.as_str(),
                &[
                    "statement_only",
                    "analytical_core",
                    "supporting_lemma",
                    "finite_specialization",
                    "asymptotic_component",
                    "full_theorem",
                ],
            ),
        ],
        PromotionProfile::Upstream => &[
            (
                TrustDimension::Kernel,
                snapshot.kernel.status.as_str(),
                &["kernel_verified"],
            ),
            (
                TrustDimension::Fidelity,
                snapshot.fidelity.status.as_str(),
                &["expert_approved"],
            ),
            (
                TrustDimension::Definition,
                snapshot.definition.status.as_str(),
                &["project_approved", "upstream_accepted"],
            ),
            (
                TrustDimension::Reuse,
                snapshot.reuse.status.as_str(),
                &["review_ready", "upstreamed"],
            ),
            (
                TrustDimension::Coverage,
                snapshot.coverage.status.as_str(),
                &[
                    "statement_only",
                    "analytical_core",
                    "supporting_lemma",
                    "finite_specialization",
                    "asymptotic_component",
                    "full_theorem",
                ],
            ),
        ],
    };
    let blockers = requirements
        .iter()
        .filter(|(_, current, accepted)| !accepted.contains(current))
        .map(|(dimension, current, accepted)| PromotionBlocker {
            dimension: *dimension,
            current_status: (*current).to_owned(),
            accepted_statuses: accepted.iter().map(|status| (*status).to_owned()).collect(),
        })
        .collect::<Vec<_>>();
    PromotionEvaluation {
        profile,
        eligible: blockers.is_empty(),
        blockers,
    }
}

pub fn trust_transition_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/trust/transition/1",
        "title": "MathOS Reviewed Trust Transition v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "formalization", "dimension", "from_status", "to_status", "reviewer_identity", "evidence_artifact_hashes", "reason", "predecessor_transition_id"],
        "properties": {
            "schema_version": {"const": TRUST_TRANSITION_SCHEMA_VERSION},
            "formalization": exact_reference_schema(),
            "dimension": {"enum": ["definition", "reuse", "coverage"]},
            "from_status": {"$ref": "#/$defs/reviewed_status"},
            "to_status": {"$ref": "#/$defs/reviewed_status"},
            "reviewer_identity": {"type": "string", "minLength": 1, "maxLength": MAX_IDENTITY},
            "evidence_artifact_hashes": {"type": "array", "minItems": 1, "maxItems": MAX_TRUST_HISTORY, "uniqueItems": true, "items": {"$ref": "#/$defs/hash"}},
            "reason": {"type": "string", "minLength": 1, "maxLength": MAX_REASON},
            "predecessor_transition_id": {"type": ["string", "null"], "format": "uuid"}
        },
        "$defs": {
            "hash": hash_schema(),
            "reviewed_status": {"enum": reviewed_status_values()}
        },
        "allOf": reviewed_dimension_conditionals()
    })
}

pub fn trust_status_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/trust/status/1",
        "title": "MathOS Multidimensional Trust Status v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "formalization", "kernel", "fidelity", "definition", "reuse", "coverage", "promotion"],
        "properties": {
            "schema_version": {"const": TRUST_STATUS_SCHEMA_VERSION},
            "formalization": exact_reference_schema(),
            "kernel": {"$ref": "#/$defs/kernel_axis"},
            "fidelity": {"$ref": "#/$defs/fidelity_axis"},
            "definition": {"$ref": "#/$defs/definition_axis"},
            "reuse": {"$ref": "#/$defs/reuse_axis"},
            "coverage": {"$ref": "#/$defs/coverage_axis"},
            "promotion": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"$ref": "#/$defs/evaluation"}}
        },
        "$defs": {
            "hash": hash_schema(),
            "decision": decision_schema(),
            "kernel_axis": axis_schema(&KernelTrustStatus::ALL.map(KernelTrustStatus::as_str)),
            "fidelity_axis": axis_schema(&FidelityTrustStatus::ALL.map(FidelityTrustStatus::as_str)),
            "definition_axis": axis_schema(&DefinitionTrustStatus::ALL.map(DefinitionTrustStatus::as_str)),
            "reuse_axis": axis_schema(&ReuseTrustStatus::ALL.map(ReuseTrustStatus::as_str)),
            "coverage_axis": axis_schema(&CoverageTrustStatus::ALL.map(CoverageTrustStatus::as_str)),
            "blocker": blocker_schema(),
            "evaluation": {
                "type": "object",
                "additionalProperties": false,
                "required": ["profile", "eligible", "blockers"],
                "properties": {
                    "profile": {"enum": ["experimental", "publication", "upstream"]},
                    "eligible": {"type": "boolean"},
                    "blockers": {"type": "array", "maxItems": 5, "items": {"$ref": "#/$defs/blocker"}}
                }
            }
        }
    })
}

pub fn promotion_assessment_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/trust/promotion-assessment/1",
        "title": "MathOS Promotion Assessment v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "profile", "trust", "eligible", "blockers"],
        "properties": {
            "schema_version": {"const": PROMOTION_ASSESSMENT_SCHEMA_VERSION},
            "profile": {"enum": ["experimental", "publication", "upstream"]},
            "trust": {"$ref": "https://mnehmos.ai/mathos/schemas/trust/status/1"},
            "eligible": {"type": "boolean"},
            "blockers": {"type": "array", "maxItems": 5, "items": blocker_schema()}
        }
    })
}

fn validate_axis<T: Copy + Eq>(
    axis: &TrustAxisSnapshot<T>,
    default: T,
    as_str: fn(T) -> &'static str,
    allowed: &[&str],
    allow_same_status_decision: bool,
) -> Result<(), AppError> {
    if axis.history.len() > MAX_TRUST_HISTORY
        || axis.head_decision_id.is_some() != axis.head_decision_hash.is_some()
    {
        return Err(trust_status_error());
    }
    let mut previous_key: Option<(i64, &str)> = None;
    let mut previous_status = as_str(default);
    for decision in &axis.history {
        decision.validate(allowed)?;
        let key = (decision.decided_at, decision.decision_id.as_str());
        if previous_key.is_some_and(|previous| previous >= key)
            || decision.from_status != previous_status
            || (!allow_same_status_decision && decision.from_status == decision.to_status)
        {
            return Err(trust_status_error());
        }
        previous_key = Some(key);
        previous_status = &decision.to_status;
    }
    if axis.history.is_empty() {
        if axis.status != default
            || axis.head_decision_id.is_some()
            || axis.head_decision_hash.is_some()
        {
            return Err(trust_status_error());
        }
        return Ok(());
    }
    let Some((head_id, head_hash)) = axis
        .head_decision_id
        .as_deref()
        .zip(axis.head_decision_hash.as_deref())
    else {
        return Err(trust_status_error());
    };
    let head = axis
        .history
        .iter()
        .find(|decision| decision.decision_id == head_id && decision.decision_hash == head_hash)
        .ok_or_else(trust_status_error)?;
    if head.to_status != as_str(axis.status)
        || axis
            .history
            .last()
            .is_none_or(|decision| decision.decision_id != head.decision_id)
    {
        return Err(trust_status_error());
    }
    Ok(())
}

fn definition_status(status: ReviewedTrustStatus) -> Option<DefinitionTrustStatus> {
    match status {
        ReviewedTrustStatus::Ungrounded => Some(DefinitionTrustStatus::Ungrounded),
        ReviewedTrustStatus::SourceGrounded => Some(DefinitionTrustStatus::SourceGrounded),
        ReviewedTrustStatus::ProjectApproved => Some(DefinitionTrustStatus::ProjectApproved),
        ReviewedTrustStatus::UpstreamAccepted => Some(DefinitionTrustStatus::UpstreamAccepted),
        _ => None,
    }
}

fn reuse_status(status: ReviewedTrustStatus) -> Option<ReuseTrustStatus> {
    match status {
        ReviewedTrustStatus::Experimental => Some(ReuseTrustStatus::Experimental),
        ReviewedTrustStatus::CampaignSpecific => Some(ReuseTrustStatus::CampaignSpecific),
        ReviewedTrustStatus::Candidate => Some(ReuseTrustStatus::Candidate),
        ReviewedTrustStatus::ReviewReady => Some(ReuseTrustStatus::ReviewReady),
        ReviewedTrustStatus::Upstreamed => Some(ReuseTrustStatus::Upstreamed),
        _ => None,
    }
}

fn reviewed_status_values() -> Vec<&'static str> {
    [
        DefinitionTrustStatus::ALL
            .map(DefinitionTrustStatus::as_str)
            .as_slice(),
        ReuseTrustStatus::ALL
            .map(ReuseTrustStatus::as_str)
            .as_slice(),
        CoverageTrustStatus::ALL
            .map(CoverageTrustStatus::as_str)
            .as_slice(),
    ]
    .into_iter()
    .flatten()
    .copied()
    .collect()
}

fn reviewed_dimension_conditionals() -> Vec<Value> {
    vec![
        (
            "definition",
            DefinitionTrustStatus::ALL
                .map(DefinitionTrustStatus::as_str)
                .to_vec(),
        ),
        (
            "reuse",
            ReuseTrustStatus::ALL.map(ReuseTrustStatus::as_str).to_vec(),
        ),
        (
            "coverage",
            CoverageTrustStatus::ALL
                .map(CoverageTrustStatus::as_str)
                .to_vec(),
        ),
    ]
    .into_iter()
    .map(|(dimension, statuses)| {
        json!({
            "if": {"properties": {"dimension": {"const": dimension}}},
            "then": {
                "properties": {
                    "from_status": {"enum": statuses},
                    "to_status": {"enum": statuses}
                }
            }
        })
    })
    .collect()
}

fn axis_schema(statuses: &[&str]) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["status", "head_decision_id", "head_decision_hash", "history"],
        "properties": {
            "status": {"enum": statuses},
            "head_decision_id": {"type": ["string", "null"], "format": "uuid"},
            "head_decision_hash": {"oneOf": [{"$ref": "#/$defs/hash"}, {"type": "null"}]},
            "history": {"type": "array", "maxItems": MAX_TRUST_HISTORY, "items": {"$ref": "#/$defs/decision"}}
        }
    })
}

fn decision_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["decision_id", "decision_hash", "from_status", "to_status", "decided_at", "reviewer_identity", "evidence_artifact_hashes", "reason"],
        "properties": {
            "decision_id": {"type": "string", "format": "uuid"},
            "decision_hash": {"$ref": "#/$defs/hash"},
            "from_status": {"type": "string", "minLength": 1, "maxLength": 64},
            "to_status": {"type": "string", "minLength": 1, "maxLength": 64},
            "decided_at": {"type": "integer", "minimum": 0},
            "reviewer_identity": {"type": "string", "minLength": 1, "maxLength": MAX_IDENTITY},
            "evidence_artifact_hashes": {"type": "array", "maxItems": MAX_TRUST_HISTORY, "uniqueItems": true, "items": {"$ref": "#/$defs/hash"}},
            "reason": {"type": "string", "minLength": 1, "maxLength": MAX_REASON}
        }
    })
}

fn blocker_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["dimension", "current_status", "accepted_statuses"],
        "properties": {
            "dimension": {"enum": ["kernel", "fidelity", "definition", "reuse", "coverage"]},
            "current_status": {"type": "string", "minLength": 1, "maxLength": 64},
            "accepted_statuses": {"type": "array", "minItems": 1, "maxItems": 7, "uniqueItems": true, "items": {"type": "string", "minLength": 1, "maxLength": 64}}
        }
    })
}

fn exact_reference_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["object_id", "version_hash"],
        "properties": {
            "object_id": {"type": "string", "format": "uuid"},
            "version_hash": hash_schema()
        }
    })
}

fn hash_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
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

fn bounded_nonempty(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && value.len() <= MAX_TEXT
}

fn trust_status_error() -> AppError {
    trust_error(
        "MCL_TRUST_STATUS_INVALID",
        "trust axis history is not a complete ordered chain with one exact head",
        "Recompute the five independent axes from their immutable evidence and reviewed transition histories.",
    )
}

fn trust_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> ExactVersionReference {
        ExactVersionReference {
            object_id: "00000000-0000-4000-8000-000000000001".to_owned(),
            version_hash: "a".repeat(64),
        }
    }

    fn decision(
        id_suffix: u8,
        from_status: &str,
        to_status: &str,
        decided_at: i64,
    ) -> TrustDecision {
        TrustDecision {
            decision_id: format!("00000000-0000-4000-8000-{id_suffix:012}"),
            decision_hash: format!("{id_suffix:x}").repeat(64),
            from_status: from_status.to_owned(),
            to_status: to_status.to_owned(),
            decided_at,
            reviewer_identity: "independent-reviewer".to_owned(),
            evidence_artifact_hashes: vec!["f".repeat(64)],
            reason: "Exact evidence-backed review decision.".to_owned(),
        }
    }

    fn axis<T>(status: T, history: Vec<TrustDecision>) -> TrustAxisSnapshot<T> {
        let (head_decision_id, head_decision_hash) =
            history.last().map_or((None, None), |decision| {
                (
                    Some(decision.decision_id.clone()),
                    Some(decision.decision_hash.clone()),
                )
            });
        TrustAxisSnapshot {
            status,
            head_decision_id,
            head_decision_hash,
            history,
        }
    }

    fn adversarial_snapshot() -> TrustStatusSnapshot {
        let mut snapshot = TrustStatusSnapshot {
            schema_version: TRUST_STATUS_SCHEMA_VERSION.to_owned(),
            formalization: reference(),
            kernel: axis(
                KernelTrustStatus::KernelVerified,
                vec![decision(1, "unverified", "kernel_verified", 1)],
            ),
            fidelity: axis(FidelityTrustStatus::Unreviewed, Vec::new()),
            definition: axis(DefinitionTrustStatus::Ungrounded, Vec::new()),
            reuse: axis(ReuseTrustStatus::Experimental, Vec::new()),
            coverage: axis(
                CoverageTrustStatus::FullTheorem,
                vec![decision(2, "unknown", "full_theorem", 2)],
            ),
            promotion: Vec::new(),
        };
        snapshot.promotion = promotion_evaluations(&snapshot);
        snapshot
    }

    #[test]
    fn reviewed_state_machines_allow_adjacent_promotion_and_evidenced_demotion_only() {
        assert!(
            ReviewedTrustStatus::Ungrounded.valid_transition(ReviewedTrustStatus::SourceGrounded)
        );
        assert!(
            !ReviewedTrustStatus::Ungrounded.valid_transition(ReviewedTrustStatus::ProjectApproved)
        );
        assert!(
            ReviewedTrustStatus::UpstreamAccepted.valid_transition(ReviewedTrustStatus::Ungrounded)
        );
        assert!(
            ReviewedTrustStatus::Unknown.valid_transition(ReviewedTrustStatus::SupportingLemma)
        );
        assert!(!ReviewedTrustStatus::Unknown.valid_transition(ReviewedTrustStatus::Unknown));
        assert!(
            !ReviewedTrustStatus::Experimental
                .valid_transition(ReviewedTrustStatus::SourceGrounded)
        );
    }

    #[test]
    fn kernel_correct_full_theorem_trivialization_fails_publication_and_upstream() {
        let snapshot = adversarial_snapshot();
        snapshot.validate().expect("closed trust snapshot");
        let experimental = snapshot
            .promotion
            .iter()
            .find(|evaluation| evaluation.profile == PromotionProfile::Experimental)
            .expect("experimental evaluation");
        assert!(experimental.eligible);
        for profile in [PromotionProfile::Publication, PromotionProfile::Upstream] {
            let evaluation = snapshot
                .promotion
                .iter()
                .find(|evaluation| evaluation.profile == profile)
                .expect("profile evaluation");
            assert!(!evaluation.eligible);
            assert!(
                evaluation
                    .blockers
                    .iter()
                    .any(|blocker| blocker.dimension == TrustDimension::Fidelity)
            );
            assert!(
                evaluation
                    .blockers
                    .iter()
                    .any(|blocker| blocker.dimension == TrustDimension::Definition)
            );
            assert!(
                evaluation
                    .blockers
                    .iter()
                    .any(|blocker| blocker.dimension == TrustDimension::Reuse)
            );
        }
        assert_eq!(snapshot.fidelity.status, FidelityTrustStatus::Unreviewed);
        assert_eq!(
            snapshot.definition.status,
            DefinitionTrustStatus::Ungrounded
        );
        assert_eq!(snapshot.reuse.status, ReuseTrustStatus::Experimental);
    }

    #[test]
    fn committed_schemas_match_closed_rust_contracts() {
        let transition: Value = serde_json::from_str(include_str!(
            "../../schemas/trust/trust-transition-1.schema.json"
        ))
        .expect("committed transition schema");
        let status: Value = serde_json::from_str(include_str!(
            "../../schemas/trust/trust-status-1.schema.json"
        ))
        .expect("committed status schema");
        let assessment: Value = serde_json::from_str(include_str!(
            "../../schemas/trust/promotion-assessment-1.schema.json"
        ))
        .expect("committed promotion schema");
        assert_eq!(transition, trust_transition_schema());
        assert_eq!(status, trust_status_schema());
        assert_eq!(assessment, promotion_assessment_schema());
        assert_eq!(
            value_hash(&transition).expect("transition schema hash"),
            "dd3acdc2a90b9eb8ce08d1e6867ae34b816411a369a1167d798cfe27b26376d7"
        );
        assert_eq!(
            value_hash(&status).expect("status schema hash"),
            "41c92c8367b45b7a86089a7c98713cc78f7f493b9486983a8afba1f3a9688410"
        );
        assert_eq!(
            value_hash(&assessment).expect("assessment schema hash"),
            "e18750973883050690a78388bfee69a57ad52e9609af2752f5435c72c59ae930"
        );
    }
}
