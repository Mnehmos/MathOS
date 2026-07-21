use std::collections::BTreeSet;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::{canonical_json, record_version_hash, value_hash};
use crate::domain::research_status::{ClaimResearchStatusWitness, ClaimResearchStatusWitnessKind};
use crate::domain::schemas::{
    CLAIM_SCHEMA_VERSION, ClaimPayload, ExactVersionReference, validate_record_payload,
};
use crate::domain::{RecordKind, RecordSnapshot};
use crate::error::AppError;

pub const COUNTEREXAMPLE_REPAIR_REQUEST_SCHEMA_VERSION: &str = "counterexample_repair_request/1";
pub const COUNTEREXAMPLE_PACKAGE_SCHEMA_VERSION: &str = "counterexample_package/1";
pub const CLAIM_REPAIR_EDGE_SCHEMA_VERSION: &str = "claim_repair_edge/1";
pub const COUNTEREXAMPLE_SEARCH_RESULT_SCHEMA_VERSION: &str = "counterexample_search_result/1";
const MAX_TEXT_BYTES: usize = 1_048_576;
const MAX_WITNESS_BYTES: usize = 1_048_576;
const MAX_SUPPORTING_ARTIFACTS: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimRepairOperation {
    AddMissingHypothesis,
    ExcludeBoundaryCase,
    WeakenConclusion,
    RestrictDomain,
    ChangePointwiseToAsymptotic,
    SplitCases,
    ReplaceEquality,
}

impl ClaimRepairOperation {
    pub const ALL: [Self; 7] = [
        Self::AddMissingHypothesis,
        Self::ExcludeBoundaryCase,
        Self::WeakenConclusion,
        Self::RestrictDomain,
        Self::ChangePointwiseToAsymptotic,
        Self::SplitCases,
        Self::ReplaceEquality,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AddMissingHypothesis => "add_missing_hypothesis",
            Self::ExcludeBoundaryCase => "exclude_boundary_case",
            Self::WeakenConclusion => "weaken_conclusion",
            Self::RestrictDomain => "restrict_domain",
            Self::ChangePointwiseToAsymptotic => "change_pointwise_to_asymptotic",
            Self::SplitCases => "split_cases",
            Self::ReplaceEquality => "replace_equality",
        }
    }
}

