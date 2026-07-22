use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

pub mod artifact;
pub mod audit;
pub mod comparator;
pub mod comparator_run;
pub mod corpus;
pub mod counterexample;
pub mod environment;
pub mod evidence;
pub mod fidelity;
pub mod publication;
pub mod release;
pub mod research_status;
pub mod rl;
pub mod schemas;
pub mod verifier;

pub use artifact::{
    ArtifactCreationSource, ArtifactMediaType, ArtifactMetadata, ArtifactRestriction,
    ArtifactSnapshot,
};
pub use audit::{
    LeanAuditClassification, LeanAuditJobSnapshot, LeanAuditPolicy, LeanAuditReport,
    LeanAuditRequest,
};
pub use comparator::{
    COMPARATOR_FORMALIZATION_SCHEMA_VERSION, COMPARATOR_PACKAGE_PLAN_SCHEMA_VERSION,
    COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION, COMPARATOR_REPOSITORY, ComparatorFileBinding,
    ComparatorFormalizationMetadata, ComparatorPackagePlan, ComparatorPackageStatus,
    ComparatorPackageVerification, ComparatorPublicationStatus, ComparatorSourceMemberBinding,
    ComparatorToolPins, LANDRUN_REPOSITORY, LEAN4EXPORT_REPOSITORY, MAX_COMPARATOR_FILE_BYTES,
    MAX_COMPARATOR_SOURCE_BYTES, comparator_package_plan_schema,
    comparator_package_verification_schema,
};
pub use comparator_run::{
    COMPARATOR_RUN_COMMAND_PROFILE, COMPARATOR_RUN_COMPARATOR_COMMIT,
    COMPARATOR_RUN_COMPARATOR_TREE, COMPARATOR_RUN_JOB, COMPARATOR_RUN_LANDRUN_COMMIT,
    COMPARATOR_RUN_LANDRUN_TREE, COMPARATOR_RUN_LEAN_TOOLCHAIN, COMPARATOR_RUN_LEAN4EXPORT_COMMIT,
    COMPARATOR_RUN_LEAN4EXPORT_TREE, COMPARATOR_RUN_PROJECT_NAME,
    COMPARATOR_RUN_REPORT_SCHEMA_VERSION, COMPARATOR_RUN_REPOSITORY, COMPARATOR_RUN_REPOSITORY_ID,
    COMPARATOR_RUN_SOURCE_REF, COMPARATOR_RUN_WORKFLOW_PATH, COMPARATOR_RUN_WORKFLOW_REF,
    ComparatorRunClassification, ComparatorRunExecutionBinding, ComparatorRunFileBinding,
    ComparatorRunHarnessBinding, ComparatorRunPackageBinding, ComparatorRunPredicates,
    ComparatorRunReport, ComparatorRunSandboxBinding, ComparatorRunToolBinding,
    ComparatorRunWorkflowBinding, MAX_ACCEPTED_COMPARATOR_STDERR_BYTES,
    MAX_ACCEPTED_COMPARATOR_STDOUT_BYTES, MAX_COMPARATOR_RUN_BINARY_BYTES,
    MAX_COMPARATOR_RUN_TEXT_BYTES,
};
pub use corpus::{
    CORPUS_EXPORT_MANIFEST_SCHEMA_VERSION, CorpusExportCuration, CorpusExportManifest,
    CorpusExportMember, CorpusExportMemberKind, CorpusExportOutputBinding, CorpusExportPolicy,
    CorpusExportSourceBinding, CorpusExportUpstreamBinding, MathCorpusDifficultyBin,
    MathCorpusDomain, MathCorpusLevel, corpus_export_manifest_schema,
};
pub use counterexample::{
    CLAIM_REPAIR_EDGE_SCHEMA_VERSION, COUNTEREXAMPLE_PACKAGE_SCHEMA_VERSION,
    COUNTEREXAMPLE_REPAIR_REQUEST_SCHEMA_VERSION, COUNTEREXAMPLE_SEARCH_RESULT_SCHEMA_VERSION,
    ClaimRepairEdgePayload, ClaimRepairOperation, CounterexampleCheckerBinding,
    CounterexampleMinimization, CounterexamplePackage, CounterexampleRepairRequest,
    CounterexampleRepairSnapshot, CounterexampleSearchProvenance, CounterexampleSearchResult,
    CounterexampleSearchResultKind, CounterexampleWitness, ProposedRepairedClaim,
};
pub use environment::{
    DependencyRevision, EnvironmentManifest, EnvironmentPlatform, EnvironmentSnapshot,
    ResourceLimits, TrustProfile, VerifierArgument, VerifierCommandTemplate, VerifierExecutable,
    WorkingDirectoryPolicy,
};
pub use evidence::{
    EvidenceAuthorityClass, EvidenceKind, EvidencePayload, EvidenceResult, EvidenceSnapshot,
    PublicationAuthorityBinding,
};
pub use fidelity::{
    AmbiguityDisposition, DefinitionMapping, FidelityReviewHistoryEntry, FidelityReviewLevel,
    FidelityReviewReport, FidelityReviewReportV2, FidelityReviewRequest, FidelityReviewRequestV2,
    FidelityStatus, FidelityStatusSnapshot, FidelityVerdict, ReviewedSourceRelation,
    VersionedFidelityReviewReport, VersionedFidelityReviewRequest,
};
pub use publication::{
    PublicationAttestationVerification, PublicationClassification,
    PublicationIngestionReceiptSnapshot, PublicationOutcome, PublicationPolicy, PublicationReport,
    PublicationRequest, PublicationRetainedArtifactRole, PublicationRetainedClosure,
    PublicationRetainedClosureEntry, PublicationRunnerEnvironment, PublicationStage,
    PublicationStageArtifact, PublicationStageSnapshot,
};
pub use release::{
    RELEASE_MANIFEST_SCHEMA_VERSION, ReleaseManifest, ReleaseMember, ReleaseMemberKind,
    ReleasePedagogyBinding, ReleasePedagogyMode, ReleaseProfile, ReleasePublicationBinding,
    ReleaseReplayBinding,
};
pub use research_status::{
    ClaimResearchStatusNonqualification, ClaimResearchStatusNonqualificationReason,
    ClaimResearchStatusSnapshot, ClaimResearchStatusWitness, ClaimResearchStatusWitnessKind,
    ResearchStatus,
};
pub use rl::{
    MAX_RL_MEMBER_BYTES, MAX_RL_MEMBERS, MAX_RL_RELEASES, MAX_RL_TOTAL_BYTES,
    RL_EXPORT_MANIFEST_SCHEMA_VERSION, RL_EXPORT_PLAN_SCHEMA_VERSION,
    RL_LEAKAGE_REPORT_SCHEMA_VERSION, RL_TASK_SCHEMA_VERSION, RlExportManifest, RlExportMember,
    RlExportMemberKind, RlExportPlan, RlExportSourceBinding, RlLeakageComponent, RlLeakageLabels,
    RlLeakageReport, RlPlanRelease, RlSplit, RlTask, RlTaskEvidenceReference, RlTaskFamily,
    RlTaskFamilySummary, RlTaskPolicy, RlTaskTrust,
};
pub use verifier::{
    VerifierExecutionClassification, VerifierExecutionReport, VerifierJobRequest,
    VerifierJobSnapshot, VerifierJobState,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Source,
    Concept,
    Claim,
    Formalization,
    LearningUnit,
}

