use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::RecordKind;
use crate::error::AppError;

pub const SOURCE_SCHEMA_VERSION: &str = "source/1";
pub const CLAIM_SCHEMA_VERSION: &str = "claim/1";
const MAX_TEXT_BYTES: usize = 1_048_576;
const MAX_ITEMS: usize = 1_000;

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
        RecordKind::Concept | RecordKind::Formalization | RecordKind::LearningUnit => Ok(()),
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
    let reference = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["object_id", "version_hash"],
        "properties": {
            "object_id": {"type": "string", "minLength": 1, "maxLength": 128},
            "version_hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        }
    });
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

fn validate_reference(reference: &ExactVersionReference, label: &str) -> Result<(), AppError> {
    nonempty(label, &reference.object_id)?;
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
    if value.trim().is_empty() || value.len() > MAX_TEXT_BYTES {
        return Err(AppError::new(
            "MCL_SCHEMA_TEXT_INVALID",
            format!("{label} must be nonempty and no larger than {MAX_TEXT_BYTES} bytes"),
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
        assert_eq!(committed_source, source_schema());
        assert_eq!(committed_claim, claim_schema());
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
}