impl FromStr for ClaimRepairOperation {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.as_str() == value)
            .ok_or_else(|| {
                counterexample_error(
                    "MCL_CLAIM_REPAIR_OPERATION_INVALID",
                    format!("unknown claim repair operation `{value}`"),
                    "Use one repair operation declared by counterexample_repair_request/1.",
                )
            })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleWitness {
    pub mathematical_type: String,
    pub canonical_value: Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterexampleSearchResultKind {
    CounterexampleConfirmed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleSearchResult {
    pub schema_version: String,
    pub original_claim: ExactVersionReference,
    pub refutation_formalization: ExactVersionReference,
    pub witness: CounterexampleWitness,
    pub result: CounterexampleSearchResultKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleMinimization {
    pub explanation: String,
    pub supporting_artifact_hashes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleRepairRequest {
    pub schema_version: String,
    pub original_claim: ExactVersionReference,
    pub refutation_formalization: ExactVersionReference,
    pub witness: CounterexampleWitness,
    pub minimization: Option<CounterexampleMinimization>,
    pub failing_assumption_explanation: String,
    pub repair_operation: ClaimRepairOperation,
    pub proposed_repaired_claim: ClaimPayload,
    pub repaired_claim_searchable_text: String,
    pub counterexample_search_run_id: String,
    pub counterexample_search_run_head_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleCheckerBinding {
    pub formal_system: String,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
    pub exact_theorem_type: String,
    pub declaration_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedRepairedClaim {
    pub schema_version: String,
    pub version_hash: String,
    pub payload: ClaimPayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleSearchProvenance {
    pub run_id: String,
    pub event_head_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexamplePackage {
    pub schema_version: String,
    pub source: ExactVersionReference,
    pub original_claim: ExactVersionReference,
    pub witness: CounterexampleWitness,
    pub checker: CounterexampleCheckerBinding,
    pub refutation_witness: ClaimResearchStatusWitness,
    pub minimization: Option<CounterexampleMinimization>,
    pub failing_assumption_explanation: String,
    pub repair_operation: ClaimRepairOperation,
    pub proposed_repaired_claim: ProposedRepairedClaim,
    pub search_provenance: CounterexampleSearchProvenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRepairEdgePayload {
    pub schema_version: String,
    pub counterexample_package_artifact_hash: String,
    pub repair_operation: ClaimRepairOperation,
    pub refutation_formalization: ExactVersionReference,
    pub counterexample_search_run_id: String,
    pub counterexample_search_run_head_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleRepairSnapshot {
    pub package_artifact: crate::domain::ArtifactSnapshot,
    pub repaired_claim: RecordSnapshot,
    pub repair_edge: crate::domain::EdgeSnapshot,
}

impl CounterexampleRepairRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != COUNTEREXAMPLE_REPAIR_REQUEST_SCHEMA_VERSION {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_SCHEMA_UNSUPPORTED",
                format!(
                    "counterexample repair request schema must be `{COUNTEREXAMPLE_REPAIR_REQUEST_SCHEMA_VERSION}`, received `{}`",
                    self.schema_version
                ),
                "Use the committed counterexample repair request schema.",
            ));
        }
        validate_reference(&self.original_claim, "original claim")?;
        validate_reference(&self.refutation_formalization, "refutation formalization")?;
        self.witness.validate()?;
        if let Some(minimization) = &self.minimization {
            minimization.validate()?;
        }
        validate_text(
            "failing assumption explanation",
            &self.failing_assumption_explanation,
        )?;
        validate_record_payload(
            RecordKind::Claim,
            CLAIM_SCHEMA_VERSION,
            &serde_json::to_value(&self.proposed_repaired_claim)
                .map_err(counterexample_serialization_error)?,
        )?;
        validate_text(
            "repaired claim searchable text",
            &self.repaired_claim_searchable_text,
        )?;
        validate_uuid(
            &self.counterexample_search_run_id,
            "counterexample search run ID",
        )?;
        validate_hash(
            &self.counterexample_search_run_head_hash,
            "counterexample search run head",
        )?;
        Ok(())
    }
}

impl CounterexampleWitness {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_text(
            "counterexample witness mathematical type",
            &self.mathematical_type,
        )?;
        if self.canonical_value.is_null() {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_WITNESS_INVALID",
                "counterexample witness canonical value may not be null",
                "Provide one exact canonical JSON witness value.",
            ));
        }
        let bytes = canonical_json(&self.canonical_value)?;
        if bytes.len() > MAX_WITNESS_BYTES {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_WITNESS_TOO_LARGE",
                format!(
                    "counterexample witness exceeds the {MAX_WITNESS_BYTES} byte canonical limit"
                ),
                "Store large generated data separately and use a bounded exact witness value.",
            ));
        }
        Ok(())
    }
}

impl CounterexampleSearchResult {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != COUNTEREXAMPLE_SEARCH_RESULT_SCHEMA_VERSION {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_SEARCH_RESULT_INVALID",
                format!(
                    "counterexample search result schema must be `{COUNTEREXAMPLE_SEARCH_RESULT_SCHEMA_VERSION}`, received `{}`",
                    self.schema_version
                ),
                "Submit one closed counterexample_search_result/1 observation.",
            ));
        }
        validate_reference(&self.original_claim, "counterexample search result claim")?;
        validate_reference(
            &self.refutation_formalization,
            "counterexample search result formalization",
        )?;
        self.witness.validate()?;
        Ok(())
    }
}

impl CounterexampleMinimization {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_text("counterexample minimization explanation", &self.explanation)?;
        validate_sorted_hashes(
            &self.supporting_artifact_hashes,
            "counterexample minimization supporting artifacts",
        )
    }
}