impl RecordKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Concept => "concept",
            Self::Claim => "claim",
            Self::Formalization => "formalization",
            Self::LearningUnit => "learning_unit",
        }
    }
}

impl Display for RecordKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RecordKind {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "source" => Ok(Self::Source),
            "concept" => Ok(Self::Concept),
            "claim" => Ok(Self::Claim),
            "formalization" => Ok(Self::Formalization),
            "learning_unit" => Ok(Self::LearningUnit),
            _ => Err(AppError::new(
                "MCL_RECORD_KIND_UNKNOWN",
                format!("unknown canonical record kind `{value}`"),
                false,
                "Use a record kind declared by the committed domain model.",
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordDraft {
    pub kind: RecordKind,
    pub schema_version: String,
    pub payload: Value,
    pub searchable_text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecordSnapshot {
    pub object_id: String,
    pub kind: RecordKind,
    pub version_hash: String,
    pub schema_version: String,
    pub payload: Value,
    pub predecessor_hash: Option<String>,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EdgeKind {
    #[serde(rename = "logic.uses_definition")]
    LogicUsesDefinition,
    #[serde(rename = "logic.depends_on")]
    LogicDependsOn,
    #[serde(rename = "logic.implies")]
    LogicImplies,
    #[serde(rename = "logic.equivalent_to")]
    LogicEquivalentTo,
    #[serde(rename = "logic.contradicts")]
    LogicContradicts,
    #[serde(rename = "logic.generalizes")]
    LogicGeneralizes,
    #[serde(rename = "logic.specializes")]
    LogicSpecializes,
    #[serde(rename = "logic.formalizes")]
    LogicFormalizes,
    #[serde(rename = "pedagogy.hard_prerequisite")]
    PedagogyHardPrerequisite,
    #[serde(rename = "pedagogy.soft_prerequisite")]
    PedagogySoftPrerequisite,
    #[serde(rename = "pedagogy.motivates")]
    PedagogyMotivates,
    #[serde(rename = "pedagogy.example_of")]
    PedagogyExampleOf,
    #[serde(rename = "pedagogy.counterexample_to")]
    PedagogyCounterexampleTo,
    #[serde(rename = "pedagogy.misconception_for")]
    PedagogyMisconceptionFor,
    #[serde(rename = "pedagogy.recommended_next")]
    PedagogyRecommendedNext,
    #[serde(rename = "research.uses_technique")]
    ResearchUsesTechnique,
    #[serde(rename = "research.blocks_method")]
    ResearchBlocksMethod,
    #[serde(rename = "research.repairs")]
    ResearchRepairs,
    #[serde(rename = "research.reduces_to")]
    ResearchReducesTo,
    #[serde(rename = "research.open_obligation_of")]
    ResearchOpenObligationOf,
    #[serde(rename = "provenance.derived_from")]
    ProvenanceDerivedFrom,
    #[serde(rename = "provenance.cites")]
    ProvenanceCites,
    #[serde(rename = "provenance.independently_reproduces")]
    ProvenanceIndependentlyReproduces,
    #[serde(rename = "provenance.supersedes")]
    ProvenanceSupersedes,
    #[serde(rename = "provenance.upstreamed_to")]
    ProvenanceUpstreamedTo,
    #[serde(rename = "implementation.declared_in")]
    ImplementationDeclaredIn,
    #[serde(rename = "implementation.imports")]
    ImplementationImports,
    #[serde(rename = "implementation.generated_from")]
    ImplementationGeneratedFrom,
    #[serde(rename = "implementation.verified_by")]
    ImplementationVerifiedBy,
    #[serde(rename = "implementation.replayed_by")]
    ImplementationReplayedBy,
}

impl EdgeKind {
    pub const ALL: [Self; 30] = [
        Self::LogicUsesDefinition,
        Self::LogicDependsOn,
        Self::LogicImplies,
        Self::LogicEquivalentTo,
        Self::LogicContradicts,
        Self::LogicGeneralizes,
        Self::LogicSpecializes,
        Self::LogicFormalizes,
        Self::PedagogyHardPrerequisite,
        Self::PedagogySoftPrerequisite,
        Self::PedagogyMotivates,
        Self::PedagogyExampleOf,
        Self::PedagogyCounterexampleTo,
        Self::PedagogyMisconceptionFor,
        Self::PedagogyRecommendedNext,
        Self::ResearchUsesTechnique,
        Self::ResearchBlocksMethod,
        Self::ResearchRepairs,
        Self::ResearchReducesTo,
        Self::ResearchOpenObligationOf,
        Self::ProvenanceDerivedFrom,
        Self::ProvenanceCites,
        Self::ProvenanceIndependentlyReproduces,
        Self::ProvenanceSupersedes,
        Self::ProvenanceUpstreamedTo,
        Self::ImplementationDeclaredIn,
        Self::ImplementationImports,
        Self::ImplementationGeneratedFrom,
        Self::ImplementationVerifiedBy,
        Self::ImplementationReplayedBy,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LogicUsesDefinition => "logic.uses_definition",
            Self::LogicDependsOn => "logic.depends_on",
            Self::LogicImplies => "logic.implies",
            Self::LogicEquivalentTo => "logic.equivalent_to",
            Self::LogicContradicts => "logic.contradicts",
            Self::LogicGeneralizes => "logic.generalizes",
            Self::LogicSpecializes => "logic.specializes",
            Self::LogicFormalizes => "logic.formalizes",
            Self::PedagogyHardPrerequisite => "pedagogy.hard_prerequisite",
            Self::PedagogySoftPrerequisite => "pedagogy.soft_prerequisite",
            Self::PedagogyMotivates => "pedagogy.motivates",
            Self::PedagogyExampleOf => "pedagogy.example_of",
            Self::PedagogyCounterexampleTo => "pedagogy.counterexample_to",
            Self::PedagogyMisconceptionFor => "pedagogy.misconception_for",
            Self::PedagogyRecommendedNext => "pedagogy.recommended_next",
            Self::ResearchUsesTechnique => "research.uses_technique",
            Self::ResearchBlocksMethod => "research.blocks_method",
            Self::ResearchRepairs => "research.repairs",
            Self::ResearchReducesTo => "research.reduces_to",
            Self::ResearchOpenObligationOf => "research.open_obligation_of",
            Self::ProvenanceDerivedFrom => "provenance.derived_from",
            Self::ProvenanceCites => "provenance.cites",
            Self::ProvenanceIndependentlyReproduces => "provenance.independently_reproduces",
            Self::ProvenanceSupersedes => "provenance.supersedes",
            Self::ProvenanceUpstreamedTo => "provenance.upstreamed_to",
            Self::ImplementationDeclaredIn => "implementation.declared_in",
            Self::ImplementationImports => "implementation.imports",
            Self::ImplementationGeneratedFrom => "implementation.generated_from",
            Self::ImplementationVerifiedBy => "implementation.verified_by",
            Self::ImplementationReplayedBy => "implementation.replayed_by",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_edge_kind_round_trips_through_its_stable_name() {
        for kind in EdgeKind::ALL {
            assert_eq!(EdgeKind::from_str(kind.as_str()).expect("known kind"), kind);
            assert_eq!(
                serde_json::to_value(kind).expect("serialize kind"),
                Value::String(kind.as_str().to_owned())
            );
        }
    }

    #[test]
    fn every_run_kind_round_trips_through_its_stable_name() {
        for kind in RunKind::ALL {
            assert_eq!(RunKind::from_str(kind.as_str()).expect("known kind"), kind);
            assert_eq!(
                serde_json::to_value(kind).expect("serialize kind"),
                Value::String(kind.as_str().to_owned())
            );
        }
    }

    #[test]
    fn every_run_event_kind_round_trips_through_its_stable_name() {
        for kind in RunEventKind::ALL {
            assert_eq!(
                RunEventKind::from_str(kind.as_str()).expect("known kind"),
                kind
            );
            assert_eq!(
                serde_json::to_value(kind).expect("serialize kind"),
                Value::String(kind.as_str().to_owned())
            );
        }
    }
}

impl FromStr for EdgeKind {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "logic.uses_definition" => Ok(Self::LogicUsesDefinition),
            "logic.depends_on" => Ok(Self::LogicDependsOn),
            "logic.implies" => Ok(Self::LogicImplies),
            "logic.equivalent_to" => Ok(Self::LogicEquivalentTo),
            "logic.contradicts" => Ok(Self::LogicContradicts),
            "logic.generalizes" => Ok(Self::LogicGeneralizes),
            "logic.specializes" => Ok(Self::LogicSpecializes),
            "logic.formalizes" => Ok(Self::LogicFormalizes),
            "pedagogy.hard_prerequisite" => Ok(Self::PedagogyHardPrerequisite),
            "pedagogy.soft_prerequisite" => Ok(Self::PedagogySoftPrerequisite),
            "pedagogy.motivates" => Ok(Self::PedagogyMotivates),
            "pedagogy.example_of" => Ok(Self::PedagogyExampleOf),
            "pedagogy.counterexample_to" => Ok(Self::PedagogyCounterexampleTo),
            "pedagogy.misconception_for" => Ok(Self::PedagogyMisconceptionFor),
            "pedagogy.recommended_next" => Ok(Self::PedagogyRecommendedNext),
            "research.uses_technique" => Ok(Self::ResearchUsesTechnique),
            "research.blocks_method" => Ok(Self::ResearchBlocksMethod),
            "research.repairs" => Ok(Self::ResearchRepairs),
            "research.reduces_to" => Ok(Self::ResearchReducesTo),
            "research.open_obligation_of" => Ok(Self::ResearchOpenObligationOf),
            "provenance.derived_from" => Ok(Self::ProvenanceDerivedFrom),
            "provenance.cites" => Ok(Self::ProvenanceCites),
            "provenance.independently_reproduces" => Ok(Self::ProvenanceIndependentlyReproduces),
            "provenance.supersedes" => Ok(Self::ProvenanceSupersedes),
            "provenance.upstreamed_to" => Ok(Self::ProvenanceUpstreamedTo),
            "implementation.declared_in" => Ok(Self::ImplementationDeclaredIn),
            "implementation.imports" => Ok(Self::ImplementationImports),
            "implementation.generated_from" => Ok(Self::ImplementationGeneratedFrom),
            "implementation.verified_by" => Ok(Self::ImplementationVerifiedBy),
            "implementation.replayed_by" => Ok(Self::ImplementationReplayedBy),
            _ => Err(AppError::new(
                "MCL_EDGE_KIND_UNKNOWN",
                format!("unknown edge kind `{value}`"),
                false,
                "Use an edge kind declared by the committed domain model.",
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EdgeDraft {
    pub kind: EdgeKind,
    pub source_object_id: String,
    pub source_version_hash: String,
    pub target_object_id: String,
    pub target_version_hash: String,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EdgeSnapshot {
    pub edge_id: String,
    pub kind: EdgeKind,
    pub source_object_id: String,
    pub source_version_hash: String,
    pub target_object_id: String,
    pub target_version_hash: String,
    pub payload: Value,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

#[derive(Clone, Debug)]
pub struct GraphTraversalRequest {
    pub root_object_id: String,
    pub root_version_hash: String,
    pub direction: TraversalDirection,
    pub edge_kinds: Vec<EdgeKind>,
    pub max_depth: u32,
    pub limit: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GraphTraversalHit {
    pub depth: u32,
    pub edge: EdgeSnapshot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Formalize,
    Prove,
    Disprove,
    CounterexampleSearch,
    LibrarySearch,
    LiteratureReview,
    Generalize,
    Audit,
    PedagogyBuild,
    ReleaseBuild,
    Migration,
}

impl RunKind {
    pub const ALL: [Self; 11] = [
        Self::Formalize,
        Self::Prove,
        Self::Disprove,
        Self::CounterexampleSearch,
        Self::LibrarySearch,
        Self::LiteratureReview,
        Self::Generalize,
        Self::Audit,
        Self::PedagogyBuild,
        Self::ReleaseBuild,
        Self::Migration,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Formalize => "formalize",
            Self::Prove => "prove",
            Self::Disprove => "disprove",
            Self::CounterexampleSearch => "counterexample_search",
            Self::LibrarySearch => "library_search",
            Self::LiteratureReview => "literature_review",
            Self::Generalize => "generalize",
            Self::Audit => "audit",
            Self::PedagogyBuild => "pedagogy_build",
            Self::ReleaseBuild => "release_build",
            Self::Migration => "migration",
        }
    }
}

impl FromStr for RunKind {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        RunKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                AppError::new(
                    "MCL_RUN_KIND_UNKNOWN",
                    format!("unknown run kind `{value}`"),
                    false,
                    "Use a run kind declared by the committed domain model.",
                )
            })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Active,
    Frozen,
    Closed,
    Failed,
}

impl RunState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Frozen => "frozen",
            Self::Closed => "closed",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for RunState {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "frozen" => Ok(Self::Frozen),
            "closed" => Ok(Self::Closed),
            "failed" => Ok(Self::Failed),
            _ => Err(AppError::new(
                "MCL_RUN_STATE_UNKNOWN",
                format!("unknown run state `{value}`"),
                false,
                "Restore a verified backup if stored run state was altered.",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEventKind {
    RunStarted,
    Observation,
    ActionSubmitted,
    OutputObserved,
    Diagnostic,
    EvidenceLinked,
    LeaseChanged,
    RunFrozen,
    RunClosed,
    RunFailed,
}

impl RunEventKind {
    pub const ALL: [Self; 10] = [
        Self::RunStarted,
        Self::Observation,
        Self::ActionSubmitted,
        Self::OutputObserved,
        Self::Diagnostic,
        Self::EvidenceLinked,
        Self::LeaseChanged,
        Self::RunFrozen,
        Self::RunClosed,
        Self::RunFailed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunStarted => "run_started",
            Self::Observation => "observation",
            Self::ActionSubmitted => "action_submitted",
            Self::OutputObserved => "output_observed",
            Self::Diagnostic => "diagnostic",
            Self::EvidenceLinked => "evidence_linked",
            Self::LeaseChanged => "lease_changed",
            Self::RunFrozen => "run_frozen",
            Self::RunClosed => "run_closed",
            Self::RunFailed => "run_failed",
        }
    }
}

impl FromStr for RunEventKind {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        RunEventKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                AppError::new(
                    "MCL_RUN_EVENT_KIND_UNKNOWN",
                    format!("unknown run event kind `{value}`"),
                    false,
                    "Use a run event kind declared by the committed domain model.",
                )
            })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunSnapshot {
    pub run_id: String,
    pub kind: RunKind,
    pub state: RunState,
    pub actor: String,
    pub budget: Value,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub event_count: i64,
    pub event_head_hash: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RunEventDraft {
    pub kind: RunEventKind,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunEventSnapshot {
    pub event_id: String,
    pub run_id: String,
    pub sequence: i64,
    pub kind: RunEventKind,
    pub payload: Value,
    pub previous_event_hash: Option<String>,
    pub event_hash: String,
    pub actor: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunChainReport {
    pub run_id: String,
    pub valid: bool,
    pub event_count: i64,
    pub head_hash: Option<String>,
    pub first_invalid_sequence: Option<i64>,
}
