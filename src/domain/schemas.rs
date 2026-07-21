use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use super::RecordKind;
use crate::error::AppError;

pub const SOURCE_SCHEMA_VERSION: &str = "source/1";
pub const CLAIM_SCHEMA_VERSION: &str = "claim/1";
pub const CONCEPT_SCHEMA_VERSION: &str = "concept/1";
pub const FORMALIZATION_SCHEMA_VERSION: &str = "formalization/1";
pub const LEARNING_UNIT_SCHEMA_VERSION: &str = "learning_unit/1";
const MAX_TEXT_BYTES: usize = 1_048_576;
const MAX_ITEMS: usize = 1_000;
const MAX_LICENSE_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Paper,
    Textbook,
    Benchmark,
    Repository,
    Webpage,
    Dataset,
    UserStatement,
    ConversationExcerpt,
    HistoricalArchive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RedistributionStatus {
    Allowed,
    Restricted,
    Prohibited,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionClass {
    Public,
    Restricted,
    Private,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePayload {
    pub source_type: SourceType,
    pub title_or_label: String,
    pub authors_or_origin: Vec<String>,
    pub canonical_locator: String,
    pub acquisition_date: String,
    pub license_expression: Option<String>,
    pub redistribution_status: RedistributionStatus,
    pub content_hash: Option<String>,
    pub citation_metadata: BTreeMap<String, String>,
    pub redaction_class: RedactionClass,
    pub provenance_notes: String,
    pub original_text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Universal,
    Existential,
    Equality,
    Inequality,
    Equivalence,
    Classification,
    FiniteComputation,
    OpenQuestion,
    DefinitionSoundness,
    MethodClaim,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactVersionReference {
    pub object_id: String,
    pub version_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VariableDomainNote {
    pub symbol: String,
    pub domain: String,
    pub notes: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimPayload {
    pub source_reference: ExactVersionReference,
    pub normalized_informal_statement: String,
    pub claim_kind: ClaimKind,
    pub logical_shape: Option<String>,
    pub assumptions: Vec<String>,
    pub variables: Vec<VariableDomainNote>,
    pub concept_links: Vec<ExactVersionReference>,
    pub source_citations: Vec<ExactVersionReference>,
    pub ambiguity_notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalDeclarationReference {
    pub environment_hash: String,
    pub declaration_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalTaxonomyCrosswalk {
    pub taxonomy_name: String,
    pub external_id: String,
    pub source_reference: ExactVersionReference,
    pub license_expression: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConceptPayload {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub subject_domains: Vec<String>,
    pub formal_declarations: Vec<FormalDeclarationReference>,
    pub external_taxonomy_crosswalks: Vec<ExternalTaxonomyCrosswalk>,
    pub pedagogy_metadata_references: Vec<ExactVersionReference>,
    pub provenance_references: Vec<ExactVersionReference>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormalSystem {
    Lean4,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormalizationClaimPolarity {
    Claim,
    Negation,
}

fn deserialize_claim_polarity<'de, D>(
    deserializer: D,
) -> Result<Option<FormalizationClaimPolarity>, D::Error>
where
    D: Deserializer<'de>,
{
    FormalizationClaimPolarity::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalizationPayload {
    pub claim_version: ExactVersionReference,
    pub formal_system: FormalSystem,
    #[serde(
        default,
        deserialize_with = "deserialize_claim_polarity",
        skip_serializing_if = "Option::is_none"
    )]
    pub claim_polarity: Option<FormalizationClaimPolarity>,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
    pub exact_theorem_type: String,
    pub declaration_hash: String,
    pub import_manifest: Vec<String>,
    pub formalization_notes: String,
    pub fidelity_evidence_references: Vec<String>,
    pub verification_evidence_references: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningUnitKind {
    Motivation,
    Definition,
    Explanation,
    Example,
    Nonexample,
    Counterexample,
    Misconception,
    WorkedProof,
    Exercise,
    MasteryCheck,
    Application,
    History,
    FrontierNote,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningTargetKind {
    Claim,
    Concept,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearningTargetReference {
    pub kind: LearningTargetKind,
    pub object_id: String,
    pub version_hash: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningUnitReviewState {
    Draft,
    Reviewed,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearningUnitReview {
    pub state: LearningUnitReviewState,
    pub reviewer: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningUnitTrainingStatus {
    Ineligible,
    Quarantined,
    EligiblePrivate,
    EligiblePublic,
    HeldOutEvaluation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearningUnitPayload {
    pub unit_kind: LearningUnitKind,
    pub target: LearningTargetReference,
    pub audience_track: String,
    pub entry_assumptions: Vec<String>,
    pub learning_objectives: Vec<String>,
    pub hard_prerequisites: Vec<ExactVersionReference>,
    pub soft_prerequisites: Vec<ExactVersionReference>,
    pub grounded_source_references: Vec<ExactVersionReference>,
    pub content_artifact_hash: String,
    pub examples: Vec<ExactVersionReference>,
    pub nonexamples: Vec<ExactVersionReference>,
    pub counterexamples: Vec<ExactVersionReference>,
    pub misconceptions: Vec<ExactVersionReference>,
    pub exercises: Vec<ExactVersionReference>,
    pub mastery_checks: Vec<ExactVersionReference>,
    pub formalization_references: Vec<ExactVersionReference>,
    pub application_references: Vec<ExactVersionReference>,
    pub frontier_references: Vec<ExactVersionReference>,
    pub review: LearningUnitReview,
    pub license_expression: Option<String>,
    pub training_status: LearningUnitTrainingStatus,
}

pub fn validate_record_payload(
    kind: RecordKind,
    schema_version: &str,
    payload: &Value,
) -> Result<(), AppError> {
    match kind {
        RecordKind::Source => {
            require_schema_version(kind, schema_version, SOURCE_SCHEMA_VERSION)?;
            let source: SourcePayload = decode(kind, payload)?;
            validate_source(&source)
        }
        RecordKind::Claim => {
            require_schema_version(kind, schema_version, CLAIM_SCHEMA_VERSION)?;
            let claim: ClaimPayload = decode(kind, payload)?;
            validate_claim(&claim)
        }
        RecordKind::Concept => {
            require_schema_version(kind, schema_version, CONCEPT_SCHEMA_VERSION)?;
            let concept: ConceptPayload = decode(kind, payload)?;
            validate_concept(&concept)
        }
        RecordKind::Formalization => {
            require_schema_version(kind, schema_version, FORMALIZATION_SCHEMA_VERSION)?;
            let formalization: FormalizationPayload = decode(kind, payload)?;
            validate_formalization(&formalization)
        }
        RecordKind::LearningUnit => {
            require_schema_version(kind, schema_version, LEARNING_UNIT_SCHEMA_VERSION)?;
            let learning_unit: LearningUnitPayload = decode(kind, payload)?;
            validate_learning_unit(&learning_unit)
        }
    }
}

pub fn source_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/source/1",
        "title": "MathOS Source Payload v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["source_type", "title_or_label", "authors_or_origin", "canonical_locator", "acquisition_date", "license_expression", "redistribution_status", "content_hash", "citation_metadata", "redaction_class", "provenance_notes", "original_text"],
        "properties": {
            "source_type": {"enum": ["paper", "textbook", "benchmark", "repository", "webpage", "dataset", "user_statement", "conversation_excerpt", "historical_archive"]},
            "title_or_label": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES},
            "authors_or_origin": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}},
            "canonical_locator": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES},
            "acquisition_date": {"type": "string", "minLength": 1, "maxLength": 64},
            "license_expression": {"type": ["string", "null"], "maxLength": MAX_TEXT_BYTES},
            "redistribution_status": {"enum": ["allowed", "restricted", "prohibited", "unknown"]},
            "content_hash": {"type": ["string", "null"], "pattern": "^[0-9a-f]{64}$"},
            "citation_metadata": {"type": "object", "maxProperties": MAX_ITEMS, "additionalProperties": {"type": "string", "maxLength": MAX_TEXT_BYTES}},
            "redaction_class": {"enum": ["public", "restricted", "private"]},
            "provenance_notes": {"type": "string", "maxLength": MAX_TEXT_BYTES},
            "original_text": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}
        }
    })
}

pub fn claim_schema() -> Value {
    let reference = exact_reference_schema();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/claim/1",
        "title": "MathOS Claim Payload v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["source_reference", "normalized_informal_statement", "claim_kind", "logical_shape", "assumptions", "variables", "concept_links", "source_citations", "ambiguity_notes"],
        "properties": {
            "source_reference": reference.clone(),
            "normalized_informal_statement": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES},
            "claim_kind": {"enum": ["universal", "existential", "equality", "inequality", "equivalence", "classification", "finite_computation", "open_question", "definition_soundness", "method_claim"]},
            "logical_shape": {"type": ["string", "null"], "maxLength": MAX_TEXT_BYTES},
            "assumptions": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}},
            "variables": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "object", "additionalProperties": false, "required": ["symbol", "domain", "notes"], "properties": {"symbol": {"type": "string", "minLength": 1, "maxLength": 256}, "domain": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}, "notes": {"type": "string", "maxLength": MAX_TEXT_BYTES}}}},
            "concept_links": {"type": "array", "maxItems": MAX_ITEMS, "items": reference.clone()},
            "source_citations": {"type": "array", "maxItems": MAX_ITEMS, "items": reference},
            "ambiguity_notes": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}}
        }
    })
}