impl CounterexamplePackage {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != COUNTEREXAMPLE_PACKAGE_SCHEMA_VERSION {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_SCHEMA_UNSUPPORTED",
                format!(
                    "counterexample package schema must be `{COUNTEREXAMPLE_PACKAGE_SCHEMA_VERSION}`, received `{}`",
                    self.schema_version
                ),
                "Use the committed counterexample package schema.",
            ));
        }
        validate_reference(&self.source, "counterexample source")?;
        validate_reference(&self.original_claim, "counterexample original claim")?;
        self.witness.validate()?;
        self.checker.validate()?;
        self.refutation_witness.validate()?;
        if self.refutation_witness.kind != ClaimResearchStatusWitnessKind::Refutation {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_REFUTATION_REQUIRED",
                "counterexample package witness is not a refutation",
                "Build a package only from a current derived refutation witness.",
            ));
        }
        if let Some(minimization) = &self.minimization {
            minimization.validate()?;
        }
        validate_text(
            "counterexample failing assumption explanation",
            &self.failing_assumption_explanation,
        )?;
        self.proposed_repaired_claim.validate()?;
        if self.proposed_repaired_claim.payload.source_reference != self.source {
            return Err(counterexample_error(
                "MCL_CLAIM_REPAIR_SOURCE_MISMATCH",
                "proposed repaired claim does not retain the exact source version",
                "Create the repaired claim from the same exact source and link it to the original claim.",
            ));
        }
        if self.proposed_repaired_claim.version_hash == self.original_claim.version_hash {
            return Err(counterexample_error(
                "MCL_CLAIM_REPAIR_UNCHANGED",
                "proposed repaired claim is canonically identical to the original claim",
                "Change the mathematical statement or assumptions and create a new claim object.",
            ));
        }
        self.search_provenance.validate()?;
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, AppError> {
        self.validate()?;
        canonical_json(&serde_json::to_value(self).map_err(counterexample_serialization_error)?)
    }

    pub fn package_hash(&self) -> Result<String, AppError> {
        value_hash(&serde_json::to_value(self).map_err(counterexample_serialization_error)?)
    }
}

impl CounterexampleCheckerBinding {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.formal_system != "lean4" {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_CHECKER_INVALID",
                "counterexample checker must be the derived Lean 4 refutation declaration",
                "Use the checker binding derived by the application from the selected formalization.",
            ));
        }
        validate_hash(&self.environment_hash, "counterexample checker environment")?;
        validate_hash(&self.module_artifact_hash, "counterexample checker module")?;
        validate_text(
            "counterexample checker declaration name",
            &self.declaration_name,
        )?;
        validate_text(
            "counterexample checker theorem type",
            &self.exact_theorem_type,
        )?;
        validate_hash(&self.declaration_hash, "counterexample checker declaration")
    }
}

impl ProposedRepairedClaim {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != CLAIM_SCHEMA_VERSION {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_SCHEMA_UNSUPPORTED",
                format!(
                    "proposed repaired claim schema must be `{CLAIM_SCHEMA_VERSION}`, received `{}`",
                    self.schema_version
                ),
                "Use the committed claim/1 schema.",
            ));
        }
        let payload =
            serde_json::to_value(&self.payload).map_err(counterexample_serialization_error)?;
        validate_record_payload(RecordKind::Claim, CLAIM_SCHEMA_VERSION, &payload)?;
        let expected = record_version_hash(CLAIM_SCHEMA_VERSION, &payload)?;
        if self.version_hash != expected {
            return Err(counterexample_error(
                "MCL_CLAIM_REPAIR_HASH_MISMATCH",
                "proposed repaired claim version hash does not match its canonical payload",
                "Use the application-derived repaired claim version hash.",
            ));
        }
        Ok(())
    }
}

impl CounterexampleSearchProvenance {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_uuid(&self.run_id, "counterexample search run ID")?;
        validate_hash(&self.event_head_hash, "counterexample search run head")
    }
}

