use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::value_hash;
use crate::domain::artifact::{ArtifactMetadata, ArtifactRestriction};
use crate::domain::publication::PublicationOutcome;
use crate::domain::schemas::ExactVersionReference;
use crate::domain::trust::{
    CoverageTrustStatus, DefinitionTrustStatus, FidelityTrustStatus, KernelTrustStatus,
    PromotionAssessment, PromotionProfile, ReuseTrustStatus,
};
use crate::error::AppError;

pub const RELEASE_MANIFEST_SCHEMA_VERSION: &str = "release_manifest/1";
pub const RELEASE_MANIFEST_V2_SCHEMA_VERSION: &str = "release_manifest/2";
pub const MAX_RELEASE_MEMBERS: usize = 4_096;
pub const MAX_RELEASE_MEMBER_BYTES: u64 = 256 * 1_048_576;
pub const MAX_RELEASE_TOTAL_BYTES: u64 = 2 * 1_073_741_824;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseProfile {
    Private,
    Public,
}

impl ReleaseProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Public => "public",
        }
    }

    pub const fn promotion_profile(self) -> PromotionProfile {
        match self {
            Self::Private => PromotionProfile::Experimental,
            Self::Public => PromotionProfile::Publication,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseMemberKind {
    Object,
    Edge,
    Evidence,
    Artifact,
    Environment,
    License,
    Replay,
    Report,
    Export,
}

impl ReleaseMemberKind {
    pub const fn directory(self) -> &'static str {
        match self {
            Self::Object => "objects",
            Self::Edge => "edges",
            Self::Evidence => "evidence",
            Self::Artifact => "artifacts",
            Self::Environment => "environments",
            Self::License => "licenses",
            Self::Replay => "replay",
            Self::Report => "reports",
            Self::Export => "exports",
        }
    }

    pub const ALL: [Self; 9] = [
        Self::Object,
        Self::Edge,
        Self::Evidence,
        Self::Artifact,
        Self::Environment,
        Self::License,
        Self::Replay,
        Self::Report,
        Self::Export,
    ];
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleasePedagogyMode {
    Prerequisites,
    Recommended,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseMember {
    pub path: String,
    pub kind: ReleaseMemberKind,
    pub content_hash: String,
    pub byte_size: u64,
    pub license_expression: Option<String>,
    pub restriction: ArtifactRestriction,
    pub artifact_metadata: Option<ArtifactMetadata>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasePublicationBinding {
    pub ingestion_receipt_hash: String,
    pub authority_evidence_id: String,
    pub authority_evidence_hash: String,
    pub fidelity_evidence_id: String,
    pub fidelity_evidence_hash: String,
    pub fidelity_report_artifact_hash: String,
    pub stage_hash: String,
    pub report_artifact_hash: String,
    pub retained_closure_artifact_hash: String,
    pub attestation_bundle_artifact_hash: String,
    pub raw_verification_hash: String,
    pub request_hash: String,
    pub policy_hash: String,
    pub subject: ExactVersionReference,
    pub outcome: PublicationOutcome,
    pub environment_hash: String,
    pub module_artifact_hash: String,
    pub declaration_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasePedagogyBinding {
    pub mode: ReleasePedagogyMode,
    pub include_soft: bool,
    pub root: ExactVersionReference,
    pub unit_order: Vec<ExactVersionReference>,
    pub edge_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseReplayBinding {
    pub module_path: String,
    pub environment_path: String,
    pub declaration_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseTrustBinding {
    pub assessment_hash: String,
    pub profile: PromotionProfile,
    pub subject: ExactVersionReference,
    pub kernel_status: KernelTrustStatus,
    pub fidelity_status: FidelityTrustStatus,
    pub definition_status: DefinitionTrustStatus,
    pub reuse_status: ReuseTrustStatus,
    pub coverage_status: CoverageTrustStatus,
    pub eligible: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema_version: String,
    pub profile: ReleaseProfile,
    pub publication: ReleasePublicationBinding,
    pub pedagogy: ReleasePedagogyBinding,
    pub replay: ReleaseReplayBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<ReleaseTrustBinding>,
    pub members: Vec<ReleaseMember>,
}

impl ReleaseManifest {
    pub fn validate(&self) -> Result<(), AppError> {
        if !matches!(
            self.schema_version.as_str(),
            RELEASE_MANIFEST_SCHEMA_VERSION | RELEASE_MANIFEST_V2_SCHEMA_VERSION
        ) {
            return Err(release_error(
                "MCL_RELEASE_MANIFEST_INVALID",
                "release manifest uses an unsupported schema version",
                "Use a committed release_manifest/1 or release_manifest/2 contract.",
            ));
        }
        self.publication.validate()?;
        self.pedagogy.validate()?;
        self.replay.validate(&self.publication)?;
        match (self.schema_version.as_str(), &self.trust) {
            (RELEASE_MANIFEST_SCHEMA_VERSION, None) => {}
            (RELEASE_MANIFEST_V2_SCHEMA_VERSION, Some(trust)) => {
                trust.validate()?;
                if trust.subject != self.publication.subject
                    || trust.profile != self.profile.promotion_profile()
                {
                    return Err(release_error(
                        "MCL_RELEASE_TRUST_BINDING_INVALID",
                        "release trust assessment does not bind the publication subject and release profile",
                        "Recompute the promotion assessment for the exact publication formalization and release profile.",
                    ));
                }
            }
            _ => {
                return Err(release_error(
                    "MCL_RELEASE_TRUST_BINDING_INVALID",
                    "release manifest version and trust binding are inconsistent",
                    "Keep release_manifest/1 byte-compatible without trust, or use release_manifest/2 with one complete binding.",
                ));
            }
        }
        if self.members.is_empty() || self.members.len() > MAX_RELEASE_MEMBERS {
            return Err(release_error(
                "MCL_RELEASE_MANIFEST_INVALID",
                "release member count is outside the closed bound",
                "Include one to 4096 deterministic bundle members.",
            ));
        }
        let mut previous = None;
        let mut total = 0_u64;
        let mut kinds = BTreeSet::new();
        for member in &self.members {
            member.validate()?;
            if previous.is_some_and(|path: &str| path >= member.path.as_str()) {
                return Err(release_error(
                    "MCL_RELEASE_MANIFEST_INVALID",
                    "release members must be strictly sorted by unique path",
                    "Sort members lexicographically and remove duplicate paths.",
                ));
            }
            previous = Some(member.path.as_str());
            total = total.checked_add(member.byte_size).ok_or_else(|| {
                release_error(
                    "MCL_RELEASE_MANIFEST_INVALID",
                    "release member sizes overflowed their bound",
                    "Reduce the release closure.",
                )
            })?;
            kinds.insert(member.kind);
            if self.profile == ReleaseProfile::Public
                && (member.restriction != ArtifactRestriction::Public
                    || member.license_expression.is_none())
            {
                return Err(release_error(
                    "MCL_RELEASE_PUBLIC_POLICY_BLOCKED",
                    format!(
                        "public release member `{}` is restricted or has no resolved license",
                        member.path
                    ),
                    "Resolve the license and public redistribution policy or build a private release.",
                ));
            }
        }
        if total > MAX_RELEASE_TOTAL_BYTES || kinds.len() != ReleaseMemberKind::ALL.len() {
            return Err(release_error(
                "MCL_RELEASE_MANIFEST_INVALID",
                "release exceeds its total size bound or omits a required top-level member family",
                "Keep the release under 2 GiB and include objects, edges, evidence, artifacts, environments, licenses, replay, reports, and exports.",
            ));
        }
        let mut required_paths = vec![
            "reports/publication-report.json".to_owned(),
            "reports/publication-retained-closure.json".to_owned(),
            "reports/publication-stage.json".to_owned(),
            "reports/publication-receipt.json".to_owned(),
            "reports/attestation-bundle.json".to_owned(),
            "reports/raw-attestation-verification.json".to_owned(),
            "reports/canonical-attestation-receipt.json".to_owned(),
            format!(
                "reports/fidelity/{}@{}.json",
                self.publication.fidelity_evidence_id, self.publication.fidelity_evidence_hash
            ),
            "licenses/index.json".to_owned(),
            "exports/pedagogy-path.json".to_owned(),
            "replay/replay.json".to_owned(),
            self.replay.module_path.clone(),
            self.replay.environment_path.clone(),
        ];
        if self.schema_version == RELEASE_MANIFEST_V2_SCHEMA_VERSION {
            required_paths.push("reports/promotion-assessment.json".to_owned());
        }
        for path in &required_paths {
            if self
                .members
                .binary_search_by_key(&path.as_str(), |member| member.path.as_str())
                .is_err()
            {
                return Err(release_error(
                    "MCL_RELEASE_MANIFEST_INVALID",
                    format!("release omits required bound member `{path}`"),
                    "Rebuild from the exact publication receipt and reviewed pedagogy path.",
                ));
            }
        }
        Ok(())
    }

    pub fn manifest_hash(&self) -> Result<String, AppError> {
        self.validate()?;
        let value = serde_json::to_value(self).map_err(|error| {
            release_error(
                "MCL_RELEASE_MANIFEST_INVALID",
                error.to_string(),
                "Report this deterministic release serialization defect.",
            )
        })?;
        value_hash(&value)
    }
}

impl ReleaseTrustBinding {
    pub fn from_assessment(assessment: &PromotionAssessment) -> Result<Self, AppError> {
        assessment.validate()?;
        Ok(Self {
            assessment_hash: assessment.assessment_hash()?,
            profile: assessment.profile,
            subject: assessment.trust.formalization.clone(),
            kernel_status: assessment.trust.kernel.status,
            fidelity_status: assessment.trust.fidelity.status,
            definition_status: assessment.trust.definition.status,
            reuse_status: assessment.trust.reuse.status,
            coverage_status: assessment.trust.coverage.status,
            eligible: assessment.eligible,
        })
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if !is_hash(&self.assessment_hash)
            || uuid::Uuid::parse_str(&self.subject.object_id).is_err()
            || !is_hash(&self.subject.version_hash)
            || !self.eligible
        {
            return Err(release_error(
                "MCL_RELEASE_TRUST_BINDING_INVALID",
                "release trust binding is not an eligible exact promotion assessment",
                "Bind one eligible promotion_assessment/1 hash for the exact formalization and profile.",
            ));
        }
        Ok(())
    }
}

impl ReleaseMember {
    pub fn validate(&self) -> Result<(), AppError> {
        if !is_safe_relative_path(&self.path)
            || !self
                .path
                .starts_with(&format!("{}/", self.kind.directory()))
            || !is_hash(&self.content_hash)
            || self.byte_size > MAX_RELEASE_MEMBER_BYTES
            || !valid_license(self.license_expression.as_deref())
        {
            return Err(release_error(
                "MCL_RELEASE_MEMBER_INVALID",
                format!(
                    "release member `{}` is unsafe or outside its closed bounds",
                    self.path
                ),
                "Use a safe family-prefixed path, SHA-256 identity, bounded content, and reviewed license expression.",
            ));
        }
        if self.kind == ReleaseMemberKind::Artifact
            && self.path != format!("artifacts/{}", self.content_hash)
        {
            return Err(release_error(
                "MCL_RELEASE_MEMBER_INVALID",
                "artifact member path differs from its content-addressed identity",
                "Store every raw artifact at artifacts/<sha256> regardless of registration state.",
            ));
        }
        match (&self.kind, &self.artifact_metadata) {
            (ReleaseMemberKind::Artifact, Some(metadata)) => {
                metadata.validate(self.byte_size)?;
                if metadata.license_expression != self.license_expression
                    || metadata.restriction != self.restriction
                {
                    return Err(release_error(
                        "MCL_RELEASE_MEMBER_INVALID",
                        "artifact member policy or CAS path differs from its canonical metadata",
                        "Copy the exact artifact bytes and metadata without policy substitution.",
                    ));
                }
            }
            (ReleaseMemberKind::Artifact, None) => {}
            (_, Some(_)) => {
                return Err(release_error(
                    "MCL_RELEASE_MEMBER_INVALID",
                    "non-artifact release member carries artifact-only metadata",
                    "Set artifact_metadata only for raw artifact members.",
                ));
            }
            (_, None) => {}
        }
        Ok(())
    }
}

impl ReleasePublicationBinding {
    fn validate(&self) -> Result<(), AppError> {
        let hashes = [
            &self.ingestion_receipt_hash,
            &self.authority_evidence_hash,
            &self.fidelity_evidence_hash,
            &self.fidelity_report_artifact_hash,
            &self.stage_hash,
            &self.report_artifact_hash,
            &self.retained_closure_artifact_hash,
            &self.attestation_bundle_artifact_hash,
            &self.raw_verification_hash,
            &self.request_hash,
            &self.policy_hash,
            &self.environment_hash,
            &self.module_artifact_hash,
            &self.subject.version_hash,
        ];
        if hashes.into_iter().any(|hash| !is_hash(hash))
            || uuid::Uuid::parse_str(&self.authority_evidence_id).is_err()
            || uuid::Uuid::parse_str(&self.fidelity_evidence_id).is_err()
            || uuid::Uuid::parse_str(&self.subject.object_id).is_err()
            || !is_lean_name(&self.declaration_name)
        {
            return Err(release_error(
                "MCL_RELEASE_PUBLICATION_BINDING_INVALID",
                "release publication binding does not identify one exact authoritative closure",
                "Use the application-derived receipt, stage, report, closure, request, subject, environment, and module identities.",
            ));
        }
        Ok(())
    }
}

impl ReleasePedagogyBinding {
    fn validate(&self) -> Result<(), AppError> {
        if self.mode == ReleasePedagogyMode::Recommended && self.include_soft
            || self.unit_order.is_empty()
            || self.unit_order.len() > 1_000
            || !self.unit_order.contains(&self.root)
            || !unique_references(&self.unit_order)
            || self.edge_ids.len() > 4_096
            || self.edge_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .edge_ids
                .iter()
                .any(|edge_id| uuid::Uuid::parse_str(edge_id).is_err())
        {
            return Err(release_error(
                "MCL_RELEASE_PEDAGOGY_BINDING_INVALID",
                "release pedagogy binding is empty, duplicated, unsorted, or inconsistent",
                "Use the exact deterministic reviewed pedagogy path returned by the application.",
            ));
        }
        Ok(())
    }
}

impl ReleaseReplayBinding {
    fn validate(&self, publication: &ReleasePublicationBinding) -> Result<(), AppError> {
        if self.module_path != "replay/Submission.lean"
            || self.environment_path != "replay/environment.json"
            || self.declaration_name != publication.declaration_name
        {
            return Err(release_error(
                "MCL_RELEASE_REPLAY_BINDING_INVALID",
                "release replay binding differs from the verifier-controlled paths or declaration",
                "Use the fixed Submission.lean and environment.json replay inputs bound to the publication request.",
            ));
        }
        Ok(())
    }
}

pub fn release_manifest_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnehmos.ai/mathos/schemas/release/manifest/1",
        "title": "MathOS Portable Release Manifest v1",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "profile", "publication", "pedagogy", "replay", "members"],
        "properties": {
            "schema_version": {"const": RELEASE_MANIFEST_SCHEMA_VERSION},
            "profile": {"enum": ["private", "public"]},
            "publication": {"$ref": "#/$defs/publication"},
            "pedagogy": {"$ref": "#/$defs/pedagogy"},
            "replay": {"$ref": "#/$defs/replay"},
            "members": {"type": "array", "minItems": 1, "maxItems": MAX_RELEASE_MEMBERS, "items": {"$ref": "#/$defs/member"}}
        },
        "$defs": {
            "exact_ref": {"type": "object", "additionalProperties": false, "required": ["object_id", "version_hash"], "properties": {"object_id": {"type": "string", "format": "uuid"}, "version_hash": {"$ref": "#/$defs/hash"}}},
            "hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "publication": {"type": "object", "additionalProperties": false, "required": ["ingestion_receipt_hash", "authority_evidence_id", "authority_evidence_hash", "fidelity_evidence_id", "fidelity_evidence_hash", "fidelity_report_artifact_hash", "stage_hash", "report_artifact_hash", "retained_closure_artifact_hash", "attestation_bundle_artifact_hash", "raw_verification_hash", "request_hash", "policy_hash", "subject", "outcome", "environment_hash", "module_artifact_hash", "declaration_name"], "properties": {"ingestion_receipt_hash": {"$ref": "#/$defs/hash"}, "authority_evidence_id": {"type": "string", "format": "uuid"}, "authority_evidence_hash": {"$ref": "#/$defs/hash"}, "fidelity_evidence_id": {"type": "string", "format": "uuid"}, "fidelity_evidence_hash": {"$ref": "#/$defs/hash"}, "fidelity_report_artifact_hash": {"$ref": "#/$defs/hash"}, "stage_hash": {"$ref": "#/$defs/hash"}, "report_artifact_hash": {"$ref": "#/$defs/hash"}, "retained_closure_artifact_hash": {"$ref": "#/$defs/hash"}, "attestation_bundle_artifact_hash": {"$ref": "#/$defs/hash"}, "raw_verification_hash": {"$ref": "#/$defs/hash"}, "request_hash": {"$ref": "#/$defs/hash"}, "policy_hash": {"$ref": "#/$defs/hash"}, "subject": {"$ref": "#/$defs/exact_ref"}, "outcome": {"enum": ["proof", "refutation"]}, "environment_hash": {"$ref": "#/$defs/hash"}, "module_artifact_hash": {"$ref": "#/$defs/hash"}, "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256}}},
            "pedagogy": {"type": "object", "additionalProperties": false, "required": ["mode", "include_soft", "root", "unit_order", "edge_ids"], "properties": {"mode": {"enum": ["prerequisites", "recommended"]}, "include_soft": {"type": "boolean"}, "root": {"$ref": "#/$defs/exact_ref"}, "unit_order": {"type": "array", "minItems": 1, "maxItems": 1000, "items": {"$ref": "#/$defs/exact_ref"}}, "edge_ids": {"type": "array", "maxItems": 4096, "items": {"type": "string", "format": "uuid"}}}},
            "replay": {"type": "object", "additionalProperties": false, "required": ["module_path", "environment_path", "declaration_name"], "properties": {"module_path": {"const": "replay/Submission.lean"}, "environment_path": {"const": "replay/environment.json"}, "declaration_name": {"type": "string", "minLength": 1, "maxLength": 256}}},
            "member": {"type": "object", "additionalProperties": false, "required": ["path", "kind", "content_hash", "byte_size", "license_expression", "restriction", "artifact_metadata"], "properties": {"path": {"type": "string", "minLength": 1, "maxLength": 512}, "kind": {"enum": ["object", "edge", "evidence", "artifact", "environment", "license", "replay", "report", "export"]}, "content_hash": {"$ref": "#/$defs/hash"}, "byte_size": {"type": "integer", "minimum": 0, "maximum": MAX_RELEASE_MEMBER_BYTES}, "license_expression": {"type": ["string", "null"], "maxLength": 512}, "restriction": {"enum": ["public", "restricted", "private"]}, "artifact_metadata": {"oneOf": [{"$ref": "https://mnehmos.ai/mathos/schemas/artifact/metadata/1"}, {"type": "null"}]}}}
        }
    })
}

pub fn release_manifest_v2_schema() -> Value {
    let mut schema = release_manifest_schema();
    schema["$id"] = json!("https://mnehmos.ai/mathos/schemas/release/manifest/2");
    schema["title"] = json!("MathOS Portable Release Manifest v2");
    schema["properties"]["schema_version"] = json!({"const": RELEASE_MANIFEST_V2_SCHEMA_VERSION});
    let required = schema["required"]
        .as_array_mut()
        .expect("release schema required array");
    required.insert(required.len() - 1, json!("trust"));
    schema["properties"]["trust"] = json!({"$ref": "#/$defs/trust"});
    schema["$defs"]["trust"] = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["assessment_hash", "profile", "subject", "kernel_status", "fidelity_status", "definition_status", "reuse_status", "coverage_status", "eligible"],
        "properties": {
            "assessment_hash": {"$ref": "#/$defs/hash"},
            "profile": {"enum": ["experimental", "publication", "upstream"]},
            "subject": {"$ref": "#/$defs/exact_ref"},
            "kernel_status": {"enum": ["unverified", "lean_checked", "kernel_verified", "failed"]},
            "fidelity_status": {"enum": ["unreviewed", "source_mapped", "reviewer_checked", "expert_approved", "rejected"]},
            "definition_status": {"enum": ["ungrounded", "source_grounded", "project_approved", "upstream_accepted"]},
            "reuse_status": {"enum": ["experimental", "campaign_specific", "candidate", "review_ready", "upstreamed"]},
            "coverage_status": {"enum": ["unknown", "statement_only", "analytical_core", "supporting_lemma", "finite_specialization", "asymptotic_component", "full_theorem"]},
            "eligible": {"const": true}
        }
    });
    schema
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn unique_references(references: &[ExactVersionReference]) -> bool {
    let mut seen = BTreeSet::new();
    references.iter().all(|reference| {
        uuid::Uuid::parse_str(&reference.object_id).is_ok()
            && is_hash(&reference.version_hash)
            && seen.insert(reference)
    })
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

fn valid_license(value: Option<&str>) -> bool {
    value.is_none_or(|license| {
        let license = license.trim();
        !license.is_empty()
            && license.len() <= 512
            && !matches!(license.to_ascii_lowercase().as_str(), "unknown" | "none")
            && license.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'-' | b'.' | b'+' | b'(' | b')' | b' ')
            })
    })
}

