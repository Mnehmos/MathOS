use std::collections::BTreeSet;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::schemas::ExactVersionReference;
use crate::domain::{ArtifactRestriction, ReleaseProfile};
use crate::error::AppError;

pub const CORPUS_EXPORT_MANIFEST_SCHEMA_VERSION: &str = "corpus_export_manifest/1";
pub const MAX_CORPUS_EXPORT_MEMBER_BYTES: u64 = 16 * 1_048_576;
pub const MAX_CORPUS_EXPORT_TOTAL_BYTES: u64 = 64 * 1_048_576;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusExportPolicy {
    PrivateAuditOnly,
    Quarantined,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusExportMemberKind {
    MathcorpusPacket,
    McipBundle,
    LeanModule,
    SourceReleaseManifest,
    Schema,
    License,
}

impl CorpusExportMemberKind {
    pub const fn directory(self) -> &'static str {
        match self {
            Self::MathcorpusPacket => "mathcorpus",
            Self::McipBundle => "mcip",
            Self::LeanModule => "lean",
            Self::SourceReleaseManifest => "source-release",
            Self::Schema => "schemas",
            Self::License => "licenses",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MathCorpusDomain {
    #[serde(rename = "arithmetic")]
    Arithmetic,
    #[serde(rename = "algebra")]
    Algebra,
    #[serde(rename = "number_theory")]
    NumberTheory,
    #[serde(rename = "combinatorics")]
    Combinatorics,
    #[serde(rename = "analysis")]
    Analysis,
    #[serde(rename = "real_analysis")]
    RealAnalysis,
    #[serde(rename = "geometry")]
    Geometry,
    #[serde(rename = "topology")]
    Topology,
    #[serde(rename = "logic")]
    Logic,
    #[serde(rename = "linear_algebra")]
    LinearAlgebra,
    #[serde(rename = "abstract_algebra")]
    AbstractAlgebra,
    #[serde(rename = "set_theory")]
    SetTheory,
    #[serde(rename = "probability")]
    Probability,
    #[serde(rename = "frontier")]
    Frontier,
}

impl MathCorpusDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Arithmetic => "arithmetic",
            Self::Algebra => "algebra",
            Self::NumberTheory => "number_theory",
            Self::Combinatorics => "combinatorics",
            Self::Analysis => "analysis",
            Self::RealAnalysis => "real_analysis",
            Self::Geometry => "geometry",
            Self::Topology => "topology",
            Self::Logic => "logic",
            Self::LinearAlgebra => "linear_algebra",
            Self::AbstractAlgebra => "abstract_algebra",
            Self::SetTheory => "set_theory",
            Self::Probability => "probability",
            Self::Frontier => "frontier",
        }
    }
}

impl FromStr for MathCorpusDomain {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = match value {
            "arithmetic" => Self::Arithmetic,
            "algebra" => Self::Algebra,
            "number_theory" => Self::NumberTheory,
            "combinatorics" => Self::Combinatorics,
            "analysis" => Self::Analysis,
            "real_analysis" => Self::RealAnalysis,
            "geometry" => Self::Geometry,
            "topology" => Self::Topology,
            "logic" => Self::Logic,
            "linear_algebra" => Self::LinearAlgebra,
            "abstract_algebra" => Self::AbstractAlgebra,
            "set_theory" => Self::SetTheory,
            "probability" => Self::Probability,
            "frontier" => Self::Frontier,
            _ => return Err(curation_error("domain", value)),
        };
        Ok(parsed)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MathCorpusLevel {
    #[serde(rename = "L0_elementary")]
    L0Elementary,
    #[serde(rename = "L1_proof_basics")]
    L1ProofBasics,
    #[serde(rename = "L2_olympiad")]
    L2Olympiad,
    #[serde(rename = "L3_undergrad")]
    L3Undergrad,
    #[serde(rename = "L4_advanced_undergrad")]
    L4AdvancedUndergrad,
    #[serde(rename = "L5_grad")]
    L5Grad,
    #[serde(rename = "L6_known_theorem")]
    L6KnownTheorem,
    #[serde(rename = "L7_frontier")]
    L7Frontier,
}

impl MathCorpusLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::L0Elementary => "L0_elementary",
            Self::L1ProofBasics => "L1_proof_basics",
            Self::L2Olympiad => "L2_olympiad",
            Self::L3Undergrad => "L3_undergrad",
            Self::L4AdvancedUndergrad => "L4_advanced_undergrad",
            Self::L5Grad => "L5_grad",
            Self::L6KnownTheorem => "L6_known_theorem",
            Self::L7Frontier => "L7_frontier",
        }
    }
}