impl ClaimRepairEdgePayload {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != CLAIM_REPAIR_EDGE_SCHEMA_VERSION {
            return Err(counterexample_error(
                "MCL_COUNTEREXAMPLE_SCHEMA_UNSUPPORTED",
                format!(
                    "claim repair edge schema must be `{CLAIM_REPAIR_EDGE_SCHEMA_VERSION}`, received `{}`",
                    self.schema_version
                ),
                "Use the application-derived claim repair edge payload.",
            ));
        }
        validate_hash(
            &self.counterexample_package_artifact_hash,
            "counterexample package artifact",
        )?;
        validate_reference(
            &self.refutation_formalization,
            "repair refutation formalization",
        )?;
        validate_uuid(
            &self.counterexample_search_run_id,
            "counterexample search run ID",
        )?;
        validate_hash(
            &self.counterexample_search_run_head_hash,
            "counterexample search run head",
        )
    }
}

pub fn counterexample_repair_request_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/counterexample/repair-request/1",
        "title": "MathOS Counterexample Repair Request v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "original_claim", "refutation_formalization", "witness", "minimization", "failing_assumption_explanation", "repair_operation", "proposed_repaired_claim", "repaired_claim_searchable_text", "counterexample_search_run_id", "counterexample_search_run_head_hash"],
        "properties": {
            "schema_version": {"const": COUNTEREXAMPLE_REPAIR_REQUEST_SCHEMA_VERSION},
            "original_claim": exact_reference_schema(),
            "refutation_formalization": exact_reference_schema(),
            "witness": witness_schema(),
            "minimization": {"oneOf": [minimization_schema(), {"type": "null"}]},
            "failing_assumption_explanation": text_schema(),
            "repair_operation": repair_operation_schema(),
            "proposed_repaired_claim": {"$ref": "https://mnehmos.ai/mathos/schemas/claim/1"},
            "repaired_claim_searchable_text": text_schema(),
            "counterexample_search_run_id": {"type": "string", "format": "uuid"},
            "counterexample_search_run_head_hash": hash_schema()
        }
    })
}

pub fn counterexample_package_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/counterexample/package/1",
        "title": "MathOS Counterexample Package v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "source", "original_claim", "witness", "checker", "refutation_witness", "minimization", "failing_assumption_explanation", "repair_operation", "proposed_repaired_claim", "search_provenance"],
        "properties": {
            "schema_version": {"const": COUNTEREXAMPLE_PACKAGE_SCHEMA_VERSION},
            "source": exact_reference_schema(),
            "original_claim": exact_reference_schema(),
            "witness": witness_schema(),
            "checker": {
                "type": "object",
                "additionalProperties": false,
                "required": ["formal_system", "environment_hash", "module_artifact_hash", "declaration_name", "exact_theorem_type", "declaration_hash"],
                "properties": {
                    "formal_system": {"const": "lean4"},
                    "environment_hash": hash_schema(),
                    "module_artifact_hash": hash_schema(),
                    "declaration_name": text_schema(),
                    "exact_theorem_type": text_schema(),
                    "declaration_hash": hash_schema()
                }
            },
            "refutation_witness": refutation_witness_schema(),
            "minimization": {"oneOf": [minimization_schema(), {"type": "null"}]},
            "failing_assumption_explanation": text_schema(),
            "repair_operation": repair_operation_schema(),
            "proposed_repaired_claim": {
                "type": "object",
                "additionalProperties": false,
                "required": ["schema_version", "version_hash", "payload"],
                "properties": {
                    "schema_version": {"const": CLAIM_SCHEMA_VERSION},
                    "version_hash": hash_schema(),
                    "payload": {"$ref": "https://mnehmos.ai/mathos/schemas/claim/1"}
                }
            },
            "search_provenance": {
                "type": "object",
                "additionalProperties": false,
                "required": ["run_id", "event_head_hash"],
                "properties": {
                    "run_id": {"type": "string", "format": "uuid"},
                    "event_head_hash": hash_schema()
                }
            }
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

fn witness_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["mathematical_type", "canonical_value"],
        "properties": {
            "mathematical_type": text_schema(),
            "canonical_value": {"not": {"type": "null"}}
        }
    })
}

fn minimization_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["explanation", "supporting_artifact_hashes"],
        "properties": {
            "explanation": text_schema(),
            "supporting_artifact_hashes": {"type": "array", "maxItems": MAX_SUPPORTING_ARTIFACTS, "items": hash_schema()}
        }
    })
}

