use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

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