pub fn concept_schema() -> Value {
    let reference = exact_reference_schema();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/concept/1",
        "title": "MathOS Concept Payload v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "aliases", "description", "subject_domains", "formal_declarations", "external_taxonomy_crosswalks", "pedagogy_metadata_references", "provenance_references"],
        "properties": {
            "name": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES},
            "aliases": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}},
            "description": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES},
            "subject_domains": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}},
            "formal_declarations": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "object", "additionalProperties": false, "required": ["environment_hash", "declaration_name"], "properties": {"environment_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}, "declaration_name": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}}}},
            "external_taxonomy_crosswalks": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "object", "additionalProperties": false, "required": ["taxonomy_name", "external_id", "source_reference", "license_expression"], "properties": {"taxonomy_name": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}, "external_id": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}, "source_reference": reference.clone(), "license_expression": {"type": "string", "minLength": 1, "maxLength": MAX_LICENSE_BYTES}}}},
            "pedagogy_metadata_references": {"type": "array", "maxItems": MAX_ITEMS, "items": reference.clone()},
            "provenance_references": {"type": "array", "maxItems": MAX_ITEMS, "items": reference}
        }
    })
}

pub fn formalization_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/formalization/1",
        "title": "MathOS Formalization Payload v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["claim_version", "formal_system", "environment_hash", "module_artifact_hash", "declaration_name", "exact_theorem_type", "declaration_hash", "import_manifest", "formalization_notes", "fidelity_evidence_references", "verification_evidence_references"],
        "properties": {
            "claim_version": exact_reference_schema(),
            "formal_system": {"enum": ["lean4"]},
            "claim_polarity": {"enum": ["claim", "negation"]},
            "environment_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "module_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "declaration_name": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES},
            "exact_theorem_type": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES},
            "declaration_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "import_manifest": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}},
            "formalization_notes": {"type": "string", "maxLength": MAX_TEXT_BYTES},
            "fidelity_evidence_references": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": 128}},
            "verification_evidence_references": {"type": "array", "maxItems": MAX_ITEMS, "items": {"type": "string", "minLength": 1, "maxLength": 128}}
        }
    })
}