fn refutation_witness_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["formalization", "kind", "reviewed_source_relation", "fidelity_request_schema_version", "fidelity_evidence_id", "fidelity_evidence_hash", "fidelity_report_artifact_hash", "authority_evidence_id", "authority_evidence_hash", "publication_receipt_hash"],
        "properties": {
            "formalization": exact_reference_schema(),
            "kind": {"const": "refutation"},
            "reviewed_source_relation": {"const": "logical_negation"},
            "fidelity_request_schema_version": {"const": "fidelity_review_request/2"},
            "fidelity_evidence_id": {"type": "string", "format": "uuid"},
            "fidelity_evidence_hash": hash_schema(),
            "fidelity_report_artifact_hash": hash_schema(),
            "authority_evidence_id": {"type": "string", "format": "uuid"},
            "authority_evidence_hash": hash_schema(),
            "publication_receipt_hash": hash_schema()
        }
    })
}

fn repair_operation_schema() -> Value {
    json!({"enum": ClaimRepairOperation::ALL.map(ClaimRepairOperation::as_str)})
}

fn text_schema() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES})
}

fn hash_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

fn validate_reference(reference: &ExactVersionReference, label: &str) -> Result<(), AppError> {
    validate_uuid(&reference.object_id, &format!("{label} object ID"))?;
    validate_hash(&reference.version_hash, &format!("{label} version hash"))
}

fn validate_uuid(value: &str, label: &str) -> Result<(), AppError> {
    if uuid::Uuid::parse_str(value).is_err() {
        return Err(counterexample_error(
            "MCL_COUNTEREXAMPLE_REFERENCE_INVALID",
            format!("{label} must be one canonical UUID"),
            "Use exact identities returned by MathOS.",
        ));
    }
    Ok(())
}

fn validate_hash(value: &str, label: &str) -> Result<(), AppError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(counterexample_error(
            "MCL_COUNTEREXAMPLE_HASH_INVALID",
            format!("{label} must be one lowercase SHA-256 hash"),
            "Use exact canonical hashes returned by MathOS.",
        ));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_BYTES {
        return Err(counterexample_error(
            "MCL_COUNTEREXAMPLE_TEXT_INVALID",
            format!("{label} must be nonempty and at most {MAX_TEXT_BYTES} bytes"),
            "Provide bounded explicit counterexample and repair text.",
        ));
    }
    Ok(())
}

fn validate_sorted_hashes(values: &[String], label: &str) -> Result<(), AppError> {
    if values.len() > MAX_SUPPORTING_ARTIFACTS {
        return Err(counterexample_error(
            "MCL_COUNTEREXAMPLE_COLLECTION_TOO_LARGE",
            format!("{label} exceeds {MAX_SUPPORTING_ARTIFACTS} entries"),
            "Use a bounded exact supporting artifact set.",
        ));
    }
    for value in values {
        validate_hash(value, label)?;
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() || !values.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(counterexample_error(
            "MCL_COUNTEREXAMPLE_ARTIFACT_ORDER_INVALID",
            format!("{label} must be sorted and unique"),
            "Sort and deduplicate the supporting artifact hashes.",
        ));
    }
    Ok(())
}

fn counterexample_serialization_error(error: serde_json::Error) -> AppError {
    counterexample_error(
        "MCL_COUNTEREXAMPLE_SERIALIZATION_FAILED",
        format!("counterexample contract could not be serialized: {error}"),
        "Report this closed-contract serialization defect.",
    )
}