impl FromStr for MathCorpusLevel {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = match value {
            "L0_elementary" => Self::L0Elementary,
            "L1_proof_basics" => Self::L1ProofBasics,
            "L2_olympiad" => Self::L2Olympiad,
            "L3_undergrad" => Self::L3Undergrad,
            "L4_advanced_undergrad" => Self::L4AdvancedUndergrad,
            "L5_grad" => Self::L5Grad,
            "L6_known_theorem" => Self::L6KnownTheorem,
            "L7_frontier" => Self::L7Frontier,
            _ => return Err(curation_error("level", value)),
        };
        Ok(parsed)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MathCorpusDifficultyBin {
    D0,
    D1,
    D2,
    D3,
    D4,
    D5,
}

impl MathCorpusDifficultyBin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::D0 => "D0",
            Self::D1 => "D1",
            Self::D2 => "D2",
            Self::D3 => "D3",
            Self::D4 => "D4",
            Self::D5 => "D5",
        }
    }
}

impl FromStr for MathCorpusDifficultyBin {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = match value {
            "D0" => Self::D0,
            "D1" => Self::D1,
            "D2" => Self::D2,
            "D3" => Self::D3,
            "D4" => Self::D4,
            "D5" => Self::D5,
            _ => return Err(curation_error("difficulty bin", value)),
        };
        Ok(parsed)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusExportSourceBinding {
    pub release_manifest_hash: String,
    pub release_profile: ReleaseProfile,
    pub publication_receipt_hash: String,
    pub authority_evidence_id: String,
    pub authority_evidence_hash: String,
    pub fidelity_evidence_id: String,
    pub fidelity_evidence_hash: String,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
    pub source: ExactVersionReference,
    pub claim: ExactVersionReference,
    pub formalization: ExactVersionReference,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusExportCuration {
    pub packet_id: String,
    pub domain: MathCorpusDomain,
    pub level: MathCorpusLevel,
    pub difficulty_bin: MathCorpusDifficultyBin,
    pub policy: CorpusExportPolicy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusExportUpstreamBinding {
    pub repository: String,
    pub commit_sha: String,
    pub tree_sha: String,
    pub license_expression: String,
    pub packet_schema_sha256: String,
    pub mcip_defs_schema_sha256: String,
    pub mcip_bundle_schema_sha256: String,
    pub mcip_packet_identity_schema_sha256: String,
    pub mcip_proof_variant_schema_sha256: String,
    pub mcip_dependency_manifest_schema_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusExportOutputBinding {
    pub packet_path: String,
    pub packet_sha256: String,
    pub mcip_bundle_path: String,
    pub mcip_bundle_sha256: String,
    pub module_path: String,
    pub module_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusExportMember {
    pub path: String,
    pub kind: CorpusExportMemberKind,
    pub content_hash: String,
    pub byte_size: u64,
    pub license_expression: Option<String>,
    pub restriction: ArtifactRestriction,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusExportManifest {
    pub schema_version: String,
    pub source_release: CorpusExportSourceBinding,
    pub curation: CorpusExportCuration,
    pub upstream: CorpusExportUpstreamBinding,
    pub outputs: CorpusExportOutputBinding,
    pub members: Vec<CorpusExportMember>,
}

impl CorpusExportManifest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != CORPUS_EXPORT_MANIFEST_SCHEMA_VERSION {
            return Err(manifest_error(
                "unsupported corpus export manifest schema version",
            ));
        }
        self.source_release.validate()?;
        self.curation
            .validate(self.source_release.release_profile)?;
        self.upstream.validate()?;
        self.outputs.validate()?;
        if self.members.len() != required_member_paths().len() {
            return Err(manifest_error(
                "corpus export manifest has the wrong closed member count",
            ));
        }
        let mut previous = None;
        let mut observed_paths = BTreeSet::new();
        let mut total = 0_u64;
        for member in &self.members {
            member.validate()?;
            if previous.is_some_and(|path: &str| path >= member.path.as_str()) {
                return Err(manifest_error(
                    "corpus export members are not strictly path-sorted",
                ));
            }
            previous = Some(member.path.as_str());
            observed_paths.insert(member.path.as_str());
            total = total.checked_add(member.byte_size).ok_or_else(|| {
                manifest_error("corpus export member sizes overflow their closed bound")
            })?;
        }
        if total > MAX_CORPUS_EXPORT_TOTAL_BYTES
            || observed_paths != required_member_paths().into_iter().collect()
        {
            return Err(manifest_error(
                "corpus export inventory differs from its closed v1 member set",
            ));
        }
        let mcip_member = self.member(&self.outputs.mcip_bundle_path)?;
        if mcip_member.content_hash != self.outputs.mcip_bundle_sha256 {
            return Err(manifest_error(
                "MCIP output binding differs from its member identity",
            ));
        }
        let module_member = self.member(&self.outputs.module_path)?;
        if module_member.content_hash != self.outputs.module_sha256 {
            return Err(manifest_error(
                "normalized Lean module binding differs from its member identity",
            ));
        }
        let sensitive_restriction = match self.curation.policy {
            CorpusExportPolicy::PrivateAuditOnly => ArtifactRestriction::Private,
            CorpusExportPolicy::Quarantined => ArtifactRestriction::Public,
        };
        for path in [
            "mathcorpus/packet.json",
            "mcip/bundle.json",
            "lean/Submission.lean",
            "source-release/manifest.json",
        ] {
            if self.member(path)?.restriction != sensitive_restriction {
                return Err(manifest_error(
                    "corpus export policy and member restriction disagree",
                ));
            }
        }
        for path in required_member_paths()
            .into_iter()
            .filter(|path| path.starts_with("schemas/") || path.starts_with("licenses/"))
        {
            let member = self.member(path)?;
            if member.restriction != ArtifactRestriction::Public
                || member.license_expression.as_deref() != Some("Apache-2.0")
            {
                return Err(manifest_error(
                    "vendored schemas and license must remain public Apache-2.0",
                ));
            }
        }
        Ok(())
    }

    fn member(&self, path: &str) -> Result<&CorpusExportMember, AppError> {
        self.members
            .iter()
            .find(|member| member.path == path)
            .ok_or_else(|| {
                manifest_error(format!("required corpus export member `{path}` is absent"))
            })
    }
}

impl CorpusExportSourceBinding {
    fn validate(&self) -> Result<(), AppError> {
        let hashes = [
            &self.release_manifest_hash,
            &self.publication_receipt_hash,
            &self.authority_evidence_hash,
            &self.fidelity_evidence_hash,
            &self.environment_hash,
            &self.module_artifact_hash,
            &self.source.version_hash,
            &self.claim.version_hash,
            &self.formalization.version_hash,
        ];
        if hashes.into_iter().any(|hash| !is_hash(hash))
            || [
                &self.authority_evidence_id,
                &self.fidelity_evidence_id,
                &self.source.object_id,
                &self.claim.object_id,
                &self.formalization.object_id,
            ]
            .into_iter()
            .any(|id| uuid::Uuid::parse_str(id).is_err())
            || !is_lean_name(&self.declaration_name)
        {
            return Err(manifest_error(
                "source release binding is not one exact closed identity",
            ));
        }
        Ok(())
    }
}

impl CorpusExportCuration {
    fn validate(&self, profile: ReleaseProfile) -> Result<(), AppError> {
        if !is_packet_id(&self.packet_id)
            || matches!(
                (profile, self.policy),
                (ReleaseProfile::Private, CorpusExportPolicy::Quarantined)
                    | (ReleaseProfile::Public, CorpusExportPolicy::PrivateAuditOnly)
            )
        {
            return Err(manifest_error(
                "curation identity or fail-closed release policy is invalid",
            ));
        }
        Ok(())
    }
}

impl CorpusExportUpstreamBinding {
    fn validate(&self) -> Result<(), AppError> {
        let hashes = [
            &self.packet_schema_sha256,
            &self.mcip_defs_schema_sha256,
            &self.mcip_bundle_schema_sha256,
            &self.mcip_packet_identity_schema_sha256,
            &self.mcip_proof_variant_schema_sha256,
            &self.mcip_dependency_manifest_schema_sha256,
        ];
        if self.repository != "Mnehmos/mathcorpus"
            || !is_git_sha(&self.commit_sha)
            || !is_git_sha(&self.tree_sha)
            || self.license_expression != "Apache-2.0"
            || hashes.into_iter().any(|hash| !is_hash(hash))
        {
            return Err(manifest_error(
                "upstream schema provenance is incomplete or invalid",
            ));
        }
        Ok(())
    }
}

impl CorpusExportOutputBinding {
    fn validate(&self) -> Result<(), AppError> {
        if self.packet_path != "mathcorpus/packet.json"
            || self.mcip_bundle_path != "mcip/bundle.json"
            || self.module_path != "lean/Submission.lean"
            || !is_hash(&self.packet_sha256)
            || !is_hash(&self.mcip_bundle_sha256)
            || !is_hash(&self.module_sha256)
        {
            return Err(manifest_error(
                "corpus output bindings are not the closed v1 identities",
            ));
        }
        Ok(())
    }
}

impl CorpusExportMember {
    fn validate(&self) -> Result<(), AppError> {
        if !is_safe_relative_path(&self.path)
            || !self
                .path
                .starts_with(&format!("{}/", self.kind.directory()))
            || !is_hash(&self.content_hash)
            || self.byte_size > MAX_CORPUS_EXPORT_MEMBER_BYTES
            || self.license_expression.as_ref().is_some_and(|license| {
                license.trim().is_empty() || license.len() > 512 || license.contains('\0')
            })
        {
            return Err(manifest_error(format!(
                "corpus export member `{}` is unsafe or outside its closed bounds",
                self.path
            )));
        }
        Ok(())
    }
}

pub fn required_member_paths() -> [&'static str; 11] {
    [
        "lean/Submission.lean",
        "licenses/mathcorpus-apache-2.0.txt",
        "mathcorpus/packet.json",
        "mcip/bundle.json",
        "schemas/mathcorpus/packet.schema.json",
        "schemas/mcip/v1/_defs.schema.json",
        "schemas/mcip/v1/bundle.schema.json",
        "schemas/mcip/v1/dependency_manifest.schema.json",
        "schemas/mcip/v1/packet_identity.schema.json",
        "schemas/mcip/v1/proof_variant.schema.json",
        "source-release/manifest.json",
    ]
}

pub fn corpus_export_manifest_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/release/corpus-export-manifest/1",
        "title": "MathOS MathCorpus and MCIP Export Manifest v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "source_release", "curation", "upstream", "outputs", "members"],
        "properties": {
            "schema_version": {"const": CORPUS_EXPORT_MANIFEST_SCHEMA_VERSION},
            "source_release": {"$ref": "#/$defs/source_release"},
            "curation": {"$ref": "#/$defs/curation"},
            "upstream": {"$ref": "#/$defs/upstream"},
            "outputs": {"$ref": "#/$defs/outputs"},
            "members": {"type": "array", "minItems": 11, "maxItems": 11, "items": {"$ref": "#/$defs/member"}}
        },
        "$defs": {
            "hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "git_sha": {"type": "string", "pattern": "^[0-9a-f]{40}$"},
            "exact_ref": {
                "type": "object",
                "additionalProperties": false,
                "required": ["object_id", "version_hash"],
                "properties": {
                    "object_id": {"type": "string", "format": "uuid"},
                    "version_hash": {"$ref": "#/$defs/hash"}
                }
            },
            "source_release": {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "release_manifest_hash", "release_profile", "publication_receipt_hash",
                    "authority_evidence_id", "authority_evidence_hash", "fidelity_evidence_id",
                    "fidelity_evidence_hash", "environment_hash", "module_artifact_hash",
                    "declaration_name", "source", "claim", "formalization"
                ],
                "properties": {
                    "release_manifest_hash": {"$ref": "#/$defs/hash"},
                    "release_profile": {"enum": ["private", "public"]},
                    "publication_receipt_hash": {"$ref": "#/$defs/hash"},
                    "authority_evidence_id": {"type": "string", "format": "uuid"},
                    "authority_evidence_hash": {"$ref": "#/$defs/hash"},
                    "fidelity_evidence_id": {"type": "string", "format": "uuid"},
                    "fidelity_evidence_hash": {"$ref": "#/$defs/hash"},
                    "environment_hash": {"$ref": "#/$defs/hash"},
                    "module_artifact_hash": {"$ref": "#/$defs/hash"},
                    "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256},
                    "source": {"$ref": "#/$defs/exact_ref"},
                    "claim": {"$ref": "#/$defs/exact_ref"},
                    "formalization": {"$ref": "#/$defs/exact_ref"}
                }
            },
            "curation": {
                "type": "object",
                "additionalProperties": false,
                "required": ["packet_id", "domain", "level", "difficulty_bin", "policy"],
                "properties": {
                    "packet_id": {"type": "string", "pattern": "^[a-z0-9]+([._][a-z0-9]+)*\\.v[0-9]+$"},
                    "domain": {"enum": [
                        "arithmetic", "algebra", "number_theory", "combinatorics", "analysis",
                        "real_analysis", "geometry", "topology", "logic", "linear_algebra",
                        "abstract_algebra", "set_theory", "probability", "frontier"
                    ]},
                    "level": {"enum": [
                        "L0_elementary", "L1_proof_basics", "L2_olympiad", "L3_undergrad",
                        "L4_advanced_undergrad", "L5_grad", "L6_known_theorem", "L7_frontier"
                    ]},
                    "difficulty_bin": {"enum": ["D0", "D1", "D2", "D3", "D4", "D5"]},
                    "policy": {"enum": ["private_audit_only", "quarantined"]}
                }
            },
            "upstream": {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "repository", "commit_sha", "tree_sha", "license_expression",
                    "packet_schema_sha256", "mcip_defs_schema_sha256", "mcip_bundle_schema_sha256",
                    "mcip_packet_identity_schema_sha256", "mcip_proof_variant_schema_sha256",
                    "mcip_dependency_manifest_schema_sha256"
                ],
                "properties": {
                    "repository": {"const": "Mnehmos/mathcorpus"},
                    "commit_sha": {"$ref": "#/$defs/git_sha"},
                    "tree_sha": {"$ref": "#/$defs/git_sha"},
                    "license_expression": {"const": "Apache-2.0"},
                    "packet_schema_sha256": {"$ref": "#/$defs/hash"},
                    "mcip_defs_schema_sha256": {"$ref": "#/$defs/hash"},
                    "mcip_bundle_schema_sha256": {"$ref": "#/$defs/hash"},
                    "mcip_packet_identity_schema_sha256": {"$ref": "#/$defs/hash"},
                    "mcip_proof_variant_schema_sha256": {"$ref": "#/$defs/hash"},
                    "mcip_dependency_manifest_schema_sha256": {"$ref": "#/$defs/hash"}
                }
            },
            "outputs": {
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "packet_path", "packet_sha256", "mcip_bundle_path", "mcip_bundle_sha256",
                    "module_path", "module_sha256"
                ],
                "properties": {
                    "packet_path": {"const": "mathcorpus/packet.json"},
                    "packet_sha256": {"$ref": "#/$defs/hash"},
                    "mcip_bundle_path": {"const": "mcip/bundle.json"},
                    "mcip_bundle_sha256": {"$ref": "#/$defs/hash"},
                    "module_path": {"const": "lean/Submission.lean"},
                    "module_sha256": {"$ref": "#/$defs/hash"}
                }
            },
            "member": {
                "type": "object",
                "additionalProperties": false,
                "required": ["path", "kind", "content_hash", "byte_size", "license_expression", "restriction"],
                "properties": {
                    "path": {"type": "string", "minLength": 1, "maxLength": 512},
                    "kind": {"enum": [
                        "mathcorpus_packet", "mcip_bundle", "lean_module",
                        "source_release_manifest", "schema", "license"
                    ]},
                    "content_hash": {"$ref": "#/$defs/hash"},
                    "byte_size": {"type": "integer", "minimum": 0, "maximum": MAX_CORPUS_EXPORT_MEMBER_BYTES},
                    "license_expression": {"type": ["string", "null"], "maxLength": 512},
                    "restriction": {"enum": ["public", "restricted", "private"]}
                }
            }
        }
    })
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_git_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_packet_id(value: &str) -> bool {
    let Some((prefix, version)) = value.rsplit_once(".v") else {
        return false;
    };
    !prefix.is_empty()
        && version.bytes().all(|byte| byte.is_ascii_digit())
        && !version.is_empty()
        && prefix.split(['.', '_']).all(|component| {
            !component.is_empty()
                && component
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn is_lean_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\''))
        })
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn curation_error(field: &str, value: &str) -> AppError {
    AppError::new(
        "MCL_CORPUS_EXPORT_CURATION_INVALID",
        format!("unsupported MathCorpus {field} `{value}`"),
        false,
        "Use an exact value declared by the pinned MathCorpus packet schema.",
    )
}