pub fn learning_unit_schema() -> Value {
    let exact_reference = exact_reference_schema();
    let reference = json!({"$ref": "#/$defs/exactVersionReference"});
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/learning-unit/1",
        "title": "MathOS Canonical Learning Unit Payload v1",
        "type": "object",
        "additionalProperties": false,
        "required": [
            "unit_kind", "target", "audience_track", "entry_assumptions",
            "learning_objectives", "hard_prerequisites", "soft_prerequisites",
            "grounded_source_references", "content_artifact_hash", "examples",
            "nonexamples", "counterexamples", "misconceptions", "exercises",
            "mastery_checks", "formalization_references", "application_references",
            "frontier_references", "review", "license_expression", "training_status"
        ],
        "properties": {
            "unit_kind": {"enum": ["motivation", "definition", "explanation", "example", "nonexample", "counterexample", "misconception", "worked_proof", "exercise", "mastery_check", "application", "history", "frontier_note"]},
            "target": {
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "object_id", "version_hash"],
                "properties": {
                    "kind": {"enum": ["claim", "concept"]},
                    "object_id": {"type": "string", "minLength": 1, "maxLength": 128},
                    "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
                }
            },
            "audience_track": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES},
            "entry_assumptions": text_array_schema(),
            "learning_objectives": text_array_schema(),
            "hard_prerequisites": reference_array_schema(&reference),
            "soft_prerequisites": reference_array_schema(&reference),
            "grounded_source_references": reference_array_schema(&reference),
            "content_artifact_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "examples": reference_array_schema(&reference),
            "nonexamples": reference_array_schema(&reference),
            "counterexamples": reference_array_schema(&reference),
            "misconceptions": reference_array_schema(&reference),
            "exercises": reference_array_schema(&reference),
            "mastery_checks": reference_array_schema(&reference),
            "formalization_references": reference_array_schema(&reference),
            "application_references": reference_array_schema(&reference),
            "frontier_references": reference_array_schema(&reference),
            "review": {
                "type": "object",
                "additionalProperties": false,
                "required": ["state", "reviewer", "notes"],
                "properties": {
                    "state": {"enum": ["draft", "reviewed", "rejected"]},
                    "reviewer": {"type": ["string", "null"], "minLength": 1, "maxLength": 256},
                    "notes": text_array_schema()
                }
            },
            "license_expression": {"type": ["string", "null"], "minLength": 1, "maxLength": MAX_LICENSE_BYTES},
            "training_status": {"enum": ["ineligible", "quarantined", "eligible_private", "eligible_public", "held_out_evaluation"]}
        },
        "$defs": {"exactVersionReference": exact_reference}
    })
}

fn text_array_schema() -> Value {
    json!({
        "type": "array",
        "maxItems": MAX_ITEMS,
        "items": {"type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES}
    })
}

fn reference_array_schema(reference: &Value) -> Value {
    json!({
        "type": "array",
        "maxItems": MAX_ITEMS,
        "items": reference
    })
}

fn exact_reference_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["object_id", "version_hash"],
        "properties": {
            "object_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        }
    })
}

