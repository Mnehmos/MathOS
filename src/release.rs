use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::canonical::{canonical_json, record_version_hash};
use crate::domain::schemas::{
    ClaimPayload, ConceptPayload, ExactVersionReference, FormalizationPayload, LearningUnitPayload,
    LearningUnitReviewState, LearningUnitTrainingStatus, RedactionClass, RedistributionStatus,
    SourcePayload, validate_record_payload,
};
use crate::domain::{
    ArtifactMetadata, ArtifactRestriction, EdgeKind, EdgeSnapshot, EnvironmentSnapshot,
    EvidenceSnapshot, PublicationIngestionReceiptSnapshot, PublicationRetainedClosure,
    PublicationStageSnapshot, RecordKind, RecordSnapshot, ReleaseManifest, ReleaseMember,
    ReleaseMemberKind, ReleaseProfile, ReleaseReplayBinding,
};
use crate::error::AppError;

pub const GENERATED_RELEASE_LICENSE: &str = "PolyForm-Noncommercial-1.0.0";
const MAX_MANIFEST_BYTES: usize = 4 * 1_048_576;
const MAX_TREE_ENTRIES: usize = 8_193;

#[derive(Clone, Debug)]
pub(crate) struct ReleaseFile {
    pub bytes: Vec<u8>,
    pub kind: ReleaseMemberKind,
    pub license_expression: Option<String>,
    pub restriction: ArtifactRestriction,
    pub artifact_metadata: Option<ArtifactMetadata>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseLicenseEntry {
    pub path: String,
    pub license_expression: Option<String>,
    pub restriction: ArtifactRestriction,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseLicenseIndex {
    pub schema_version: String,
    pub entries: Vec<ReleaseLicenseEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleasePedagogyLinkPayload {
    rationale: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReleaseBuildOutcome {
    pub dry_run: bool,
    pub manifest_hash: String,
    pub bundle_path: PathBuf,
    pub profile: ReleaseProfile,
    pub member_count: usize,
    pub total_member_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReleaseVerificationReport {
    pub manifest_hash: String,
    pub profile: ReleaseProfile,
    pub member_count: usize,
    pub total_member_bytes: u64,
    pub database_independent: bool,
    pub inventory_verified: bool,
    pub hashes_verified: bool,
    pub schemas_verified: bool,
    pub references_verified: bool,
    pub replay_succeeded: bool,
    pub observed_lean_toolchain: String,
    pub replay_duration_milliseconds: u64,
}

#[derive(Debug)]
pub(crate) struct ReleaseIntegrity {
    pub manifest: ReleaseManifest,
    pub manifest_hash: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

impl ReleaseFile {
    pub(crate) fn canonical<T: Serialize>(
        value: &T,
        kind: ReleaseMemberKind,
        license_expression: Option<String>,
        restriction: ArtifactRestriction,
    ) -> Result<Self, AppError> {
        let value = serde_json::to_value(value).map_err(|error| {
            release_error(
                "MCL_RELEASE_SERIALIZATION_FAILED",
                error.to_string(),
                "Report this deterministic release serialization defect.",
            )
        })?;
        Ok(Self {
            bytes: canonical_json(&value)?,
            kind,
            license_expression,
            restriction,
            artifact_metadata: None,
        })
    }

    pub(crate) fn raw_artifact(bytes: Vec<u8>, metadata: ArtifactMetadata) -> Self {
        Self {
            bytes,
            kind: ReleaseMemberKind::Artifact,
            license_expression: metadata.license_expression.clone(),
            restriction: metadata.restriction,
            artifact_metadata: Some(metadata),
        }
    }

    pub(crate) fn unregistered_artifact(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            kind: ReleaseMemberKind::Artifact,
            license_expression: None,
            restriction: ArtifactRestriction::Private,
            artifact_metadata: None,
        }
    }

    pub(crate) fn member(&self, path: String) -> ReleaseMember {
        ReleaseMember {
            path,
            kind: self.kind,
            content_hash: format!("{:x}", Sha256::digest(&self.bytes)),
            byte_size: self.bytes.len() as u64,
            license_expression: self.license_expression.clone(),
            restriction: self.restriction,
            artifact_metadata: self.artifact_metadata.clone(),
        }
    }
}

pub(crate) fn write_release_bundle(
    output_dir: &Path,
    manifest: &ReleaseManifest,
    files: &BTreeMap<String, ReleaseFile>,
    dry_run: bool,
) -> Result<ReleaseBuildOutcome, AppError> {
    manifest.validate()?;
    let expected = manifest
        .members
        .iter()
        .map(|member| member.path.clone())
        .collect::<BTreeSet<_>>();
    if expected != files.keys().cloned().collect::<BTreeSet<_>>() {
        return Err(release_error(
            "MCL_RELEASE_BUILD_INVENTORY_MISMATCH",
            "release files differ from the canonical manifest inventory",
            "Rebuild the manifest and files from the same immutable closure.",
        ));
    }
    for member in &manifest.members {
        let file = files.get(&member.path).expect("inventory equality");
        if file.member(member.path.clone()) != *member {
            return Err(release_error(
                "MCL_RELEASE_BUILD_MEMBER_MISMATCH",
                format!(
                    "release member `{}` changed before materialization",
                    member.path
                ),
                "Rebuild from immutable canonical inputs.",
            ));
        }
    }
    let manifest_bytes = canonical_json(&serde_json::to_value(manifest).map_err(|error| {
        release_error(
            "MCL_RELEASE_SERIALIZATION_FAILED",
            error.to_string(),
            "Report this deterministic release manifest serialization defect.",
        )
    })?)?;
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err(release_error(
            "MCL_RELEASE_MANIFEST_TOO_LARGE",
            "release manifest exceeds its 4 MiB verification bound",
            "Reduce the release member count or metadata before building.",
        ));
    }
    let (parent, destination) = resolve_new_output(output_dir)?;
    let manifest_hash = format!("{:x}", Sha256::digest(&manifest_bytes));
    let outcome = ReleaseBuildOutcome {
        dry_run,
        manifest_hash,
        bundle_path: destination.clone(),
        profile: manifest.profile,
        member_count: manifest.members.len(),
        total_member_bytes: manifest.members.iter().map(|member| member.byte_size).sum(),
    };
    if dry_run {
        return Ok(outcome);
    }
    let temporary = tempfile::Builder::new()
        .prefix(".mcl-release-")
        .tempdir_in(&parent)
        .map_err(|error| AppError::io("create release staging directory", error))?;
    for (path, file) in files {
        write_new_member(temporary.path(), path, &file.bytes)?;
    }
    write_new_member(temporary.path(), "manifest.json", &manifest_bytes)?;
    let staged = verify_release_bundle_integrity(temporary.path())?;
    if staged.manifest_hash != outcome.manifest_hash {
        return Err(release_error(
            "MCL_RELEASE_BUILD_MEMBER_MISMATCH",
            "staged release manifest identity changed before publication",
            "Quarantine the build and retry from unchanged canonical inputs.",
        ));
    }
    fs::rename(temporary.path(), &destination)
        .map_err(|error| AppError::io("atomically publish release directory", error))?;
    Ok(outcome)
}

pub fn verify_release_bundle(
    bundle_dir: &Path,
    expected_manifest_hash: &str,
    lean_command: &str,
) -> Result<ReleaseVerificationReport, AppError> {
    if !is_hash(expected_manifest_hash) {
        return Err(release_error(
            "MCL_RELEASE_EXPECTED_HASH_INVALID",
            "expected release manifest hash is not a lowercase SHA-256 identity",
            "Use the manifest hash emitted by the trusted release build or publication channel.",
        ));
    }
    let verified = verify_release_bundle_integrity(bundle_dir)?;
    if verified.manifest_hash != expected_manifest_hash {
        return Err(release_error(
            "MCL_RELEASE_MANIFEST_HASH_MISMATCH",
            format!(
                "copied release manifest hash {} differs from expected {expected_manifest_hash}",
                verified.manifest_hash
            ),
            "Quarantine the substituted bundle and restore the exact expected release.",
        ));
    }
    let (observed_lean_toolchain, replay_duration_milliseconds) =
        replay_release(&verified.manifest, &verified.files, lean_command)?;
    Ok(ReleaseVerificationReport {
        manifest_hash: verified.manifest_hash,
        profile: verified.manifest.profile,
        member_count: verified.manifest.members.len(),
        total_member_bytes: verified
            .manifest
            .members
            .iter()
            .map(|member| member.byte_size)
            .sum(),
        database_independent: true,
        inventory_verified: true,
        hashes_verified: true,
        schemas_verified: true,
        references_verified: true,
        replay_succeeded: true,
        observed_lean_toolchain,
        replay_duration_milliseconds,
    })
}

pub(crate) fn verify_release_bundle_integrity(
    bundle_dir: &Path,
) -> Result<ReleaseIntegrity, AppError> {
    let root = require_real_directory(bundle_dir, "release bundle")?;
    let manifest_bytes = read_real_file(&root.join("manifest.json"), MAX_MANIFEST_BYTES as u64)?;
    let manifest: ReleaseManifest = decode_canonical(&manifest_bytes, "release manifest")?;
    manifest.validate()?;
    let manifest_hash = manifest.manifest_hash()?;

    let observed_inventory = inventory(&root)?;
    let mut expected_inventory = manifest
        .members
        .iter()
        .map(|member| member.path.clone())
        .collect::<BTreeSet<_>>();
    expected_inventory.insert("manifest.json".to_owned());
    if observed_inventory != expected_inventory {
        return Err(release_error(
            "MCL_RELEASE_INVENTORY_MISMATCH",
            "copied release tree contains missing, extra, or unsafe files",
            "Restore the exact bundle identified by manifest.json.",
        ));
    }

    let mut bytes_by_path = BTreeMap::new();
    for member in &manifest.members {
        let path = safe_member_path(&root, &member.path)?;
        let bytes = read_real_file(&path, member.byte_size)?;
        if bytes.len() as u64 != member.byte_size
            || format!("{:x}", Sha256::digest(&bytes)) != member.content_hash
        {
            return Err(release_error(
                "MCL_RELEASE_MEMBER_INTEGRITY_FAILED",
                format!(
                    "release member `{}` failed hash or size verification",
                    member.path
                ),
                "Quarantine the copied bundle and restore the exact manifest-bound bytes.",
            ));
        }
        if let Some(metadata) = &member.artifact_metadata {
            metadata.validate_bytes(&bytes)?;
        }
        bytes_by_path.insert(member.path.clone(), bytes);
    }

    validate_semantic_closure(&manifest, &bytes_by_path)?;
    Ok(ReleaseIntegrity {
        manifest,
        manifest_hash,
        files: bytes_by_path,
    })
}

fn validate_semantic_closure(
    manifest: &ReleaseManifest,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), AppError> {
    let mut records = BTreeMap::new();
    let mut artifact_hashes = BTreeSet::new();
    let mut environments = BTreeMap::new();
    let mut edge_ids = BTreeSet::new();
    let mut repair_witnesses = Vec::new();

    for member in &manifest.members {
        let bytes = files.get(&member.path).expect("verified inventory");
        match member.kind {
            ReleaseMemberKind::Object => {
                let record: RecordSnapshot = decode_canonical(bytes, &member.path)?;
                validate_record_payload(record.kind, &record.schema_version, &record.payload)?;
                if record.version_hash
                    != record_version_hash(&record.schema_version, &record.payload)?
                    || member.path != object_path(&record)
                    || records
                        .insert(
                            ExactVersionReference {
                                object_id: record.object_id.clone(),
                                version_hash: record.version_hash.clone(),
                            },
                            record,
                        )
                        .is_some()
                {
                    return Err(invalid_semantics(
                        "record snapshot identity or path mismatch",
                    ));
                }
            }
            ReleaseMemberKind::Artifact => {
                artifact_hashes.insert(member.content_hash.clone());
            }
            ReleaseMemberKind::Environment => {
                let environment: EnvironmentSnapshot = decode_canonical(bytes, &member.path)?;
                environment.manifest.validate()?;
                if environment.environment_hash != environment.manifest.environment_hash()?
                    || member.path != format!("environments/{}.json", environment.environment_hash)
                    || environments
                        .insert(environment.environment_hash.clone(), environment)
                        .is_some()
                {
                    return Err(invalid_semantics(
                        "environment snapshot identity or path mismatch",
                    ));
                }
            }
            _ => {}
        }
    }

    if !records.contains_key(&manifest.publication.subject)
        || !environments.contains_key(&manifest.publication.environment_hash)
        || !artifact_hashes.contains(&manifest.publication.module_artifact_hash)
    {
        return Err(invalid_semantics(
            "publication subject, environment, or module artifact is absent",
        ));
    }
    for record in records.values() {
        for reference in record_references(record)? {
            if !records.contains_key(&reference) {
                return Err(invalid_semantics(format!(
                    "record {}@{} has unresolved exact reference {}@{}",
                    record.object_id,
                    record.version_hash,
                    reference.object_id,
                    reference.version_hash
                )));
            }
        }
        validate_record_assets(record, &artifact_hashes, &environments)?;
    }
    for unit in &manifest.pedagogy.unit_order {
        let record = records
            .get(unit)
            .ok_or_else(|| invalid_semantics("pedagogy unit is absent from object closure"))?;
        if record.kind != RecordKind::LearningUnit {
            return Err(invalid_semantics(
                "pedagogy binding contains a non-learning-unit record",
            ));
        }
        let payload: LearningUnitPayload = decode_value(&record.payload, "learning unit")?;
        if payload.review.state != LearningUnitReviewState::Reviewed
            || (manifest.profile == ReleaseProfile::Public
                && payload.training_status != LearningUnitTrainingStatus::EligiblePublic)
        {
            return Err(invalid_semantics(
                "pedagogy release path is unreviewed or ineligible for its profile",
            ));
        }
    }

    let pedagogy_units = manifest
        .pedagogy
        .unit_order
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut connected_pedagogy_units = BTreeSet::from([manifest.pedagogy.root.clone()]);
    let mut observed_pedagogy_edges = BTreeSet::new();
    for member in manifest
        .members
        .iter()
        .filter(|member| member.kind == ReleaseMemberKind::Edge)
    {
        let edge: EdgeSnapshot = decode_canonical(
            files.get(&member.path).expect("verified inventory"),
            &member.path,
        )?;
        let source = ExactVersionReference {
            object_id: edge.source_object_id.clone(),
            version_hash: edge.source_version_hash.clone(),
        };
        let target = ExactVersionReference {
            object_id: edge.target_object_id.clone(),
            version_hash: edge.target_version_hash.clone(),
        };
        if member.path != format!("edges/{}.json", edge.edge_id)
            || uuid::Uuid::parse_str(&edge.edge_id).is_err()
            || !records.contains_key(&source)
            || !records.contains_key(&target)
            || !edge_ids.insert(edge.edge_id.clone())
        {
            return Err(invalid_semantics(
                "edge identity, path, or endpoint mismatch",
            ));
        }
        match edge.kind {
            EdgeKind::PedagogyHardPrerequisite
            | EdgeKind::PedagogySoftPrerequisite
            | EdgeKind::PedagogyRecommendedNext => {
                let allowed = match manifest.pedagogy.mode {
                    crate::domain::ReleasePedagogyMode::Prerequisites => {
                        edge.kind == EdgeKind::PedagogyHardPrerequisite
                            || (manifest.pedagogy.include_soft
                                && edge.kind == EdgeKind::PedagogySoftPrerequisite)
                    }
                    crate::domain::ReleasePedagogyMode::Recommended => {
                        edge.kind == EdgeKind::PedagogyRecommendedNext
                    }
                };
                let link: ReleasePedagogyLinkPayload =
                    decode_value(&edge.payload, "pedagogy edge")?;
                let source_record = records
                    .get(&source)
                    .ok_or_else(|| invalid_semantics("pedagogy source record is absent"))?;
                let source_payload: LearningUnitPayload =
                    decode_value(&source_record.payload, "pedagogy edge source")?;
                let declared = match edge.kind {
                    EdgeKind::PedagogyHardPrerequisite => {
                        source_payload.hard_prerequisites.contains(&target)
                    }
                    EdgeKind::PedagogySoftPrerequisite => {
                        source_payload.soft_prerequisites.contains(&target)
                    }
                    EdgeKind::PedagogyRecommendedNext => true,
                    _ => unreachable!("matched path pedagogy edge"),
                };
                if !allowed
                    || !pedagogy_units.contains(&source)
                    || !pedagogy_units.contains(&target)
                    || link.rationale.trim().is_empty()
                    || link.rationale.len() > 1_048_576
                    || !declared
                {
                    return Err(invalid_semantics(
                        "pedagogy edge is outside the exact reviewed path contract",
                    ));
                }
                connected_pedagogy_units.extend([source, target]);
                observed_pedagogy_edges.insert(edge.edge_id);
            }
            EdgeKind::ResearchRepairs => {
                let repair: crate::domain::ClaimRepairEdgePayload =
                    decode_value(&edge.payload, "claim repair edge")?;
                repair.validate()?;
                let package_path =
                    format!("artifacts/{}", repair.counterexample_package_artifact_hash);
                let package_bytes = required(files, &package_path)?;
                let package: crate::domain::CounterexamplePackage =
                    decode_canonical(package_bytes, "counterexample package")?;
                package.validate()?;
                let repaired_record = records
                    .get(&source)
                    .ok_or_else(|| invalid_semantics("repaired claim record is absent"))?;
                let proposed_payload = serde_json::to_value(
                    &package.proposed_repaired_claim.payload,
                )
                .map_err(|error| {
                    invalid_semantics(format!("cannot serialize proposed repaired claim: {error}"))
                })?;
                let refutation_record =
                    records
                        .get(&repair.refutation_formalization)
                        .ok_or_else(|| {
                            invalid_semantics("repair refutation formalization is absent")
                        })?;
                let refutation: FormalizationPayload = decode_value(
                    &refutation_record.payload,
                    "repair refutation formalization",
                )?;
                let original_record = records
                    .get(&target)
                    .ok_or_else(|| invalid_semantics("original claim record is absent"))?;
                let original: ClaimPayload =
                    decode_value(&original_record.payload, "repair original claim")?;
                if package.package_hash()? != repair.counterexample_package_artifact_hash
                    || package.original_claim != target
                    || package.refutation_witness.formalization != repair.refutation_formalization
                    || package.repair_operation != repair.repair_operation
                    || package.search_provenance.run_id != repair.counterexample_search_run_id
                    || package.search_provenance.event_head_hash
                        != repair.counterexample_search_run_head_hash
                    || repaired_record.kind != RecordKind::Claim
                    || repaired_record.version_hash != package.proposed_repaired_claim.version_hash
                    || repaired_record.schema_version
                        != package.proposed_repaired_claim.schema_version
                    || repaired_record.payload != proposed_payload
                    || original_record.kind != RecordKind::Claim
                    || original.source_reference != package.source
                    || records.get(&package.source).map(|record| record.kind)
                        != Some(RecordKind::Source)
                    || refutation_record.kind != RecordKind::Formalization
                    || refutation.claim_version != package.original_claim
                    || !artifact_hashes.contains(&repair.counterexample_package_artifact_hash)
                    || !artifact_hashes.contains(&package.checker.module_artifact_hash)
                    || !environments.contains_key(&package.checker.environment_hash)
                    || package.minimization.as_ref().is_some_and(|minimization| {
                        minimization
                            .supporting_artifact_hashes
                            .iter()
                            .any(|hash| !artifact_hashes.contains(hash))
                    })
                    || refutation.environment_hash != package.checker.environment_hash
                    || refutation.module_artifact_hash != package.checker.module_artifact_hash
                    || refutation.declaration_name != package.checker.declaration_name
                    || refutation.exact_theorem_type != package.checker.exact_theorem_type
                    || refutation.declaration_hash != package.checker.declaration_hash
                {
                    return Err(invalid_semantics(
                        "claim repair edge is outside the exact logical closure",
                    ));
                }
                repair_witnesses.push(package.refutation_witness);
            }
            _ => {
                return Err(invalid_semantics(
                    "release edge kind is outside the closed pedagogy/repair slice",
                ));
            }
        }
    }
    if observed_pedagogy_edges != manifest.pedagogy.edge_ids.iter().cloned().collect()
        || connected_pedagogy_units != pedagogy_units
    {
        return Err(invalid_semantics(
            "pedagogy edge binding differs from edge inventory",
        ));
    }

    validate_evidence(
        manifest,
        files,
        &records,
        &artifact_hashes,
        &environments,
        &repair_witnesses,
    )?;
    validate_publication_reports(manifest, files, &artifact_hashes)?;
    validate_license_index(manifest, files)?;
    validate_replay_bindings(manifest, files)?;
    Ok(())
}

fn validate_evidence(
    manifest: &ReleaseManifest,
    files: &BTreeMap<String, Vec<u8>>,
    records: &BTreeMap<ExactVersionReference, RecordSnapshot>,
    artifact_hashes: &BTreeSet<String>,
    environments: &BTreeMap<String, EnvironmentSnapshot>,
    repair_witnesses: &[crate::domain::ClaimResearchStatusWitness],
) -> Result<(), AppError> {
    let mut snapshots = BTreeMap::new();
    for member in manifest
        .members
        .iter()
        .filter(|member| member.kind == ReleaseMemberKind::Evidence)
    {
        let bytes = files.get(&member.path).expect("verified inventory");
        let evidence: EvidenceSnapshot = decode_canonical(bytes, &member.path)?;
        evidence.payload.validate()?;
        if evidence.evidence_hash != evidence.payload.evidence_hash()?
            || uuid::Uuid::parse_str(&evidence.evidence_id).is_err()
            || member.path
                != format!(
                    "evidence/{}@{}.json",
                    evidence.evidence_id, evidence.evidence_hash
                )
            || !records.contains_key(&evidence.payload.subject)
            || evidence
                .payload
                .artifact_hashes
                .iter()
                .any(|hash| !artifact_hashes.contains(hash))
            || evidence
                .payload
                .environment_hash
                .as_ref()
                .is_some_and(|hash| !environments.contains_key(hash))
            || snapshots
                .insert(evidence.evidence_id.clone(), evidence)
                .is_some()
        {
            return Err(invalid_semantics("evidence snapshot closure mismatch"));
        }
    }
    if snapshots.values().any(|evidence| {
        evidence
            .payload
            .supersedes_evidence_id
            .as_ref()
            .is_some_and(|id| !snapshots.contains_key(id))
    }) {
        return Err(invalid_semantics(
            "evidence supersession reference is absent from the release",
        ));
    }

    let mut fidelity_reports = BTreeMap::new();
    for evidence in snapshots.values().filter(|evidence| {
        evidence.payload.evidence_kind == crate::domain::EvidenceKind::StatementFidelityReview
    }) {
        let report_path = format!(
            "reports/fidelity/{}@{}.json",
            evidence.evidence_id, evidence.evidence_hash
        );
        let report_bytes = required(files, &report_path)?;
        let report: crate::domain::VersionedFidelityReviewReport =
            decode_canonical(report_bytes, "fidelity report")?;
        report.validate()?;
        let request = report.request();
        let report_hash = format!("{:x}", Sha256::digest(report_bytes));
        let artifact_path = format!("artifacts/{report_hash}");
        if !evidence.payload.artifact_hashes.contains(&report_hash)
            || required(files, &artifact_path)? != report_bytes
            || &evidence.payload.subject != request.formalization()
            || !records.contains_key(request.source())
            || !records.contains_key(request.claim())
            || !records.contains_key(request.formalization())
        {
            return Err(invalid_semantics(
                "fidelity evidence report or exact lineage is incomplete",
            ));
        }
        fidelity_reports.insert(evidence.evidence_id.clone(), (report_hash, request));
    }

    let authority = snapshots
        .get(&manifest.publication.authority_evidence_id)
        .ok_or_else(|| invalid_semantics("bound publication authority evidence is absent"))?;
    let fidelity = snapshots
        .get(&manifest.publication.fidelity_evidence_id)
        .ok_or_else(|| invalid_semantics("bound publication fidelity evidence is absent"))?;
    let authority_binding = authority
        .payload
        .publication_authority
        .as_ref()
        .ok_or_else(|| invalid_semantics("release authority evidence has no receipt binding"))?;
    let expected_authority_kind = match manifest.publication.outcome {
        crate::domain::PublicationOutcome::Proof => crate::domain::EvidenceKind::LeanKernelProof,
        crate::domain::PublicationOutcome::Refutation => {
            crate::domain::EvidenceKind::LeanKernelRefutation
        }
    };
    if authority.evidence_hash != manifest.publication.authority_evidence_hash
        || authority.payload.subject != manifest.publication.subject
        || authority.payload.evidence_kind != expected_authority_kind
        || authority.payload.result != crate::domain::EvidenceResult::Accepted
        || authority.payload.authority_class != crate::domain::EvidenceAuthorityClass::Authoritative
        || authority.payload.environment_hash.as_deref()
            != Some(manifest.publication.environment_hash.as_str())
        || authority.payload.stale
        || authority_binding.ingestion_receipt_hash != manifest.publication.ingestion_receipt_hash
        || authority_binding.stage_hash != manifest.publication.stage_hash
        || authority_binding.report_artifact_hash != manifest.publication.report_artifact_hash
        || authority_binding.retained_closure_artifact_hash
            != manifest.publication.retained_closure_artifact_hash
        || authority_binding.attestation_bundle_artifact_hash
            != manifest.publication.attestation_bundle_artifact_hash
        || authority_binding.raw_verification_hash != manifest.publication.raw_verification_hash
        || authority_binding.publication_request_hash != manifest.publication.request_hash
        || authority_binding.publication_policy_hash != manifest.publication.policy_hash
    {
        return Err(invalid_semantics(
            "release authority evidence differs from the exact publication binding",
        ));
    }

    let (fidelity_report_hash, fidelity_request) = fidelity_reports
        .get(&fidelity.evidence_id)
        .ok_or_else(|| invalid_semantics("bound fidelity report is absent"))?;
    if fidelity.evidence_hash != manifest.publication.fidelity_evidence_hash
        || fidelity.payload.subject != manifest.publication.subject
        || fidelity.payload.evidence_kind != crate::domain::EvidenceKind::StatementFidelityReview
        || fidelity.payload.result != crate::domain::EvidenceResult::Accepted
        || fidelity.payload.authority_class != crate::domain::EvidenceAuthorityClass::Reviewed
        || fidelity.payload.stale
        || fidelity_request.verdict() != crate::domain::FidelityVerdict::Verified
        || fidelity_request.formalization() != &manifest.publication.subject
        || !records.contains_key(fidelity_request.source())
        || !records.contains_key(fidelity_request.claim())
        || fidelity_report_hash != &manifest.publication.fidelity_report_artifact_hash
        || !fidelity
            .payload
            .artifact_hashes
            .contains(&manifest.publication.fidelity_report_artifact_hash)
    {
        return Err(invalid_semantics(
            "release fidelity evidence or report differs from the exact reviewed witness",
        ));
    }
    for witness in repair_witnesses {
        witness.validate()?;
        let repair_authority = snapshots
            .get(&witness.authority_evidence_id)
            .ok_or_else(|| invalid_semantics("repair authority evidence is absent"))?;
        let repair_fidelity = snapshots
            .get(&witness.fidelity_evidence_id)
            .ok_or_else(|| invalid_semantics("repair fidelity evidence is absent"))?;
        let repair_binding = repair_authority
            .payload
            .publication_authority
            .as_ref()
            .ok_or_else(|| invalid_semantics("repair authority receipt binding is absent"))?;
        let (repair_report_hash, repair_request) = fidelity_reports
            .get(&witness.fidelity_evidence_id)
            .ok_or_else(|| invalid_semantics("repair fidelity report is absent"))?;
        if repair_authority.evidence_hash != witness.authority_evidence_hash
            || repair_authority.payload.subject != witness.formalization
            || repair_authority.payload.evidence_kind
                != crate::domain::EvidenceKind::LeanKernelRefutation
            || repair_authority.payload.result != crate::domain::EvidenceResult::Accepted
            || repair_authority.payload.authority_class
                != crate::domain::EvidenceAuthorityClass::Authoritative
            || repair_authority.payload.stale
            || repair_binding.ingestion_receipt_hash != witness.publication_receipt_hash
            || repair_fidelity.evidence_hash != witness.fidelity_evidence_hash
            || repair_fidelity.payload.subject != witness.formalization
            || repair_fidelity.payload.evidence_kind
                != crate::domain::EvidenceKind::StatementFidelityReview
            || repair_fidelity.payload.result != crate::domain::EvidenceResult::Accepted
            || repair_fidelity.payload.authority_class
                != crate::domain::EvidenceAuthorityClass::Reviewed
            || repair_fidelity.payload.stale
            || repair_report_hash != &witness.fidelity_report_artifact_hash
            || repair_request.formalization() != &witness.formalization
            || repair_request.schema_version() != witness.fidelity_request_schema_version
            || repair_request.reviewed_source_relation()
                != Some(crate::domain::ReviewedSourceRelation::LogicalNegation)
            || repair_request.verdict() != crate::domain::FidelityVerdict::Verified
        {
            return Err(invalid_semantics(
                "counterexample repair authority/fidelity witness is incomplete or changed",
            ));
        }
    }
    Ok(())
}

fn validate_publication_reports(
    manifest: &ReleaseManifest,
    files: &BTreeMap<String, Vec<u8>>,
    artifact_hashes: &BTreeSet<String>,
) -> Result<(), AppError> {
    let policy = crate::domain::publication::committed_publication_policy()?;
    let report: crate::domain::PublicationReport = decode_canonical(
        required(files, "reports/publication-report.json")?,
        "publication report",
    )?;
    report.validate_candidate(&policy)?;
    let closure: PublicationRetainedClosure = decode_canonical(
        required(files, "reports/publication-retained-closure.json")?,
        "publication retained closure",
    )?;
    closure.validate(&report.request)?;
    let stage: PublicationStageSnapshot = decode_canonical(
        required(files, "reports/publication-stage.json")?,
        "publication stage",
    )?;
    stage.stage.validate()?;
    let receipt: PublicationIngestionReceiptSnapshot = decode_canonical(
        required(files, "reports/publication-receipt.json")?,
        "publication receipt",
    )?;
    receipt.verification.validate(&report, &policy)?;
    let receipt_hash = crate::canonical::value_hash(
        &serde_json::to_value(&receipt.verification).map_err(|error| {
            invalid_semantics(format!(
                "cannot serialize publication verification: {error}"
            ))
        })?,
    )?;
    if report.report_hash(&policy)? != manifest.publication.report_artifact_hash
        || closure.closure_hash(&report.request)?
            != manifest.publication.retained_closure_artifact_hash
        || stage.stage.stage_hash()? != manifest.publication.stage_hash
        || receipt_hash != receipt.receipt_hash
        || receipt.receipt_hash != manifest.publication.ingestion_receipt_hash
        || receipt.stage_hash != stage.stage_hash
        || report.request.request_hash()? != manifest.publication.request_hash
        || report.request.policy_hash != manifest.publication.policy_hash
        || report.request.subject != manifest.publication.subject
        || report.request.outcome != manifest.publication.outcome
        || report.request.environment_hash != manifest.publication.environment_hash
        || report.request.module_artifact_hash != manifest.publication.module_artifact_hash
        || report.request.declaration_name != manifest.publication.declaration_name
        || stage.stage.report_artifact_hash != manifest.publication.report_artifact_hash
        || stage.stage.retained_closure_artifact_hash
            != manifest.publication.retained_closure_artifact_hash
        || stage.stage.attestation_bundle_artifact_hash
            != manifest.publication.attestation_bundle_artifact_hash
        || receipt.verification.raw_verification_hash != manifest.publication.raw_verification_hash
    {
        return Err(invalid_semantics(
            "publication report, stage, receipt, or manifest mismatch",
        ));
    }
    let required_artifacts = stage
        .stage
        .retained_artifacts
        .iter()
        .map(|member| member.artifact_hash.clone())
        .chain([
            stage.stage.report_artifact_hash.clone(),
            stage.stage.retained_closure_artifact_hash.clone(),
            stage.stage.attestation_bundle_artifact_hash.clone(),
            receipt.verification.raw_verification_hash.clone(),
            receipt.receipt_hash.clone(),
        ]);
    if required_artifacts
        .into_iter()
        .any(|hash| !artifact_hashes.contains(&hash))
    {
        return Err(invalid_semantics("publication CAS closure is incomplete"));
    }
    for (report_path, artifact_hash) in [
        (
            "reports/publication-report.json",
            manifest.publication.report_artifact_hash.as_str(),
        ),
        (
            "reports/publication-retained-closure.json",
            manifest.publication.retained_closure_artifact_hash.as_str(),
        ),
        (
            "reports/attestation-bundle.json",
            manifest
                .publication
                .attestation_bundle_artifact_hash
                .as_str(),
        ),
        (
            "reports/raw-attestation-verification.json",
            manifest.publication.raw_verification_hash.as_str(),
        ),
        (
            "reports/canonical-attestation-receipt.json",
            manifest.publication.ingestion_receipt_hash.as_str(),
        ),
    ] {
        let artifact_path = format!("artifacts/{artifact_hash}");
        if required(files, report_path)? != required(files, &artifact_path)? {
            return Err(invalid_semantics(format!(
                "publication report copy `{report_path}` differs from its exact CAS member"
            )));
        }
    }
    Ok(())
}

fn validate_license_index(
    manifest: &ReleaseManifest,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), AppError> {
    let index: ReleaseLicenseIndex = decode_canonical(
        required(files, "licenses/index.json")?,
        "release license index",
    )?;
    let expected = manifest
        .members
        .iter()
        .filter(|member| member.path != "licenses/index.json")
        .map(|member| ReleaseLicenseEntry {
            path: member.path.clone(),
            license_expression: member.license_expression.clone(),
            restriction: member.restriction,
        })
        .collect::<Vec<_>>();
    if index.schema_version != "release_license_index/1" || index.entries != expected {
        return Err(invalid_semantics(
            "release license index differs from manifest policy",
        ));
    }
    Ok(())
}

fn validate_replay_bindings(
    manifest: &ReleaseManifest,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), AppError> {
    let replay: ReleaseReplayBinding = decode_canonical(
        required(files, "replay/replay.json")?,
        "release replay binding",
    )?;
    let environment: EnvironmentSnapshot = decode_canonical(
        required(files, &manifest.replay.environment_path)?,
        "release replay environment",
    )?;
    let environment_path = format!(
        "environments/{}.json",
        manifest.publication.environment_hash
    );
    let module_path = format!("artifacts/{}", manifest.publication.module_artifact_hash);
    if replay != manifest.replay
        || environment.environment_hash != manifest.publication.environment_hash
        || required(files, &manifest.replay.environment_path)?
            != required(files, &environment_path)?
        || required(files, &manifest.replay.module_path)? != required(files, &module_path)?
        || format!(
            "{:x}",
            Sha256::digest(required(files, &manifest.replay.module_path)?)
        ) != manifest.publication.module_artifact_hash
    {
        return Err(invalid_semantics(
            "release replay inputs differ from publication binding",
        ));
    }
    let exported: crate::domain::ReleasePedagogyBinding = decode_canonical(
        required(files, "exports/pedagogy-path.json")?,
        "pedagogy path export",
    )?;
    if exported != manifest.pedagogy {
        return Err(invalid_semantics(
            "pedagogy export differs from manifest binding",
        ));
    }
    Ok(())
}

fn replay_release(
    manifest: &ReleaseManifest,
    files: &BTreeMap<String, Vec<u8>>,
    lean_command: &str,
) -> Result<(String, u64), AppError> {
    let environment: EnvironmentSnapshot = decode_canonical(
        required(files, &manifest.replay.environment_path)?,
        "release replay environment",
    )?;
    let module = required(files, &manifest.replay.module_path)?;
    if let Some(token) = crate::verifier::scan_forbidden_source_token(module)? {
        return Err(release_error(
            "MCL_RELEASE_REPLAY_SOURCE_UNSAFE",
            format!("release module contains forbidden verifier token `{token}`"),
            "Quarantine the release and rebuild from a safe publication module.",
        ));
    }
    let workspace = tempfile::Builder::new()
        .prefix(".mcl-release-replay-")
        .tempdir()
        .map_err(|error| AppError::io("create release replay workspace", error))?;
    let mut driver = module.to_vec();
    driver.extend_from_slice(format!("\n#check {}\n", manifest.replay.declaration_name).as_bytes());
    write_new_member(workspace.path(), "Driver.lean", &driver)?;
    let result = crate::verifier::execute_release_lean(
        lean_command,
        workspace.path(),
        "Driver.lean",
        &environment.manifest,
    )?;
    if result.timed_out
        || result.output_limit_exceeded
        || result.exit_code != Some(0)
        || !result.stderr.is_empty()
    {
        return Err(release_error(
            "MCL_RELEASE_REPLAY_FAILED",
            format!(
                "Lean replay failed (exit={:?}, timed_out={}, output_limit_exceeded={}, stderr_bytes={})",
                result.exit_code,
                result.timed_out,
                result.output_limit_exceeded,
                result.stderr.len()
            ),
            "Activate the exact pinned Lean toolchain and verify the unchanged release on its declared platform.",
        ));
    }
    Ok((
        result.observed_toolchain_version,
        result.duration_milliseconds,
    ))
}

pub(crate) fn record_references(
    record: &RecordSnapshot,
) -> Result<Vec<ExactVersionReference>, AppError> {
    let references = match record.kind {
        RecordKind::Source => Vec::new(),
        RecordKind::Claim => {
            let payload: ClaimPayload = decode_value(&record.payload, "claim")?;
            let mut references = vec![payload.source_reference];
            references.extend(payload.concept_links);
            references.extend(payload.source_citations);
            references
        }
        RecordKind::Concept => {
            let payload: ConceptPayload = decode_value(&record.payload, "concept")?;
            let mut references = payload
                .external_taxonomy_crosswalks
                .into_iter()
                .map(|crosswalk| crosswalk.source_reference)
                .collect::<Vec<_>>();
            references.extend(payload.pedagogy_metadata_references);
            references.extend(payload.provenance_references);
            references
        }
        RecordKind::Formalization => {
            let payload: FormalizationPayload = decode_value(&record.payload, "formalization")?;
            vec![payload.claim_version]
        }
        RecordKind::LearningUnit => {
            let payload: LearningUnitPayload = decode_value(&record.payload, "learning unit")?;
            let mut references = vec![ExactVersionReference {
                object_id: payload.target.object_id,
                version_hash: payload.target.version_hash,
            }];
            references.extend(payload.hard_prerequisites);
            references.extend(payload.soft_prerequisites);
            references.extend(payload.grounded_source_references);
            references.extend(payload.examples);
            references.extend(payload.nonexamples);
            references.extend(payload.counterexamples);
            references.extend(payload.misconceptions);
            references.extend(payload.exercises);
            references.extend(payload.mastery_checks);
            references.extend(payload.formalization_references);
            references.extend(payload.application_references);
            references.extend(payload.frontier_references);
            references
        }
    };
    Ok(references)
}

pub(crate) fn record_policy(
    record: &RecordSnapshot,
) -> Result<(Option<String>, ArtifactRestriction), AppError> {
    match record.kind {
        RecordKind::Source => {
            let source: SourcePayload = decode_value(&record.payload, "source")?;
            let restriction = match (source.redaction_class, source.redistribution_status) {
                (RedactionClass::Public, RedistributionStatus::Allowed)
                    if source.license_expression.is_some() =>
                {
                    ArtifactRestriction::Public
                }
                (RedactionClass::Restricted, _) | (_, RedistributionStatus::Restricted) => {
                    ArtifactRestriction::Restricted
                }
                _ => ArtifactRestriction::Private,
            };
            Ok((source.license_expression, restriction))
        }
        RecordKind::LearningUnit => {
            let unit: LearningUnitPayload = decode_value(&record.payload, "learning unit")?;
            let restriction = if unit.training_status == LearningUnitTrainingStatus::EligiblePublic
                && unit.license_expression.is_some()
            {
                ArtifactRestriction::Public
            } else {
                ArtifactRestriction::Private
            };
            Ok((unit.license_expression, restriction))
        }
        _ => Ok((
            Some(GENERATED_RELEASE_LICENSE.to_owned()),
            ArtifactRestriction::Public,
        )),
    }
}

pub(crate) fn object_path(record: &RecordSnapshot) -> String {
    format!(
        "objects/{}/{}@{}.json",
        record.kind.as_str(),
        record.object_id,
        record.version_hash
    )
}

fn validate_record_assets(
    record: &RecordSnapshot,
    artifacts: &BTreeSet<String>,
    environments: &BTreeMap<String, EnvironmentSnapshot>,
) -> Result<(), AppError> {
    match record.kind {
        RecordKind::Formalization => {
            let payload: FormalizationPayload = decode_value(&record.payload, "formalization")?;
            if !artifacts.contains(&payload.module_artifact_hash)
                || !environments.contains_key(&payload.environment_hash)
            {
                return Err(invalid_semantics(
                    "formalization artifact or environment is absent",
                ));
            }
        }
        RecordKind::LearningUnit => {
            let payload: LearningUnitPayload = decode_value(&record.payload, "learning unit")?;
            if !artifacts.contains(&payload.content_artifact_hash) {
                return Err(invalid_semantics(
                    "learning-unit content artifact is absent",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn resolve_new_output(output: &Path) -> Result<(PathBuf, PathBuf), AppError> {
    let absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| AppError::io("resolve current directory", error))?
            .join(output)
    };
    if fs::symlink_metadata(&absolute).is_ok() {
        return Err(release_error(
            "MCL_RELEASE_OUTPUT_EXISTS",
            format!("release output already exists at {}", absolute.display()),
            "Choose a new empty destination; release builds never overwrite paths.",
        ));
    }
    let parent = absolute.parent().ok_or_else(|| {
        release_error(
            "MCL_RELEASE_OUTPUT_UNSAFE",
            "release output has no parent directory",
            "Choose a new directory beneath a real existing parent.",
        )
    })?;
    let parent = require_real_directory(parent, "release output parent")?;
    let name = absolute.file_name().ok_or_else(|| {
        release_error(
            "MCL_RELEASE_OUTPUT_UNSAFE",
            "release output has no plain directory name",
            "Choose a named directory beneath a real existing parent.",
        )
    })?;
    Ok((parent.clone(), parent.join(name)))
}

fn write_new_member(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), AppError> {
    let destination = safe_member_path(root, relative)?;
    let parent = destination.parent().expect("member has parent");
    fs::create_dir_all(parent)
        .map_err(|error| AppError::io("create release member directory", error))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&destination)
        .map_err(|error| AppError::io("create release member", error))?;
    file.write_all(bytes)
        .map_err(|error| AppError::io("write release member", error))?;
    file.sync_all()
        .map_err(|error| AppError::io("sync release member", error))
}

fn require_real_directory(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| AppError::io("inspect directory", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(release_error(
            "MCL_RELEASE_PATH_UNSAFE",
            format!("{label} is not a real directory"),
            "Use a real directory tree without symbolic links.",
        ));
    }
    path.canonicalize()
        .map_err(|error| AppError::io("canonicalize release directory", error))
}

fn safe_member_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || relative.contains('\\')
    {
        return Err(release_error(
            "MCL_RELEASE_PATH_UNSAFE",
            format!("unsafe release path `{relative}`"),
            "Use manifest-controlled relative paths without traversal or platform separators.",
        ));
    }
    Ok(root.join(path))
}

fn read_real_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io("inspect release member", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(release_error(
            "MCL_RELEASE_PATH_UNSAFE",
            format!("release member {} is unsafe or oversized", path.display()),
            "Restore the exact regular file within its manifest bound.",
        ));
    }
    fs::read(path).map_err(|error| AppError::io("read release member", error))
}

fn inventory(root: &Path) -> Result<BTreeSet<String>, AppError> {
    fn visit(
        root: &Path,
        directory: &Path,
        files: &mut BTreeSet<String>,
        entries: &mut usize,
    ) -> Result<(), AppError> {
        for entry in fs::read_dir(directory)
            .map_err(|error| AppError::io("read release directory", error))?
        {
            let entry = entry.map_err(|error| AppError::io("read release entry", error))?;
            *entries += 1;
            if *entries > MAX_TREE_ENTRIES {
                return Err(release_error(
                    "MCL_RELEASE_INVENTORY_MISMATCH",
                    "release tree exceeds its bounded entry count",
                    "Restore the exact bounded release tree.",
                ));
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| AppError::io("inspect release entry", error))?;
            if metadata.file_type().is_symlink() {
                return Err(release_error(
                    "MCL_RELEASE_PATH_UNSAFE",
                    "release tree contains a symbolic link",
                    "Use a copied release containing only real directories and files.",
                ));
            }
            if metadata.is_dir() {
                visit(root, &path, files, entries)?;
            } else if metadata.is_file() {
                let relative = path.strip_prefix(root).expect("walk rooted");
                let components = relative
                    .components()
                    .map(|component| {
                        let Component::Normal(name) = component else {
                            return Err(invalid_semantics("release inventory path is unsafe"));
                        };
                        name.to_str()
                            .map(str::to_owned)
                            .ok_or_else(|| invalid_semantics("release path is not UTF-8"))
                    })
                    .collect::<Result<Vec<_>, AppError>>()?;
                files.insert(components.join("/"));
            } else {
                return Err(release_error(
                    "MCL_RELEASE_PATH_UNSAFE",
                    "release tree contains a non-file filesystem entry",
                    "Use only regular directories and files.",
                ));
            }
        }
        Ok(())
    }

    let mut files = BTreeSet::new();
    let mut entries = 0;
    visit(root, root, &mut files, &mut entries)?;
    Ok(files)
}

fn decode_canonical<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    label: &str,
) -> Result<T, AppError> {
    let decoded: T = serde_json::from_slice(bytes).map_err(|error| {
        release_error(
            "MCL_RELEASE_JSON_INVALID",
            format!("{label} is not closed valid JSON: {error}"),
            "Restore the exact canonical JSON member.",
        )
    })?;
    let value = serde_json::to_value(&decoded).map_err(|error| {
        release_error(
            "MCL_RELEASE_JSON_INVALID",
            format!("{label} cannot be serialized: {error}"),
            "Restore the exact canonical JSON member.",
        )
    })?;
    if canonical_json(&value)? != bytes {
        return Err(release_error(
            "MCL_RELEASE_JSON_NONCANONICAL",
            format!("{label} is not exact canonical JSON"),
            "Restore the exact canonical JSON bytes without whitespace or unknown fields.",
        ));
    }
    Ok(decoded)
}

fn decode_value<T: DeserializeOwned>(value: &Value, label: &str) -> Result<T, AppError> {
    serde_json::from_value(value.clone()).map_err(|error| {
        release_error(
            "MCL_RELEASE_SCHEMA_INVALID",
            format!("{label} payload is invalid: {error}"),
            "Restore the exact schema-valid canonical record.",
        )
    })
}

fn required<'a>(files: &'a BTreeMap<String, Vec<u8>>, path: &str) -> Result<&'a [u8], AppError> {
    files
        .get(path)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid_semantics(format!("required release member `{path}` is absent")))
}

fn invalid_semantics(message: impl Into<String>) -> AppError {
    release_error(
        "MCL_RELEASE_SEMANTIC_CLOSURE_INVALID",
        message,
        "Quarantine the release and rebuild from the exact receipt-bound canonical closure.",
    )
}

fn release_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
