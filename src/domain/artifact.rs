use std::collections::BTreeMap;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::AppError;

pub const ARTIFACT_METADATA_SCHEMA_VERSION: &str = "artifact_metadata/1";
pub const MAX_ARTIFACT_BYTES: u64 = 256 * 1_048_576;
pub const MAX_LEAN_SOURCE_BYTES: u64 = 1_048_576;
const MAX_METADATA_ENTRIES: usize = 64;
const MAX_METADATA_VALUE_BYTES: usize = 1_024;
const MAX_LICENSE_BYTES: usize = 512;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactMetadata {
    pub schema_version: String,
    pub media_type: ArtifactMediaType,
    pub creation_source: ArtifactCreationSource,
    pub license_expression: Option<String>,
    pub restriction: ArtifactRestriction,
    pub semantic_metadata: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ArtifactMediaType {
    #[serde(rename = "text/x-lean")]
    LeanSource,
    #[serde(rename = "application/json")]
    Json,
    #[serde(rename = "text/plain")]
    PlainText,
    #[serde(rename = "application/x-lrat")]
    Lrat,
    #[serde(rename = "application/octet-stream")]
    OctetStream,
}

impl ArtifactMediaType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LeanSource => "text/x-lean",
            Self::Json => "application/json",
            Self::PlainText => "text/plain",
            Self::Lrat => "application/x-lrat",
            Self::OctetStream => "application/octet-stream",
        }
    }
}

impl FromStr for ArtifactMediaType {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "text/x-lean" => Ok(Self::LeanSource),
            "application/json" => Ok(Self::Json),
            "text/plain" => Ok(Self::PlainText),
            "application/x-lrat" => Ok(Self::Lrat),
            "application/octet-stream" => Ok(Self::OctetStream),
            _ => Err(artifact_error(
                format!("unsupported artifact media type `{value}`"),
                "Use a media type declared by artifact_metadata/1.",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCreationSource {
    UserIngest,
    Generated,
    Verifier,
    Import,
    Migration,
}

impl ArtifactCreationSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserIngest => "user_ingest",
            Self::Generated => "generated",
            Self::Verifier => "verifier",
            Self::Import => "import",
            Self::Migration => "migration",
        }
    }
}

impl FromStr for ArtifactCreationSource {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "user_ingest" => Ok(Self::UserIngest),
            "generated" => Ok(Self::Generated),
            "verifier" => Ok(Self::Verifier),
            "import" => Ok(Self::Import),
            "migration" => Ok(Self::Migration),
            _ => Err(artifact_error(
                format!("unsupported artifact creation source `{value}`"),
                "Use a creation source declared by artifact_metadata/1.",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRestriction {
    Public,
    Restricted,
    Private,
}

impl ArtifactRestriction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Restricted => "restricted",
            Self::Private => "private",
        }
    }
}

impl FromStr for ArtifactRestriction {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "public" => Ok(Self::Public),
            "restricted" => Ok(Self::Restricted),
            "private" => Ok(Self::Private),
            _ => Err(artifact_error(
                format!("unsupported artifact restriction `{value}`"),
                "Use a restriction declared by artifact_metadata/1.",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactSnapshot {
    pub artifact_hash: String,
    pub media_type: ArtifactMediaType,
    pub byte_size: u64,
    pub creation_source: ArtifactCreationSource,
    pub license_expression: Option<String>,
    pub restriction: ArtifactRestriction,
    pub semantic_metadata: BTreeMap<String, String>,
    pub created_at: i64,
    pub created_by: String,
}

impl ArtifactMetadata {
    pub fn validate(&self, byte_size: u64) -> Result<(), AppError> {
        if self.schema_version != ARTIFACT_METADATA_SCHEMA_VERSION {
            return Err(artifact_error(
                format!(
                    "artifact metadata schema must be `{ARTIFACT_METADATA_SCHEMA_VERSION}`, received `{}`",
                    self.schema_version
                ),
                "Use the committed artifact metadata schema.",
            ));
        }
        let size_limit = if self.media_type == ArtifactMediaType::LeanSource {
            MAX_LEAN_SOURCE_BYTES
        } else {
            MAX_ARTIFACT_BYTES
        };
        if byte_size == 0 || byte_size > size_limit {
            return Err(artifact_error(
                format!("artifact byte size {byte_size} is outside the reviewed bound"),
                format!("Supply a nonempty artifact no larger than {size_limit} bytes."),
            ));
        }
        if let Some(license) = &self.license_expression {
            let trimmed = license.trim();
            let safe = !trimmed.is_empty()
                && trimmed.len() <= MAX_LICENSE_BYTES
                && trimmed.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'-' | b'.' | b'+' | b'(' | b')' | b' ')
                })
                && !matches!(trimmed.to_ascii_lowercase().as_str(), "unknown" | "none");
            if !safe {
                return Err(artifact_error(
                    "artifact license expression is unresolved or malformed",
                    "Supply a concise reviewed SPDX expression or leave it null for restricted/private review.",
                ));
            }
        }
        if self.restriction == ArtifactRestriction::Public && self.license_expression.is_none() {
            return Err(artifact_error(
                "public artifact metadata requires a resolved license expression",
                "Supply the reviewed license or mark the artifact restricted/private.",
            ));
        }
        if self.semantic_metadata.len() > MAX_METADATA_ENTRIES {
            return Err(artifact_error(
                "artifact semantic metadata exceeds its bounded entry count",
                "Keep only stable, relevant semantic metadata.",
            ));
        }
        for (key, value) in &self.semantic_metadata {
            let key_valid = !key.is_empty()
                && key.len() <= 64
                && key.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-' | b'.')
                });
            if !key_valid || value.len() > MAX_METADATA_VALUE_BYTES {
                return Err(artifact_error(
                    format!("unsafe or oversized artifact metadata entry `{key}`"),
                    "Use a short lowercase metadata key and a value no longer than 1024 bytes.",
                ));
            }
            if matches!(
                key.as_str(),
                "proved" | "disproved" | "faithful" | "certified" | "authoritative"
            ) {
                return Err(artifact_error(
                    format!("artifact metadata cannot embed authority field `{key}`"),
                    "Record mathematical authority only through typed verifier evidence.",
                ));
            }
        }
        Ok(())
    }

    pub fn validate_bytes(&self, bytes: &[u8]) -> Result<(), AppError> {
        self.validate(bytes.len() as u64)?;
        match self.media_type {
            ArtifactMediaType::LeanSource | ArtifactMediaType::PlainText => {
                std::str::from_utf8(bytes).map_err(|error| {
                    artifact_error(
                        format!("text artifact is not valid UTF-8: {error}"),
                        "Supply UTF-8 source text without binary encoding.",
                    )
                })?;
            }
            ArtifactMediaType::Json => {
                serde_json::from_slice::<Value>(bytes).map_err(|error| {
                    artifact_error(
                        format!("JSON artifact is invalid: {error}"),
                        "Supply one complete UTF-8 JSON value.",
                    )
                })?;
            }
            ArtifactMediaType::Lrat | ArtifactMediaType::OctetStream => {}
        }
        Ok(())
    }
}