fn decode<T: for<'de> Deserialize<'de>>(kind: RecordKind, value: &Value) -> Result<T, AppError> {
    serde_json::from_value(value.clone()).map_err(|error| {
        AppError::new(
            "MCL_SCHEMA_VALIDATION_FAILED",
            format!(
                "{} payload does not match its schema: {error}",
                kind.as_str()
            ),
            false,
            "Submit a payload matching the committed schema and schema version.",
        )
    })
}

fn require_schema_version(kind: RecordKind, actual: &str, expected: &str) -> Result<(), AppError> {
    if actual != expected {
        return Err(AppError::new(
            "MCL_SCHEMA_VERSION_UNSUPPORTED",
            format!(
                "{} records require schema `{expected}`, received `{actual}`",
                kind.as_str()
            ),
            false,
            "Use a committed schema version or add a reviewed migration before writing.",
        ));
    }
    Ok(())
}

fn validate_source(source: &SourcePayload) -> Result<(), AppError> {
    nonempty("source title or label", &source.title_or_label)?;
    nonempty("source canonical locator", &source.canonical_locator)?;
    nonempty("source acquisition date", &source.acquisition_date)?;
    nonempty("source original text", &source.original_text)?;
    bounded_items("source authors or origin", &source.authors_or_origin)?;
    bounded_items("source citation metadata", &source.citation_metadata)?;
    if let Some(hash) = &source.content_hash {
        valid_hash(hash, "source content hash")?;
    }
    for value in source
        .authors_or_origin
        .iter()
        .chain(source.citation_metadata.values())
    {
        nonempty("source attribution or citation value", value)?;
    }
    Ok(())
}

fn validate_claim(claim: &ClaimPayload) -> Result<(), AppError> {
    validate_reference(&claim.source_reference, "claim source reference")?;
    nonempty(
        "claim normalized informal statement",
        &claim.normalized_informal_statement,
    )?;
    bounded_items("claim assumptions", &claim.assumptions)?;
    bounded_items("claim variables", &claim.variables)?;
    bounded_items("claim concept links", &claim.concept_links)?;
    bounded_items("claim source citations", &claim.source_citations)?;
    bounded_items("claim ambiguity notes", &claim.ambiguity_notes)?;
    for text in claim.assumptions.iter().chain(&claim.ambiguity_notes) {
        nonempty("claim list text", text)?;
    }
    for variable in &claim.variables {
        nonempty("claim variable symbol", &variable.symbol)?;
        nonempty("claim variable domain", &variable.domain)?;
    }
    for reference in claim.concept_links.iter().chain(&claim.source_citations) {
        validate_reference(reference, "claim linked version")?;
    }
    Ok(())
}

fn validate_concept(concept: &ConceptPayload) -> Result<(), AppError> {
    nonempty("concept name", &concept.name)?;
    nonempty("concept description", &concept.description)?;
    bounded_items("concept aliases", &concept.aliases)?;
    bounded_items("concept subject domains", &concept.subject_domains)?;
    bounded_items("concept formal declarations", &concept.formal_declarations)?;
    bounded_items(
        "concept external taxonomy crosswalks",
        &concept.external_taxonomy_crosswalks,
    )?;
    bounded_items(
        "concept pedagogy metadata references",
        &concept.pedagogy_metadata_references,
    )?;
    bounded_items(
        "concept provenance references",
        &concept.provenance_references,
    )?;
    for text in concept.aliases.iter().chain(&concept.subject_domains) {
        nonempty("concept alias or subject domain", text)?;
    }
    for declaration in &concept.formal_declarations {
        valid_hash(
            &declaration.environment_hash,
            "formal declaration environment",
        )?;
        nonempty("formal declaration name", &declaration.declaration_name)?;
    }
    let mut crosswalk_ids = std::collections::BTreeSet::new();
    for crosswalk in &concept.external_taxonomy_crosswalks {
        nonempty("taxonomy name", &crosswalk.taxonomy_name)?;
        nonempty("taxonomy external ID", &crosswalk.external_id)?;
        validate_license_expression(
            &crosswalk.license_expression,
            "MCL_TAXONOMY_LICENSE_INVALID",
            "taxonomy crosswalk",
        )?;
        validate_reference(&crosswalk.source_reference, "taxonomy source reference")?;
        if !crosswalk_ids.insert((&crosswalk.taxonomy_name, &crosswalk.external_id)) {
            return Err(AppError::new(
                "MCL_TAXONOMY_CROSSWALK_DUPLICATE",
                "concept contains a duplicate taxonomy name and external ID",
                false,
                "Keep one stable crosswalk per external taxonomy identity.",
            ));
        }
    }
    for reference in concept
        .pedagogy_metadata_references
        .iter()
        .chain(&concept.provenance_references)
    {
        validate_reference(reference, "concept linked version")?;
    }
    Ok(())
}