fn release_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_schema_matches_closed_rust_contract() {
        let committed: Value = serde_json::from_str(include_str!(
            "../../schemas/release/release-manifest-1.schema.json"
        ))
        .expect("committed release manifest schema");
        assert_eq!(committed, release_manifest_schema());
        assert_eq!(
            value_hash(&committed).expect("release schema hash"),
            "63090d65dea509c4c3e1d4e5572d29fa688e4cc30792b3f4c94a1647f9491ae3"
        );
        let committed_v2: Value = serde_json::from_str(include_str!(
            "../../schemas/release/release-manifest-2.schema.json"
        ))
        .expect("committed release manifest v2 schema");
        assert_eq!(committed_v2, release_manifest_v2_schema());
        assert_eq!(
            value_hash(&committed_v2).expect("release v2 schema hash"),
            "3da87107817ca9bf658398aed3cfbe9db65e3878987dcc6e376db0150772e3fc"
        );
    }

    #[test]
    fn member_paths_reject_traversal_and_wrong_families() {
        let base = ReleaseMember {
            path: "reports/report.json".to_owned(),
            kind: ReleaseMemberKind::Report,
            content_hash: "a".repeat(64),
            byte_size: 1,
            license_expression: Some("MIT".to_owned()),
            restriction: ArtifactRestriction::Public,
            artifact_metadata: None,
        };
        base.validate().expect("safe member");
        for path in [
            "../report.json",
            "reports/../report.json",
            "objects/report.json",
        ] {
            let mut invalid = base.clone();
            invalid.path = path.to_owned();
            assert_eq!(
                invalid.validate().expect_err("unsafe path").code,
                "MCL_RELEASE_MEMBER_INVALID"
            );
        }

        let mut unregistered_artifact = base;
        unregistered_artifact.kind = ReleaseMemberKind::Artifact;
        unregistered_artifact.path = format!("artifacts/{}", unregistered_artifact.content_hash);
        unregistered_artifact.license_expression = None;
        unregistered_artifact.restriction = ArtifactRestriction::Private;
        unregistered_artifact
            .validate()
            .expect("unregistered artifact uses its exact CAS path");
        unregistered_artifact.path = format!("artifacts/{}", "b".repeat(64));
        assert_eq!(
            unregistered_artifact
                .validate()
                .expect_err("artifact path substitution")
                .code,
            "MCL_RELEASE_MEMBER_INVALID"
        );
    }
}