pub fn artifact_metadata_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/artifact/metadata/1",
        "title": "MathOS Artifact Metadata v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "media_type", "creation_source", "license_expression", "restriction", "semantic_metadata"],
        "properties": {
            "schema_version": {"const": ARTIFACT_METADATA_SCHEMA_VERSION},
            "media_type": {"enum": ["text/x-lean", "application/json", "text/plain", "application/x-lrat", "application/octet-stream"]},
            "creation_source": {"enum": ["user_ingest", "generated", "verifier", "import", "migration"]},
            "license_expression": {"type": ["string", "null"], "minLength": 1, "maxLength": MAX_LICENSE_BYTES},
            "restriction": {"enum": ["public", "restricted", "private"]},
            "semantic_metadata": {"type": "object", "maxProperties": MAX_METADATA_ENTRIES, "additionalProperties": {"type": "string", "maxLength": MAX_METADATA_VALUE_BYTES}}
        }
    })
}

fn artifact_error(message: impl Into<String>, action: impl Into<String>) -> AppError {
    AppError::new("MCL_ARTIFACT_METADATA_INVALID", message, false, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lean_metadata() -> ArtifactMetadata {
        ArtifactMetadata {
            schema_version: ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: ArtifactMediaType::LeanSource,
            creation_source: ArtifactCreationSource::UserIngest,
            license_expression: Some("PolyForm-Noncommercial-1.0.0".to_owned()),
            restriction: ArtifactRestriction::Restricted,
            semantic_metadata: BTreeMap::from([(
                "declaration_name".to_owned(),
                "MathOS.Fixture.truth".to_owned(),
            )]),
        }
    }

    #[test]
    fn lean_source_metadata_is_closed_bounded_and_non_authoritative() {
        let metadata = lean_metadata();
        metadata
            .validate_bytes(b"theorem truth : True := by trivial\n")
            .expect("valid Lean source metadata");

        let mut authority = metadata.clone();
        authority
            .semantic_metadata
            .insert("proved".to_owned(), "true".to_owned());
        assert_eq!(
            authority
                .validate(32)
                .expect_err("authority field rejected")
                .code,
            "MCL_ARTIFACT_METADATA_INVALID"
        );

        let mut public_unknown = metadata;
        public_unknown.restriction = ArtifactRestriction::Public;
        public_unknown.license_expression = None;
        assert_eq!(
            public_unknown
                .validate(32)
                .expect_err("public unknown license")
                .code,
            "MCL_ARTIFACT_METADATA_INVALID"
        );
    }

    #[test]
    fn media_type_content_and_size_are_checked_before_ingest() {
        let metadata = lean_metadata();
        assert_eq!(
            metadata
                .validate_bytes(&[0xff, 0xfe])
                .expect_err("invalid UTF-8")
                .code,
            "MCL_ARTIFACT_METADATA_INVALID"
        );
        assert_eq!(
            metadata.validate(0).expect_err("empty artifact").code,
            "MCL_ARTIFACT_METADATA_INVALID"
        );
        assert_eq!(
            metadata
                .validate(MAX_LEAN_SOURCE_BYTES + 1)
                .expect_err("oversized Lean artifact")
                .code,
            "MCL_ARTIFACT_METADATA_INVALID"
        );

        let mut json_metadata = metadata;
        json_metadata.media_type = ArtifactMediaType::Json;
        assert_eq!(
            json_metadata
                .validate_bytes(b"{not-json}")
                .expect_err("invalid JSON")
                .code,
            "MCL_ARTIFACT_METADATA_INVALID"
        );
    }

    #[test]
    fn committed_schema_matches_the_closed_rust_contract() {
        let committed: Value = serde_json::from_str(include_str!(
            "../../schemas/artifact/artifact-metadata-1.schema.json"
        ))
        .expect("committed artifact metadata schema");
        assert_eq!(committed, artifact_metadata_schema());

        let mut unknown = serde_json::to_value(lean_metadata()).expect("metadata JSON");
        unknown["proof_status"] = json!("proved");
        assert!(serde_json::from_value::<ArtifactMetadata>(unknown).is_err());
    }
}