fn validate_formalization(formalization: &FormalizationPayload) -> Result<(), AppError> {
    validate_reference(&formalization.claim_version, "formalization claim version")?;
    valid_hash(&formalization.environment_hash, "formalization environment")?;
    valid_hash(
        &formalization.module_artifact_hash,
        "formalization module artifact",
    )?;
    valid_hash(&formalization.declaration_hash, "formalization declaration")?;
    nonempty(
        "formalization declaration name",
        &formalization.declaration_name,
    )?;
    nonempty(
        "formalization exact theorem type",
        &formalization.exact_theorem_type,
    )?;
    bounded_items(
        "formalization import manifest",
        &formalization.import_manifest,
    )?;
    bounded_items(
        "formalization fidelity evidence references",
        &formalization.fidelity_evidence_references,
    )?;
    bounded_items(
        "formalization verification evidence references",
        &formalization.verification_evidence_references,
    )?;
    for item in formalization.import_manifest.iter() {
        nonempty("formalization list item", item)?;
    }
    for reference in formalization
        .fidelity_evidence_references
        .iter()
        .chain(&formalization.verification_evidence_references)
    {
        bounded_text("formalization evidence reference", reference, 128)?;
    }
    Ok(())
}

fn validate_learning_unit(learning_unit: &LearningUnitPayload) -> Result<(), AppError> {
    validate_reference(
        &ExactVersionReference {
            object_id: learning_unit.target.object_id.clone(),
            version_hash: learning_unit.target.version_hash.clone(),
        },
        "learning unit target",
    )?;
    nonempty(
        "learning unit audience track",
        &learning_unit.audience_track,
    )?;
    valid_hash(
        &learning_unit.content_artifact_hash,
        "learning unit content artifact",
    )?;

    let text_collections = [
        (
            "learning unit entry assumptions",
            &learning_unit.entry_assumptions,
        ),
        (
            "learning unit objectives",
            &learning_unit.learning_objectives,
        ),
        ("learning unit review notes", &learning_unit.review.notes),
    ];
    for (label, values) in text_collections {
        bounded_items(label, values)?;
        for value in values {
            nonempty(label, value)?;
        }
    }
    let total_text_bytes = learning_unit.audience_track.len()
        + learning_unit
            .entry_assumptions
            .iter()
            .chain(&learning_unit.learning_objectives)
            .chain(&learning_unit.review.notes)
            .map(String::len)
            .sum::<usize>()
        + learning_unit.review.reviewer.as_deref().map_or(0, str::len);
    if total_text_bytes > MAX_TEXT_BYTES {
        return Err(AppError::new(
            "MCL_PEDAGOGY_TEXT_LIMIT",
            "combined learning-unit metadata text exceeds the 1 MiB bound",
            false,
            "Move lesson prose into the content artifact and keep metadata concise.",
        ));
    }
    if learning_unit.learning_objectives.is_empty() {
        return Err(AppError::new(
            "MCL_PEDAGOGY_OBJECTIVE_REQUIRED",
            "learning units require at least one explicit learning objective",
            false,
            "State the observable outcome this unit is intended to teach.",
        ));
    }
    if learning_unit.grounded_source_references.is_empty() {
        return Err(AppError::new(
            "MCL_PEDAGOGY_GROUNDING_REQUIRED",
            "learning units require at least one exact grounded source reference",
            false,
            "Ground the unit in an exact canonical source version.",
        ));
    }

    let reference_collections = [
        ("hard prerequisites", &learning_unit.hard_prerequisites),
        ("soft prerequisites", &learning_unit.soft_prerequisites),
        (
            "grounded sources",
            &learning_unit.grounded_source_references,
        ),
        ("examples", &learning_unit.examples),
        ("nonexamples", &learning_unit.nonexamples),
        ("counterexamples", &learning_unit.counterexamples),
        ("misconceptions", &learning_unit.misconceptions),
        ("exercises", &learning_unit.exercises),
        ("mastery checks", &learning_unit.mastery_checks),
        ("formalizations", &learning_unit.formalization_references),
        ("applications", &learning_unit.application_references),
        ("frontier notes", &learning_unit.frontier_references),
    ];
    for (label, references) in reference_collections {
        bounded_items(label, references)?;
        let mut unique = std::collections::BTreeSet::new();
        for reference in references {
            validate_reference(reference, label)?;
            if !unique.insert(reference) {
                return Err(AppError::new(
                    "MCL_PEDAGOGY_REFERENCE_DUPLICATE",
                    format!("learning unit {label} contain a duplicate exact reference"),
                    false,
                    "Keep each exact canonical reference at most once per field.",
                ));
            }
        }
    }
    let total_references = 1
        + learning_unit.hard_prerequisites.len()
        + learning_unit.soft_prerequisites.len()
        + learning_unit.grounded_source_references.len()
        + learning_unit.examples.len()
        + learning_unit.nonexamples.len()
        + learning_unit.counterexamples.len()
        + learning_unit.misconceptions.len()
        + learning_unit.exercises.len()
        + learning_unit.mastery_checks.len()
        + learning_unit.formalization_references.len()
        + learning_unit.application_references.len()
        + learning_unit.frontier_references.len();
    if total_references > MAX_ITEMS {
        return Err(AppError::new(
            "MCL_PEDAGOGY_REFERENCE_LIMIT",
            "combined learning-unit references exceed the 1000-reference bound",
            false,
            "Split the unit or reduce its direct grounding and relationship set.",
        ));
    }
    if learning_unit
        .hard_prerequisites
        .iter()
        .any(|reference| learning_unit.soft_prerequisites.contains(reference))
    {
        return Err(AppError::new(
            "MCL_PEDAGOGY_PREREQUISITE_AMBIGUOUS",
            "the same exact learning unit cannot be both a hard and soft prerequisite",
            false,
            "Classify each prerequisite as hard or soft, never both.",
        ));
    }
    if learning_unit.hard_prerequisites.len() + learning_unit.soft_prerequisites.len() > 999 {
        return Err(AppError::new(
            "MCL_PEDAGOGY_PREREQUISITE_LIMIT",
            "combined hard and soft prerequisites exceed the 999-edge validation bound",
            false,
            "Split the curriculum unit or reduce its direct prerequisite set.",
        ));
    }

    match learning_unit.review.state {
        LearningUnitReviewState::Draft => {
            if learning_unit.review.reviewer.is_some() || !learning_unit.review.notes.is_empty() {
                return Err(AppError::new(
                    "MCL_PEDAGOGY_REVIEW_INVALID",
                    "draft learning units cannot carry reviewer identity or review notes",
                    false,
                    "Use the controlled pedagogy review action to record a decision.",
                ));
            }
        }
        LearningUnitReviewState::Reviewed | LearningUnitReviewState::Rejected => {
            let reviewer = learning_unit.review.reviewer.as_deref().ok_or_else(|| {
                AppError::new(
                    "MCL_PEDAGOGY_REVIEW_INVALID",
                    "reviewed or rejected learning units require reviewer identity",
                    false,
                    "Use the controlled pedagogy review action with actor attribution.",
                )
            })?;
            bounded_text("learning unit reviewer", reviewer, 256)?;
            if learning_unit.review.notes.is_empty() {
                return Err(AppError::new(
                    "MCL_PEDAGOGY_REVIEW_INVALID",
                    "reviewed or rejected learning units require review notes",
                    false,
                    "Record a bounded rationale for the review decision.",
                ));
            }
        }
    }

    if let Some(license) = &learning_unit.license_expression {
        validate_license_expression(license, "MCL_PEDAGOGY_LICENSE_INVALID", "learning unit")?;
    }
    if matches!(
        learning_unit.training_status,
        LearningUnitTrainingStatus::EligiblePrivate | LearningUnitTrainingStatus::EligiblePublic
    ) && (learning_unit.review.state != LearningUnitReviewState::Reviewed
        || learning_unit.license_expression.is_none())
    {
        return Err(AppError::new(
            "MCL_PEDAGOGY_TRAINING_POLICY",
            "training-eligible learning units require reviewed state and a resolved license",
            false,
            "Complete controlled review and resolve the content license before eligibility.",
        ));
    }
    if learning_unit.review.state == LearningUnitReviewState::Rejected
        && !matches!(
            learning_unit.training_status,
            LearningUnitTrainingStatus::Ineligible | LearningUnitTrainingStatus::Quarantined
        )
    {
        return Err(AppError::new(
            "MCL_PEDAGOGY_TRAINING_POLICY",
            "rejected learning units cannot be training eligible or held out for evaluation",
            false,
            "Mark rejected content ineligible or quarantined.",
        ));
    }
    Ok(())
}