fn counterexample_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ReviewedSourceRelation, schemas::VariableDomainNote};

    fn reference(seed: u128, hash: char) -> ExactVersionReference {
        ExactVersionReference {
            object_id: uuid::Uuid::from_u128(seed).to_string(),
            version_hash: hash.to_string().repeat(64),
        }
    }

    fn repaired_claim(source: ExactVersionReference) -> ClaimPayload {
        ClaimPayload {
            source_reference: source,
            normalized_informal_statement: "Every prime number other than 2 is odd.".to_owned(),
            claim_kind: crate::domain::schemas::ClaimKind::Universal,
            logical_shape: Some("∀ n : Nat, Prime n -> n ≠ 2 -> Odd n".to_owned()),
            assumptions: vec!["n ≠ 2".to_owned()],
            variables: vec![VariableDomainNote {
                symbol: "n".to_owned(),
                domain: "natural numbers".to_owned(),
                notes: "prime candidate".to_owned(),
            }],
            concept_links: Vec::new(),
            source_citations: Vec::new(),
            ambiguity_notes: Vec::new(),
        }
    }

    fn package() -> CounterexamplePackage {
        let source = reference(1, 'a');
        let original_claim = reference(2, 'b');
        let formalization = reference(3, 'c');
        let payload = repaired_claim(source.clone());
        let payload_value = serde_json::to_value(&payload).expect("claim payload");
        CounterexamplePackage {
            schema_version: COUNTEREXAMPLE_PACKAGE_SCHEMA_VERSION.to_owned(),
            source,
            original_claim,
            witness: CounterexampleWitness {
                mathematical_type: "natural number".to_owned(),
                canonical_value: json!(2),
            },
            checker: CounterexampleCheckerBinding {
                formal_system: "lean4".to_owned(),
                environment_hash: "d".repeat(64),
                module_artifact_hash: "e".repeat(64),
                declaration_name: "MathOS.PilotA.every_prime_is_odd_refuted".to_owned(),
                exact_theorem_type: "Not (∀ n : Nat, MathOS.PilotA.Prime n -> MathOS.PilotA.Odd n)"
                    .to_owned(),
                declaration_hash: "f".repeat(64),
            },
            refutation_witness: ClaimResearchStatusWitness {
                formalization,
                kind: ClaimResearchStatusWitnessKind::Refutation,
                reviewed_source_relation: ReviewedSourceRelation::LogicalNegation,
                fidelity_request_schema_version: "fidelity_review_request/2".to_owned(),
                fidelity_evidence_id: uuid::Uuid::from_u128(4).to_string(),
                fidelity_evidence_hash: "1".repeat(64),
                fidelity_report_artifact_hash: "2".repeat(64),
                authority_evidence_id: uuid::Uuid::from_u128(5).to_string(),
                authority_evidence_hash: "3".repeat(64),
                publication_receipt_hash: "4".repeat(64),
            },
            minimization: Some(CounterexampleMinimization {
                explanation: "Prime requires n >= 2, so 2 is the boundary witness.".to_owned(),
                supporting_artifact_hashes: vec!["e".repeat(64)],
            }),
            failing_assumption_explanation: "The statement overlooks the unique even prime."
                .to_owned(),
            repair_operation: ClaimRepairOperation::ExcludeBoundaryCase,
            proposed_repaired_claim: ProposedRepairedClaim {
                schema_version: CLAIM_SCHEMA_VERSION.to_owned(),
                version_hash: record_version_hash(CLAIM_SCHEMA_VERSION, &payload_value)
                    .expect("claim hash"),
                payload,
            },
            search_provenance: CounterexampleSearchProvenance {
                run_id: uuid::Uuid::from_u128(6).to_string(),
                event_head_hash: "5".repeat(64),
            },
        }
    }

    #[test]
    fn package_is_canonical_hash_stable_and_rejects_authority_mismatch() {
        let mut package = package();
        package.validate().expect("valid package");
        assert_eq!(
            package.package_hash().expect("package hash"),
            value_hash(&serde_json::to_value(&package).expect("package value"))
                .expect("value hash")
        );
        assert_eq!(
            package.canonical_bytes().expect("package bytes"),
            canonical_json(&serde_json::to_value(&package).expect("package value"))
                .expect("canonical package")
        );

        package.refutation_witness.kind = ClaimResearchStatusWitnessKind::Proof;
        assert_eq!(
            package
                .validate()
                .expect_err("proof is not a counterexample")
                .code,
            "MCL_CLAIM_RESEARCH_STATUS_INVALID"
        );
    }

    #[test]
    fn package_rejects_null_witness_bad_repair_hash_and_unsorted_artifacts() {
        let mut null_witness = package();
        null_witness.witness.canonical_value = Value::Null;
        assert_eq!(
            null_witness.validate().expect_err("null witness").code,
            "MCL_COUNTEREXAMPLE_WITNESS_INVALID"
        );

        let mut bad_repair_hash = package();
        bad_repair_hash.proposed_repaired_claim.version_hash = "0".repeat(64);
        assert_eq!(
            bad_repair_hash
                .validate()
                .expect_err("bad repair hash")
                .code,
            "MCL_CLAIM_REPAIR_HASH_MISMATCH"
        );

        let mut unsorted_artifacts = package();
        unsorted_artifacts
            .minimization
            .as_mut()
            .expect("minimization")
            .supporting_artifact_hashes = vec!["f".repeat(64), "e".repeat(64)];
        assert_eq!(
            unsorted_artifacts
                .validate()
                .expect_err("unsorted hashes")
                .code,
            "MCL_COUNTEREXAMPLE_ARTIFACT_ORDER_INVALID"
        );
    }

    #[test]
    fn committed_schemas_match_the_closed_rust_contracts() {
        let request: Value = serde_json::from_str(include_str!(
            "../../schemas/counterexample/counterexample-repair-request-1.schema.json"
        ))
        .expect("committed request schema");
        let package: Value = serde_json::from_str(include_str!(
            "../../schemas/counterexample/counterexample-package-1.schema.json"
        ))
        .expect("committed package schema");
        assert_eq!(request, counterexample_repair_request_schema());
        assert_eq!(package, counterexample_package_schema());
    }

    #[test]
    fn request_rejects_unknown_fields_during_deserialization() {
        let mut value = serde_json::to_value(CounterexampleRepairRequest {
            schema_version: COUNTEREXAMPLE_REPAIR_REQUEST_SCHEMA_VERSION.to_owned(),
            original_claim: reference(2, 'b'),
            refutation_formalization: reference(3, 'c'),
            witness: CounterexampleWitness {
                mathematical_type: "natural number".to_owned(),
                canonical_value: json!(2),
            },
            minimization: None,
            failing_assumption_explanation: "The statement overlooks 2.".to_owned(),
            repair_operation: ClaimRepairOperation::ExcludeBoundaryCase,
            proposed_repaired_claim: repaired_claim(reference(1, 'a')),
            repaired_claim_searchable_text: "prime number other than 2 odd".to_owned(),
            counterexample_search_run_id: uuid::Uuid::from_u128(6).to_string(),
            counterexample_search_run_head_hash: "5".repeat(64),
        })
        .expect("request value");
        value["status"] = json!("disproved");
        assert!(serde_json::from_value::<CounterexampleRepairRequest>(value).is_err());

        let mut package_value = serde_json::to_value(package()).expect("package value");
        package_value["caller_selected_checker"] = json!({});
        assert!(serde_json::from_value::<CounterexamplePackage>(package_value).is_err());

        let mut result = serde_json::to_value(CounterexampleSearchResult {
            schema_version: COUNTEREXAMPLE_SEARCH_RESULT_SCHEMA_VERSION.to_owned(),
            original_claim: reference(2, 'b'),
            refutation_formalization: reference(3, 'c'),
            witness: CounterexampleWitness {
                mathematical_type: "natural number".to_owned(),
                canonical_value: json!(2),
            },
            result: CounterexampleSearchResultKind::CounterexampleConfirmed,
        })
        .expect("search result value");
        result["status"] = json!("disproved");
        assert!(serde_json::from_value::<CounterexampleSearchResult>(result).is_err());
    }

    #[test]
    fn contracts_reject_oversize_witnesses_and_inexact_references() {
        let mut oversized = package();
        oversized.witness.mathematical_type = "x".repeat(MAX_TEXT_BYTES + 1);
        assert_eq!(
            oversized
                .validate()
                .expect_err("oversize witness type")
                .code,
            "MCL_COUNTEREXAMPLE_TEXT_INVALID"
        );

        let mut inexact = package();
        inexact.original_claim.object_id = "caller-selected-object".to_owned();
        assert_eq!(
            inexact
                .validate()
                .expect_err("non-UUID record reference")
                .code,
            "MCL_COUNTEREXAMPLE_REFERENCE_INVALID"
        );
    }
}