fn manifest_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_CORPUS_EXPORT_MANIFEST_INVALID",
        message,
        false,
        "Quarantine the export and rebuild it from the exact frozen release and pinned schemas.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curation_enums_round_trip_the_upstream_spelling() {
        for value in [
            "arithmetic",
            "number_theory",
            "real_analysis",
            "abstract_algebra",
            "frontier",
        ] {
            let parsed = MathCorpusDomain::from_str(value).expect("known domain");
            assert_eq!(parsed.as_str(), value);
            assert_eq!(serde_json::to_value(parsed).expect("domain JSON"), value);
        }
        for value in ["L0_elementary", "L1_proof_basics", "L7_frontier"] {
            let parsed = MathCorpusLevel::from_str(value).expect("known level");
            assert_eq!(parsed.as_str(), value);
            assert_eq!(serde_json::to_value(parsed).expect("level JSON"), value);
        }
        for value in ["D0", "D3", "D5"] {
            let parsed = MathCorpusDifficultyBin::from_str(value).expect("known bin");
            assert_eq!(parsed.as_str(), value);
            assert_eq!(serde_json::to_value(parsed).expect("bin JSON"), value);
        }
    }

    #[test]
    fn packet_identity_rejects_case_and_empty_components() {
        assert!(is_packet_id("mathos.number_theory.pilot_a_repair.v1"));
        for invalid in [
            "MathOS.number_theory.item.v1",
            "mathos..item.v1",
            "mathos.item",
            "mathos.item.v",
        ] {
            assert!(!is_packet_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn committed_schema_matches_the_closed_rust_contract() {
        let committed: Value = serde_json::from_str(include_str!(
            "../../schemas/release/corpus-export-manifest-1.schema.json"
        ))
        .expect("committed corpus export manifest schema");
        assert_eq!(committed, corpus_export_manifest_schema());
        assert_eq!(
            crate::canonical::value_hash(&committed).expect("corpus export schema hash"),
            "bfb5bf991a215289c440009ba7c80e130bfb9540fec0965bd728460843cdf194"
        );
    }
}