fn validate_license_expression(
    value: &str,
    code: &'static str,
    label: &str,
) -> Result<(), AppError> {
    let trimmed = value.trim();
    let valid = !trimmed.is_empty()
        && trimmed.len() <= MAX_LICENSE_BYTES
        && trimmed.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'+' | b'(' | b')' | b' ')
        })
        && !matches!(trimmed.to_ascii_lowercase().as_str(), "unknown" | "none");
    if !valid {
        return Err(AppError::new(
            code,
            format!("{label} license expression is unresolved or malformed"),
            false,
            "Supply a concise reviewed SPDX expression.",
        ));
    }
    Ok(())
}

fn validate_reference(reference: &ExactVersionReference, label: &str) -> Result<(), AppError> {
    bounded_text(label, &reference.object_id, 128)?;
    valid_hash(&reference.version_hash, label)
}

fn valid_hash(hash: &str, label: &str) -> Result<(), AppError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::new(
            "MCL_SCHEMA_HASH_INVALID",
            format!("{label} must be a lowercase hexadecimal SHA-256 identity"),
            false,
            "Use the exact hash returned by canonical lookup or artifact storage.",
        ));
    }
    Ok(())
}

fn nonempty(label: &str, value: &str) -> Result<(), AppError> {
    bounded_text(label, value, MAX_TEXT_BYTES)
}

fn bounded_text(label: &str, value: &str, maximum: usize) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > maximum {
        return Err(AppError::new(
            "MCL_SCHEMA_TEXT_INVALID",
            format!("{label} must be nonempty and no larger than {maximum} bytes"),
            false,
            "Supply bounded, explicit text required by the committed schema.",
        ));
    }
    Ok(())
}

fn bounded_items(label: &str, values: &impl CollectionLength) -> Result<(), AppError> {
    if values.collection_len() > MAX_ITEMS {
        return Err(AppError::new(
            "MCL_SCHEMA_COLLECTION_TOO_LARGE",
            format!("{label} exceeds the {MAX_ITEMS}-item limit"),
            false,
            "Split the record or reduce the collection to the committed bound.",
        ));
    }
    Ok(())
}

trait CollectionLength {
    fn collection_len(&self) -> usize;
}

impl<T> CollectionLength for Vec<T> {
    fn collection_len(&self) -> usize {
        self.len()
    }
}

impl<K, V> CollectionLength for BTreeMap<K, V> {
    fn collection_len(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_schemas_match_the_rust_owned_contract() {
        let committed_source: Value =
            serde_json::from_str(include_str!("../../schemas/source/source-1.schema.json"))
                .expect("committed source schema");
        let committed_claim: Value =
            serde_json::from_str(include_str!("../../schemas/claim/claim-1.schema.json"))
                .expect("committed claim schema");
        let committed_concept: Value =
            serde_json::from_str(include_str!("../../schemas/concept/concept-1.schema.json"))
                .expect("committed concept schema");
        let committed_formalization: Value = serde_json::from_str(include_str!(
            "../../schemas/formalization/formalization-1.schema.json"
        ))
        .expect("committed formalization schema");
        let committed_learning_unit: Value = serde_json::from_str(include_str!(
            "../../schemas/pedagogy/learning-unit-1.schema.json"
        ))
        .expect("committed learning-unit schema");
        assert_eq!(committed_source, source_schema());
        assert_eq!(committed_claim, claim_schema());
        assert_eq!(committed_concept, concept_schema());
        assert_eq!(committed_formalization, formalization_schema());
        assert_eq!(committed_learning_unit, learning_unit_schema());
    }

    #[test]
    fn unknown_claim_fields_fail_closed() {
        let payload = json!({
            "source_reference": {"object_id": "source", "version_hash": "a".repeat(64)},
            "normalized_informal_statement": "Every prime number is odd.",
            "claim_kind": "universal",
            "logical_shape": "forall",
            "assumptions": [],
            "variables": [],
            "concept_links": [],
            "source_citations": [],
            "ambiguity_notes": [],
            "proved": true
        });
        assert_eq!(
            validate_record_payload(RecordKind::Claim, CLAIM_SCHEMA_VERSION, &payload)
                .expect_err("unknown field")
                .code,
            "MCL_SCHEMA_VALIDATION_FAILED"
        );
    }

    #[test]
    fn malformed_hash_empty_text_and_excessive_collections_fail_closed() {
        let base = json!({
            "source_reference": {"object_id": "source", "version_hash": "a".repeat(64)},
            "normalized_informal_statement": "Every prime number is odd.",
            "claim_kind": "universal",
            "logical_shape": "forall",
            "assumptions": [],
            "variables": [],
            "concept_links": [],
            "source_citations": [],
            "ambiguity_notes": []
        });
        let mut malformed = base.clone();
        malformed["source_reference"]["version_hash"] = json!("not-a-hash");
        assert_eq!(
            validate_record_payload(RecordKind::Claim, CLAIM_SCHEMA_VERSION, &malformed)
                .expect_err("malformed hash")
                .code,
            "MCL_SCHEMA_HASH_INVALID"
        );
        let mut empty = base.clone();
        empty["normalized_informal_statement"] = json!("  ");
        assert_eq!(
            validate_record_payload(RecordKind::Claim, CLAIM_SCHEMA_VERSION, &empty)
                .expect_err("empty statement")
                .code,
            "MCL_SCHEMA_TEXT_INVALID"
        );
        let mut excessive = base;
        excessive["ambiguity_notes"] = json!(vec!["note"; MAX_ITEMS + 1]);
        assert_eq!(
            validate_record_payload(RecordKind::Claim, CLAIM_SCHEMA_VERSION, &excessive)
                .expect_err("excessive collection")
                .code,
            "MCL_SCHEMA_COLLECTION_TOO_LARGE"
        );
    }

    #[test]
    fn formalization_cannot_embed_truth_or_fidelity_verdicts() {
        let mut payload = json!({
            "claim_version": {"object_id": "claim", "version_hash": "a".repeat(64)},
            "formal_system": "lean4",
            "environment_hash": "b".repeat(64),
            "module_artifact_hash": "c".repeat(64),
            "declaration_name": "MathOS.Example",
            "exact_theorem_type": "True",
            "declaration_hash": "d".repeat(64),
            "import_manifest": ["Mathlib"],
            "formalization_notes": "an interpretation, not a verdict",
            "fidelity_evidence_references": [],
            "verification_evidence_references": []
        });
        for prohibited in ["proved", "disproved", "faithful", "certified"] {
            payload[prohibited] = json!(true);
            assert_eq!(
                validate_record_payload(
                    RecordKind::Formalization,
                    FORMALIZATION_SCHEMA_VERSION,
                    &payload,
                )
                .expect_err("verdict field must be rejected")
                .code,
                "MCL_SCHEMA_VALIDATION_FAILED"
            );
            payload.as_object_mut().expect("object").remove(prohibited);
        }
    }

    #[test]
    fn formalization_claim_polarity_is_typed_and_identity_bearing() {
        let mut payload = json!({
            "claim_version": {"object_id": "claim", "version_hash": "a".repeat(64)},
            "formal_system": "lean4",
            "claim_polarity": "claim",
            "environment_hash": "b".repeat(64),
            "module_artifact_hash": "c".repeat(64),
            "declaration_name": "MathOS.Example",
            "exact_theorem_type": "True",
            "declaration_hash": "d".repeat(64),
            "import_manifest": [],
            "formalization_notes": "typed publication polarity",
            "fidelity_evidence_references": [],
            "verification_evidence_references": []
        });
        validate_record_payload(
            RecordKind::Formalization,
            FORMALIZATION_SCHEMA_VERSION,
            &payload,
        )
        .expect("claim-polarity formalization");
        let claim_hash =
            crate::canonical::record_version_hash(FORMALIZATION_SCHEMA_VERSION, &payload)
                .expect("claim polarity hash");

        payload["claim_polarity"] = json!("negation");
        validate_record_payload(
            RecordKind::Formalization,
            FORMALIZATION_SCHEMA_VERSION,
            &payload,
        )
        .expect("negation-polarity formalization");
        assert_ne!(
            crate::canonical::record_version_hash(FORMALIZATION_SCHEMA_VERSION, &payload)
                .expect("negation polarity hash"),
            claim_hash
        );

        payload["claim_polarity"] = json!("unknown");
        assert_eq!(
            validate_record_payload(
                RecordKind::Formalization,
                FORMALIZATION_SCHEMA_VERSION,
                &payload,
            )
            .expect_err("unknown polarity fails closed")
            .code,
            "MCL_SCHEMA_VALIDATION_FAILED"
        );

        payload["claim_polarity"] = Value::Null;
        assert_eq!(
            validate_record_payload(
                RecordKind::Formalization,
                FORMALIZATION_SCHEMA_VERSION,
                &payload,
            )
            .expect_err("explicit null polarity fails closed")
            .code,
            "MCL_SCHEMA_VALIDATION_FAILED"
        );
    }

    #[test]
    fn learning_units_fail_closed_on_unknown_fields_and_training_policy() {
        let mut payload = json!({
            "unit_kind": "explanation",
            "target": {"kind": "claim", "object_id": "claim", "version_hash": "a".repeat(64)},
            "audience_track": "Pilot A",
            "entry_assumptions": [],
            "learning_objectives": ["Distinguish the original claim from its repair."],
            "hard_prerequisites": [],
            "soft_prerequisites": [],
            "grounded_source_references": [{"object_id": "source", "version_hash": "b".repeat(64)}],
            "content_artifact_hash": "c".repeat(64),
            "examples": [],
            "nonexamples": [],
            "counterexamples": [],
            "misconceptions": [],
            "exercises": [],
            "mastery_checks": [],
            "formalization_references": [],
            "application_references": [],
            "frontier_references": [],
            "review": {"state": "draft", "reviewer": null, "notes": []},
            "license_expression": "CC-BY-4.0",
            "training_status": "ineligible"
        });
        validate_record_payload(
            RecordKind::LearningUnit,
            LEARNING_UNIT_SCHEMA_VERSION,
            &payload,
        )
        .expect("well-formed draft");

        payload["authoritative"] = json!(true);
        assert_eq!(
            validate_record_payload(
                RecordKind::LearningUnit,
                LEARNING_UNIT_SCHEMA_VERSION,
                &payload,
            )
            .expect_err("unknown authority field")
            .code,
            "MCL_SCHEMA_VALIDATION_FAILED"
        );
        payload
            .as_object_mut()
            .expect("learning unit object")
            .remove("authoritative");

        payload["training_status"] = json!("eligible_public");
        assert_eq!(
            validate_record_payload(
                RecordKind::LearningUnit,
                LEARNING_UNIT_SCHEMA_VERSION,
                &payload,
            )
            .expect_err("draft cannot be eligible")
            .code,
            "MCL_PEDAGOGY_TRAINING_POLICY"
        );
    }
}
