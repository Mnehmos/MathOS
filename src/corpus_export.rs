use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use jsonschema::{Retrieve, Uri};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::canonical::{canonical_json, value_hash};
use crate::domain::schemas::{
    ClaimPayload, ExactVersionReference, FormalizationPayload, RedactionClass,
    RedistributionStatus, SourcePayload,
};
use crate::domain::{
    ArtifactRestriction, CorpusExportCuration, CorpusExportManifest, CorpusExportMember,
    CorpusExportMemberKind, CorpusExportOutputBinding, CorpusExportPolicy,
    CorpusExportSourceBinding, CorpusExportUpstreamBinding, EnvironmentSnapshot,
    MathCorpusDifficultyBin, MathCorpusDomain, MathCorpusLevel,
    PublicationIngestionReceiptSnapshot, RecordKind, RecordSnapshot, ReleaseProfile,
};
use crate::error::AppError;
use crate::release::{GENERATED_RELEASE_LICENSE, ReleaseIntegrity};

const UPSTREAM_REPOSITORY: &str = "Mnehmos/mathcorpus";
const UPSTREAM_COMMIT: &str = "a0d08c9ace0dcc70a8bc281dcf29c560242075d3";
const UPSTREAM_TREE: &str = "62bc32fac877a82958ffcbe86402f8e793295f99";
const PACKET_SCHEMA_HASH: &str = "6b6dfb3d558acbe53c9ca9e4d559f4e2677486e1a9d3b5c852a7cab6f7af532e";
const MCIP_DEFS_SCHEMA_HASH: &str =
    "d0201d4abdb106974de7b27f2a10909069e0aac1df490811bed3c15fec123137";
const MCIP_BUNDLE_SCHEMA_HASH: &str =
    "00eab00b02761d4e82574052d7c5547d7a1b70a49e434f87a5ff77f8c3e6fb49";
const MCIP_PACKET_IDENTITY_SCHEMA_HASH: &str =
    "e40aab76c1682f8ee5be840c5eeae82f3e0da1572b2b322ed20f84ad74e69595";
const MCIP_PROOF_VARIANT_SCHEMA_HASH: &str =
    "497f1dce5e49e311a2af586c2bd035439724c13b85e37cb734909eccebbb5fdb";
const MCIP_DEPENDENCY_MANIFEST_SCHEMA_HASH: &str =
    "4014bbb84b2e09bd838ca60365a8d44156342d3e44918236a37cc87c15eb8bbf";

const PACKET_SCHEMA: &[u8] = include_bytes!(
    "../schemas/mathcorpus/upstream-a0d08c9ace0dcc70a8bc281dcf29c560242075d3/packet.schema.json"
);
const UPSTREAM_LICENSE: &[u8] = include_bytes!(
    "../schemas/mathcorpus/upstream-a0d08c9ace0dcc70a8bc281dcf29c560242075d3/LICENSE"
);
const MCIP_DEFS_SCHEMA: &[u8] = include_bytes!(
    "../schemas/mcip/v1/upstream-a0d08c9ace0dcc70a8bc281dcf29c560242075d3/_defs.schema.json"
);
const MCIP_BUNDLE_SCHEMA: &[u8] = include_bytes!(
    "../schemas/mcip/v1/upstream-a0d08c9ace0dcc70a8bc281dcf29c560242075d3/bundle.schema.json"
);
const MCIP_PACKET_IDENTITY_SCHEMA: &[u8] = include_bytes!(
    "../schemas/mcip/v1/upstream-a0d08c9ace0dcc70a8bc281dcf29c560242075d3/packet_identity.schema.json"
);
const MCIP_PROOF_VARIANT_SCHEMA: &[u8] = include_bytes!(
    "../schemas/mcip/v1/upstream-a0d08c9ace0dcc70a8bc281dcf29c560242075d3/proof_variant.schema.json"
);
const MCIP_DEPENDENCY_MANIFEST_SCHEMA: &[u8] = include_bytes!(
    "../schemas/mcip/v1/upstream-a0d08c9ace0dcc70a8bc281dcf29c560242075d3/dependency_manifest.schema.json"
);

const MAX_MANIFEST_BYTES: u64 = 4 * 1_048_576;
const MAX_MEMBER_BYTES: u64 = 16 * 1_048_576;
const MAX_TREE_ENTRIES: usize = 64;

#[derive(Clone, Debug)]
struct ExportFile {
    bytes: Vec<u8>,
    kind: CorpusExportMemberKind,
    license_expression: Option<String>,
    restriction: ArtifactRestriction,
}

#[derive(Debug)]
struct Projection {
    manifest: CorpusExportManifest,
    files: BTreeMap<String, ExportFile>,
}

#[derive(Debug)]
struct VerifiedExport {
    manifest: CorpusExportManifest,
    manifest_hash: String,
    files: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CorpusExportOutcome {
    pub dry_run: bool,
    pub manifest_hash: String,
    pub export_path: PathBuf,
    pub source_release_manifest_hash: String,
    pub policy: CorpusExportPolicy,
    pub packet_sha256: String,
    pub member_count: usize,
    pub total_member_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CorpusExportVerificationReport {
    pub manifest_hash: String,
    pub source_release_manifest_hash: String,
    pub policy: CorpusExportPolicy,
    pub packet_sha256: String,
    pub member_count: usize,
    pub total_member_bytes: u64,
    pub database_independent: bool,
    pub inventory_verified: bool,
    pub hashes_verified: bool,
    pub schemas_verified: bool,
    pub bindings_verified: bool,
    pub deterministic_reprojection_verified: bool,
}

pub struct CorpusExportRequest<'a> {
    pub bundle_dir: &'a Path,
    pub expected_manifest_hash: &'a str,
    pub packet_id: &'a str,
    pub domain: MathCorpusDomain,
    pub level: MathCorpusLevel,
    pub difficulty_bin: MathCorpusDifficultyBin,
    pub output_dir: &'a Path,
    pub dry_run: bool,
}

pub fn export_release(request: CorpusExportRequest<'_>) -> Result<CorpusExportOutcome, AppError> {
    let source = verify_source_release(request.bundle_dir, request.expected_manifest_hash)?;
    let curation = CorpusExportCuration {
        packet_id: request.packet_id.to_owned(),
        domain: request.domain,
        level: request.level,
        difficulty_bin: request.difficulty_bin,
        policy: policy_for(source.manifest.profile),
    };
    let projection = project_release(&source, &curation)?;
    projection.manifest.validate()?;
    validate_projection_files(&projection)?;

    let (parent, destination) = resolve_new_output(request.output_dir)?;
    let manifest_bytes = canonical_bytes(&projection.manifest, "corpus export manifest")?;
    let manifest_hash = sha256(&manifest_bytes);
    let outcome = CorpusExportOutcome {
        dry_run: request.dry_run,
        manifest_hash: manifest_hash.clone(),
        export_path: destination.clone(),
        source_release_manifest_hash: source.manifest_hash.clone(),
        policy: projection.manifest.curation.policy,
        packet_sha256: projection.manifest.outputs.packet_sha256.clone(),
        member_count: projection.manifest.members.len(),
        total_member_bytes: projection
            .manifest
            .members
            .iter()
            .map(|member| member.byte_size)
            .sum(),
    };
    if request.dry_run {
        return Ok(outcome);
    }

    let temporary = tempfile::Builder::new()
        .prefix(".mcl-corpus-export-")
        .tempdir_in(&parent)
        .map_err(|error| AppError::io("create corpus export staging directory", error))?;
    for (path, file) in &projection.files {
        write_new_member(temporary.path(), path, &file.bytes)?;
    }
    write_new_member(temporary.path(), "manifest.json", &manifest_bytes)?;
    let staged = verify_export_integrity(temporary.path())?;
    if staged.manifest_hash != manifest_hash
        || staged.manifest != projection.manifest
        || staged.files
            != projection
                .files
                .iter()
                .map(|(path, file)| (path.clone(), file.bytes.clone()))
                .collect()
    {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_STAGING_MISMATCH",
            "staged corpus export changed before atomic publication",
            "Quarantine the staging directory and retry from unchanged inputs.",
        ));
    }
    fs::rename(temporary.path(), &destination)
        .map_err(|error| AppError::io("atomically publish corpus export directory", error))?;
    Ok(outcome)
}

pub fn verify_export(
    export_dir: &Path,
    expected_manifest_hash: &str,
    source_bundle_dir: &Path,
) -> Result<CorpusExportVerificationReport, AppError> {
    require_hash(
        expected_manifest_hash,
        "MCL_CORPUS_EXPORT_EXPECTED_HASH_INVALID",
        "expected corpus export manifest hash",
    )?;
    let observed = verify_export_integrity(export_dir)?;
    if observed.manifest_hash != expected_manifest_hash {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_MANIFEST_HASH_MISMATCH",
            format!(
                "corpus export manifest hash {} differs from expected {expected_manifest_hash}",
                observed.manifest_hash
            ),
            "Quarantine the substituted export and restore the exact trusted projection.",
        ));
    }
    let source = verify_source_release(
        source_bundle_dir,
        &observed.manifest.source_release.release_manifest_hash,
    )?;
    let expected = project_release(&source, &observed.manifest.curation)?;
    let expected_files = expected
        .files
        .iter()
        .map(|(path, file)| (path.clone(), file.bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    if expected.manifest != observed.manifest || expected_files != observed.files {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_REPROJECTION_MISMATCH",
            "export is not the unique deterministic projection of its bound frozen release",
            "Quarantine the export and rebuild it from the exact source release.",
        ));
    }
    Ok(CorpusExportVerificationReport {
        manifest_hash: observed.manifest_hash,
        source_release_manifest_hash: source.manifest_hash,
        policy: observed.manifest.curation.policy,
        packet_sha256: observed.manifest.outputs.packet_sha256,
        member_count: observed.manifest.members.len(),
        total_member_bytes: observed
            .manifest
            .members
            .iter()
            .map(|member| member.byte_size)
            .sum(),
        database_independent: true,
        inventory_verified: true,
        hashes_verified: true,
        schemas_verified: true,
        bindings_verified: true,
        deterministic_reprojection_verified: true,
    })
}

fn verify_source_release(
    bundle_dir: &Path,
    expected_manifest_hash: &str,
) -> Result<ReleaseIntegrity, AppError> {
    require_hash(
        expected_manifest_hash,
        "MCL_CORPUS_EXPORT_SOURCE_HASH_INVALID",
        "expected source release manifest hash",
    )?;
    let source = crate::release::verify_release_bundle_integrity(bundle_dir)?;
    if source.manifest_hash != expected_manifest_hash {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_SOURCE_HASH_MISMATCH",
            format!(
                "source release manifest hash {} differs from expected {expected_manifest_hash}",
                source.manifest_hash
            ),
            "Quarantine the substituted release and restore the exact trusted source bundle.",
        ));
    }
    Ok(source)
}

fn policy_for(profile: ReleaseProfile) -> CorpusExportPolicy {
    match profile {
        ReleaseProfile::Private => CorpusExportPolicy::PrivateAuditOnly,
        ReleaseProfile::Public => CorpusExportPolicy::Quarantined,
    }
}

fn project_release(
    source: &ReleaseIntegrity,
    curation: &CorpusExportCuration,
) -> Result<Projection, AppError> {
    verify_pinned_contract()?;
    if curation.policy != policy_for(source.manifest.profile) {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_POLICY_MISMATCH",
            "requested corpus policy does not match the frozen release profile",
            "Derive private_audit_only from private releases and quarantined from public releases.",
        ));
    }

    let formalization_reference = source.manifest.publication.subject.clone();
    let (formalization_record, formalization): (RecordSnapshot, FormalizationPayload) =
        release_record(source, RecordKind::Formalization, &formalization_reference)?;
    if formalization.environment_hash != source.manifest.publication.environment_hash
        || formalization.module_artifact_hash != source.manifest.publication.module_artifact_hash
        || formalization.declaration_name != source.manifest.publication.declaration_name
    {
        return Err(binding_error(
            "formalization differs from the source release publication binding",
        ));
    }
    let (claim_record, claim): (RecordSnapshot, ClaimPayload) =
        release_record(source, RecordKind::Claim, &formalization.claim_version)?;
    let (source_record, source_payload): (RecordSnapshot, SourcePayload) =
        release_record(source, RecordKind::Source, &claim.source_reference)?;

    if source.manifest.profile == ReleaseProfile::Public
        && (source_payload.redistribution_status != RedistributionStatus::Allowed
            || source_payload.redaction_class != RedactionClass::Public
            || source_payload.license_expression.is_none())
    {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_PUBLIC_POLICY_BLOCKED",
            "public corpus projection lacks explicit public source redistribution and license authority",
            "Use a private release or resolve source redistribution, redaction, and license policy first.",
        ));
    }

    let receipt: PublicationIngestionReceiptSnapshot = decode_canonical(
        required(source, "reports/publication-receipt.json")?,
        "source publication receipt",
    )?;
    if receipt.receipt_hash != source.manifest.publication.ingestion_receipt_hash {
        return Err(binding_error(
            "publication receipt differs from the source release manifest",
        ));
    }
    let environment: EnvironmentSnapshot = decode_canonical(
        required(source, &source.manifest.replay.environment_path)?,
        "source replay environment",
    )?;
    if environment.environment_hash != source.manifest.publication.environment_hash {
        return Err(binding_error(
            "replay environment differs from the source release manifest",
        ));
    }
    let source_module = required(source, &source.manifest.replay.module_path)?;
    let module_bytes = normalize_file_bytes(source_module);
    let module_sha256 = sha256(&module_bytes);
    let created_at = unix_timestamp_rfc3339(receipt.created_at)?;
    let mathlib_rev = environment
        .manifest
        .dependencies
        .iter()
        .find(|dependency| dependency.name.eq_ignore_ascii_case("mathlib"))
        .map(|dependency| dependency.revision.clone())
        .unwrap_or_else(|| "not_applicable".to_owned());
    let imports = if formalization.import_manifest.is_empty() {
        vec!["Init".to_owned()]
    } else {
        formalization.import_manifest.clone()
    };
    let toolchain = json!({
        "lean_version": environment.manifest.lean_toolchain,
        "mathlib_rev": mathlib_rev,
    });
    let formal_statement_pp = format!(
        "theorem {} : {}",
        formalization.declaration_name, formalization.exact_theorem_type
    );
    let formal_statement_sha256 = value_hash(&json!({
        "theorem_name": formalization.declaration_name,
        "formal_statement_pp": formal_statement_pp,
        "toolchain": toolchain,
    }))?;
    let root_statement_sha256 = sha256(formalization.exact_theorem_type.as_bytes());
    let import_manifest_hash = value_hash(
        &serde_json::to_value(&formalization.import_manifest)
            .map_err(|error| serialization_error("formalization import manifest", error))?,
    )?;
    let packet = build_packet(
        source,
        curation,
        &source_record,
        &source_payload,
        &claim_record,
        &claim,
        &formalization_record,
        &formalization,
        &created_at,
        &imports,
        &toolchain,
        &formal_statement_pp,
        &formal_statement_sha256,
        &root_statement_sha256,
        &import_manifest_hash,
        &module_sha256,
    )?;
    validate_packet(&packet)?;
    let packet_sha256 = packet
        .pointer("/hashes/packet_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| binding_error("projected packet omits its canonical identity"))?
        .to_owned();
    let packet_bytes = canonical_json(&packet)?;

    let mcip = build_mcip_bundle(
        source,
        curation,
        &formalization,
        &created_at,
        &toolchain,
        &formal_statement_sha256,
        &root_statement_sha256,
        &module_sha256,
    )?;
    validate_mcip(&mcip, curation, &formalization.environment_hash)?;
    let mcip_bytes = canonical_json(&mcip)?;
    let mcip_bundle_sha256 = sha256(&mcip_bytes);

    let source_manifest_bytes = canonical_bytes(&source.manifest, "source release manifest")?;
    if sha256(&source_manifest_bytes) != source.manifest_hash {
        return Err(binding_error(
            "source release manifest bytes changed during projection",
        ));
    }
    let sensitive_restriction = match curation.policy {
        CorpusExportPolicy::PrivateAuditOnly => ArtifactRestriction::Private,
        CorpusExportPolicy::Quarantined => ArtifactRestriction::Public,
    };
    let generated_license = match curation.policy {
        CorpusExportPolicy::PrivateAuditOnly => None,
        CorpusExportPolicy::Quarantined => Some(GENERATED_RELEASE_LICENSE.to_owned()),
    };
    let source_module_member = source
        .manifest
        .members
        .iter()
        .find(|member| member.path == source.manifest.replay.module_path)
        .ok_or_else(|| binding_error("source release omits its replay module member"))?;
    let module_license = match curation.policy {
        CorpusExportPolicy::PrivateAuditOnly => None,
        CorpusExportPolicy::Quarantined => source_module_member.license_expression.clone(),
    };
    if curation.policy == CorpusExportPolicy::Quarantined && module_license.is_none() {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_PUBLIC_POLICY_BLOCKED",
            "public corpus projection has no resolved Lean module license",
            "Resolve the source module license before creating a public projection.",
        ));
    }

    let mut files = BTreeMap::new();
    insert_file(
        &mut files,
        "lean/Submission.lean",
        module_bytes,
        CorpusExportMemberKind::LeanModule,
        module_license,
        sensitive_restriction,
    );
    insert_file(
        &mut files,
        "licenses/mathcorpus-apache-2.0.txt",
        UPSTREAM_LICENSE.to_vec(),
        CorpusExportMemberKind::License,
        Some("Apache-2.0".to_owned()),
        ArtifactRestriction::Public,
    );
    insert_file(
        &mut files,
        "mathcorpus/packet.json",
        packet_bytes,
        CorpusExportMemberKind::MathcorpusPacket,
        generated_license.clone(),
        sensitive_restriction,
    );
    insert_file(
        &mut files,
        "mcip/bundle.json",
        mcip_bytes,
        CorpusExportMemberKind::McipBundle,
        generated_license.clone(),
        sensitive_restriction,
    );
    insert_pinned_schema_files(&mut files);
    insert_file(
        &mut files,
        "source-release/manifest.json",
        source_manifest_bytes,
        CorpusExportMemberKind::SourceReleaseManifest,
        generated_license,
        sensitive_restriction,
    );

    let members = files
        .iter()
        .map(|(path, file)| file.member(path.clone()))
        .collect::<Vec<_>>();
    let manifest = CorpusExportManifest {
        schema_version: crate::domain::CORPUS_EXPORT_MANIFEST_SCHEMA_VERSION.to_owned(),
        source_release: CorpusExportSourceBinding {
            release_manifest_hash: source.manifest_hash.clone(),
            release_profile: source.manifest.profile,
            publication_receipt_hash: source.manifest.publication.ingestion_receipt_hash.clone(),
            authority_evidence_id: source.manifest.publication.authority_evidence_id.clone(),
            authority_evidence_hash: source.manifest.publication.authority_evidence_hash.clone(),
            fidelity_evidence_id: source.manifest.publication.fidelity_evidence_id.clone(),
            fidelity_evidence_hash: source.manifest.publication.fidelity_evidence_hash.clone(),
            environment_hash: source.manifest.publication.environment_hash.clone(),
            module_artifact_hash: source.manifest.publication.module_artifact_hash.clone(),
            declaration_name: source.manifest.publication.declaration_name.clone(),
            source: exact_reference(&source_record),
            claim: exact_reference(&claim_record),
            formalization: exact_reference(&formalization_record),
        },
        curation: curation.clone(),
        upstream: upstream_binding(),
        outputs: CorpusExportOutputBinding {
            packet_path: "mathcorpus/packet.json".to_owned(),
            packet_sha256,
            mcip_bundle_path: "mcip/bundle.json".to_owned(),
            mcip_bundle_sha256,
            module_path: "lean/Submission.lean".to_owned(),
            module_sha256,
        },
        members,
    };
    manifest.validate()?;
    Ok(Projection { manifest, files })
}

#[allow(clippy::too_many_arguments)]
fn build_packet(
    release: &ReleaseIntegrity,
    curation: &CorpusExportCuration,
    source_record: &RecordSnapshot,
    source: &SourcePayload,
    claim_record: &RecordSnapshot,
    claim: &ClaimPayload,
    formalization_record: &RecordSnapshot,
    formalization: &FormalizationPayload,
    created_at: &str,
    imports: &[String],
    toolchain: &Value,
    formal_statement_pp: &str,
    formal_statement_sha256: &str,
    root_statement_sha256: &str,
    import_manifest_hash: &str,
    module_sha256: &str,
) -> Result<Value, AppError> {
    let private = curation.policy == CorpusExportPolicy::PrivateAuditOnly;
    let source_kind = if private {
        "private_audit"
    } else if matches!(
        source.source_type,
        crate::domain::schemas::SourceType::Repository
    ) {
        "imported_open_repo"
    } else {
        "adapted_public_source"
    };
    let mut provenance = json!({
        "source_kind": source_kind,
        "source_refs": [
            format!("mathos:{}@{}", source_record.object_id, source_record.version_hash),
            source.canonical_locator,
        ],
        "authors": source.authors_or_origin,
        "statement_fidelity": "adapted_with_review",
        "provenance_attested_by": release.manifest.publication.authority_evidence_id,
        "provenance_checked_at": created_at,
    });
    if !private {
        provenance
            .as_object_mut()
            .expect("provenance object")
            .insert(
                "license_spdx".to_owned(),
                Value::String(
                    source
                        .license_expression
                        .clone()
                        .expect("public policy checked license"),
                ),
            );
    }
    let training = if private {
        json!({
            "eligibility": "private_audit_only",
            "split": "private_audit_only",
            "reason_codes": ["restricted_source", "private_release", "reviewed_training_ineligible"],
            "contamination_risk": "high",
            "can_export_proof_body": false,
        })
    } else {
        json!({
            "eligibility": "quarantined",
            "split": "quarantined",
            "reason_codes": ["leakage_split_not_assigned", "reviewed_but_not_training_eligible"],
            "contamination_risk": "high",
            "can_export_proof_body": true,
        })
    };
    let proof_variant_id = format!("{}.variant.canonical", curation.packet_id);
    let dependency_manifest = json!({
        "environment_hash": formalization.environment_hash,
        "declared_theorem_deps": [],
        "used_theorem_deps": [],
        "obligation_deps": [],
        "verified_module_item_deps": [],
        "transitive_dependency_count": null,
        "transitive_dependency_depth": null,
        "retrieval_candidates": [],
        "retrieved_unused_candidates": [],
        "claim_sources": [],
    });
    let artifact_inventory = if private {
        json!({"private_artifact_inventory": ["lean/Submission.lean", "source-release/manifest.json"]})
    } else {
        json!({"public_files": ["lean/Submission.lean", "source-release/manifest.json"]})
    };
    let mut hashes = Map::new();
    if let Some(source_hash) = &source.content_hash {
        hashes.insert(
            "source_sha256".to_owned(),
            Value::String(source_hash.clone()),
        );
    }
    hashes.insert(
        "formal_statement_sha256".to_owned(),
        Value::String(formal_statement_sha256.to_owned()),
    );
    hashes.insert(
        "proof_body_sha256".to_owned(),
        Value::String(module_sha256.to_owned()),
    );
    hashes.insert(
        "module_sha256".to_owned(),
        Value::String(module_sha256.to_owned()),
    );
    if private {
        hashes.insert(
            "private_artifact_bundle_sha256".to_owned(),
            Value::String(release.manifest_hash.clone()),
        );
    }
    let mut packet = json!({
        "packet_id": curation.packet_id,
        "packet_version": "1.0.0",
        "title": claim.normalized_informal_statement,
        "domain": curation.domain,
        "level": curation.level,
        "kind": "theorem",
        "status": "kernel_verified",
        "difficulty_bin": curation.difficulty_bin,
        "lean_module": "Submission",
        "theorem_name": formalization.declaration_name,
        "imports": imports,
        "toolchain": toolchain,
        "source_provenance": provenance,
        "informal_statement": claim.normalized_informal_statement,
        "formal_statement_pp": formal_statement_pp,
        "proof_body_path": "lean/Submission.lean",
        "proof_body_redacted": false,
        "trust": {
            "rung": 1,
            "proof_authority": "lean_kernel",
            "encoding_required": false,
            "encoding_soundness_status": "stated_directly",
            "independent_review_status": "repo_reviewed",
            "public_claim_class": if private { "private_only" } else { "public_safe" },
        },
        "training": training,
        "proof_variants": [{
            "variant_id": proof_variant_id,
            "variant_style": "canonical",
            "formal_statement_sha256": formal_statement_sha256,
            "environment_hash": formalization.environment_hash,
            "proof_body_path": "lean/Submission.lean",
            "proof_body_sha256": module_sha256,
            "proof_body_redacted": false,
            "source": "imported",
        }],
        "dependency_manifest": dependency_manifest,
        "artifacts": artifact_inventory,
        "verification": {
            "verifier": "MathOS publication boundary",
            "environment_hash": formalization.environment_hash,
            "root_statement_sha256": root_statement_sha256,
            "outcome": "kernel_verified",
            "kernel_verified": true,
            "fidelity_status": "verified",
            "import_manifest_hash": import_manifest_hash,
            "verified_at": created_at,
        },
        "notes": format!(
            "Deterministic MathOS projection of source {}, claim {}, and formalization {}; MCIP evidence remains subordinate to the frozen release.",
            source_record.object_id, claim_record.object_id, formalization_record.object_id
        ),
        "hashes": Value::Object(hashes),
    });
    let packet_hash = packet_hash(&packet)?;
    packet
        .pointer_mut("/hashes")
        .and_then(Value::as_object_mut)
        .expect("packet hashes object")
        .insert("packet_sha256".to_owned(), Value::String(packet_hash));
    Ok(packet)
}

#[allow(clippy::too_many_arguments)]
fn build_mcip_bundle(
    release: &ReleaseIntegrity,
    curation: &CorpusExportCuration,
    formalization: &FormalizationPayload,
    created_at: &str,
    toolchain: &Value,
    _formal_statement_sha256: &str,
    root_statement_sha256: &str,
    module_sha256: &str,
) -> Result<Value, AppError> {
    let export_eligibility = match curation.policy {
        CorpusExportPolicy::PrivateAuditOnly => "private_only",
        CorpusExportPolicy::Quarantined => "quarantined",
    };
    let proof_variant_id = format!("{}.variant.canonical", curation.packet_id);
    let artifact_hashes = json!({
        "mathos_release_manifest": release.manifest_hash,
        "mathos_publication_receipt": release.manifest.publication.ingestion_receipt_hash,
        "mathos_authority_evidence": release.manifest.publication.authority_evidence_hash,
        "mathos_fidelity_evidence": release.manifest.publication.fidelity_evidence_hash,
        "mathos_source_module_artifact": release.manifest.publication.module_artifact_hash,
        "normalized_lean_module": module_sha256,
    });
    let mut packet_identity = json!({
        "schema_version": "1.0.0",
        "record_type": "packet_identity",
        "record_id": format!("{}.packet_identity", curation.packet_id),
        "packet_id": curation.packet_id,
        "packet_version": "1.0.0",
        "formal_statement_sha256": root_statement_sha256,
        "environment_hash": formalization.environment_hash,
        "artifact_hashes": artifact_hashes,
        "created_at": created_at,
        "trust_status": "kernel_verified",
        "export_eligibility": export_eligibility,
        "lean_module": "Submission",
        "theorem_name": formalization.declaration_name,
        "toolchain": toolchain,
        "status": "kernel_verified",
    });
    insert_record_hash(&mut packet_identity)?;

    let mut proof_variant = json!({
        "schema_version": "1.0.0",
        "record_type": "proof_variant",
        "record_id": proof_variant_id,
        "packet_id": curation.packet_id,
        "environment_hash": formalization.environment_hash,
        "artifact_hashes": artifact_hashes,
        "created_at": created_at,
        "trust_status": "kernel_verified",
        "export_eligibility": export_eligibility,
        "formal_statement_sha256": root_statement_sha256,
        "variant_style": "canonical",
        "proof_body_sha256": module_sha256,
        "proof_body_redacted": false,
        "source": "imported",
    });
    insert_record_hash(&mut proof_variant)?;

    let mut dependency_manifest = json!({
        "schema_version": "1.0.0",
        "record_type": "dependency_manifest",
        "record_id": format!("{}.dependency_manifest", curation.packet_id),
        "packet_id": curation.packet_id,
        "environment_hash": formalization.environment_hash,
        "artifact_hashes": artifact_hashes,
        "created_at": created_at,
        "trust_status": "kernel_verified",
        "export_eligibility": export_eligibility,
        "proof_variant_id": format!("{}.variant.canonical", curation.packet_id),
        "declared_theorem_deps": [],
        "used_theorem_deps": [],
        "obligation_deps": [],
        "verified_module_item_deps": [],
        "transitive_dependency_count": null,
        "transitive_dependency_depth": null,
        "retrieval_candidates": [],
        "retrieved_unused_candidates": [],
        "kits": [],
        "tactic_tags": [],
        "prerequisite_concepts": [],
        "claim_sources": [],
    });
    insert_record_hash(&mut dependency_manifest)?;

    Ok(json!({
        "mcip_version": "1.0.0",
        "bundle_id": format!("{}.mathos_export", curation.packet_id),
        "created_at": created_at,
        "producer": "MathOS",
        "records": [packet_identity, proof_variant, dependency_manifest],
    }))
}

fn validate_packet(packet: &Value) -> Result<(), AppError> {
    validate_schema(packet, PACKET_SCHEMA, "MathCorpus packet")?;
    let stored = packet
        .pointer("/hashes/packet_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| schema_error("MathCorpus packet omits hashes.packet_sha256"))?;
    let expected = packet_hash(packet)?;
    if stored != expected {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_PACKET_HASH_MISMATCH",
            format!("MathCorpus packet hash {stored} differs from recomputed {expected}"),
            "Restore the exact canonical packet or rebuild the projection.",
        ));
    }
    let formal = packet
        .pointer("/hashes/formal_statement_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| schema_error("MathCorpus packet omits its formal statement hash"))?;
    let expected_formal = value_hash(&json!({
        "theorem_name": packet.get("theorem_name"),
        "formal_statement_pp": packet.get("formal_statement_pp"),
        "toolchain": packet.get("toolchain"),
    }))?;
    if formal != expected_formal {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_STATEMENT_HASH_MISMATCH",
            "MathCorpus formal statement identity does not match its canonical fields",
            "Restore the exact packet or rebuild it from the frozen formalization.",
        ));
    }
    Ok(())
}

fn validate_mcip(
    bundle: &Value,
    curation: &CorpusExportCuration,
    environment_hash: &str,
) -> Result<(), AppError> {
    validate_mcip_schema(bundle, MCIP_BUNDLE_SCHEMA, "MCIP bundle")?;
    let records = bundle
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| schema_error("MCIP bundle records are absent"))?;
    if records.len() != 3 {
        return Err(schema_error(
            "MathOS MCIP v1 bundle must contain exactly PacketIdentity, ProofVariant, and DependencyManifest",
        ));
    }
    let expected_export = match curation.policy {
        CorpusExportPolicy::PrivateAuditOnly => "private_only",
        CorpusExportPolicy::Quarantined => "quarantined",
    };
    let mut record_types = BTreeSet::new();
    for record in records {
        let record_type = record
            .get("record_type")
            .and_then(Value::as_str)
            .ok_or_else(|| schema_error("MCIP record omits record_type"))?;
        let schema = match record_type {
            "packet_identity" => MCIP_PACKET_IDENTITY_SCHEMA,
            "proof_variant" => MCIP_PROOF_VARIANT_SCHEMA,
            "dependency_manifest" => MCIP_DEPENDENCY_MANIFEST_SCHEMA,
            _ => {
                return Err(schema_error(format!(
                    "unsupported MCIP record type `{record_type}` in MathOS projection"
                )));
            }
        };
        if !record_types.insert(record_type) {
            return Err(schema_error(format!(
                "duplicate MCIP record type `{record_type}`"
            )));
        }
        validate_mcip_schema(record, schema, record_type)?;
        let stored_hash = record
            .get("record_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| schema_error("MCIP record omits record_hash"))?;
        let expected_hash = record_hash(record)?;
        if stored_hash != expected_hash {
            return Err(export_error(
                "MCL_CORPUS_EXPORT_MCIP_HASH_MISMATCH",
                format!("MCIP {record_type} hash differs from its canonical record"),
                "Restore the exact MCIP record or rebuild the projection.",
            ));
        }
        if record.get("packet_id").and_then(Value::as_str) != Some(&curation.packet_id)
            || record.get("environment_hash").and_then(Value::as_str) != Some(environment_hash)
            || record.get("export_eligibility").and_then(Value::as_str) != Some(expected_export)
        {
            return Err(binding_error(
                "MCIP record packet, environment, or export policy differs from its manifest",
            ));
        }
    }
    if record_types
        != ["dependency_manifest", "packet_identity", "proof_variant"]
            .into_iter()
            .collect()
    {
        return Err(schema_error("MCIP bundle has the wrong closed record set"));
    }
    Ok(())
}

fn packet_hash(packet: &Value) -> Result<String, AppError> {
    let mut clone = packet.clone();
    if let Some(hashes) = clone.get_mut("hashes").and_then(Value::as_object_mut) {
        hashes.remove("packet_sha256");
    }
    value_hash(&clone)
}

fn record_hash(record: &Value) -> Result<String, AppError> {
    let mut clone = record.clone();
    clone
        .as_object_mut()
        .ok_or_else(|| schema_error("MCIP record is not an object"))?
        .remove("record_hash");
    value_hash(&clone)
}

fn insert_record_hash(record: &mut Value) -> Result<(), AppError> {
    let hash = record_hash(record)?;
    record
        .as_object_mut()
        .expect("MCIP record object")
        .insert("record_hash".to_owned(), Value::String(hash));
    Ok(())
}

fn validate_projection_files(projection: &Projection) -> Result<(), AppError> {
    projection.manifest.validate()?;
    validate_manifest_schema(&projection.manifest)?;
    let expected = projection
        .manifest
        .members
        .iter()
        .map(|member| member.path.as_str())
        .collect::<BTreeSet<_>>();
    if expected != projection.files.keys().map(String::as_str).collect() {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_BUILD_INVENTORY_MISMATCH",
            "projected files differ from the closed manifest inventory",
            "Rebuild the manifest and files from the same frozen release.",
        ));
    }
    for member in &projection.manifest.members {
        let file = projection.files.get(&member.path).expect("inventory equal");
        if file.member(member.path.clone()) != *member {
            return Err(export_error(
                "MCL_CORPUS_EXPORT_BUILD_MEMBER_MISMATCH",
                format!(
                    "projected member `{}` changed before materialization",
                    member.path
                ),
                "Retry from unchanged immutable inputs.",
            ));
        }
    }
    Ok(())
}

fn verify_export_integrity(export_dir: &Path) -> Result<VerifiedExport, AppError> {
    let root = require_real_directory(export_dir, "corpus export")?;
    let manifest_bytes = read_real_file(&root.join("manifest.json"), MAX_MANIFEST_BYTES)?;
    let manifest: CorpusExportManifest =
        decode_canonical(&manifest_bytes, "corpus export manifest")?;
    manifest.validate()?;
    validate_manifest_schema(&manifest)?;
    let manifest_hash = sha256(&manifest_bytes);

    let observed_inventory = inventory(&root)?;
    let mut expected_inventory = manifest
        .members
        .iter()
        .map(|member| member.path.clone())
        .collect::<BTreeSet<_>>();
    for member in &manifest.members {
        let components = member.path.split('/').collect::<Vec<_>>();
        for depth in 1..components.len() {
            expected_inventory.insert(format!("{}/", components[..depth].join("/")));
        }
    }
    expected_inventory.insert("manifest.json".to_owned());
    if observed_inventory != expected_inventory {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_INVENTORY_MISMATCH",
            "corpus export tree has missing, extra, or renamed members",
            "Restore the exact manifest-controlled directory without extra files.",
        ));
    }
    let mut files = BTreeMap::new();
    for member in &manifest.members {
        let bytes = read_real_file(&safe_member_path(&root, &member.path)?, MAX_MEMBER_BYTES)?;
        if bytes.len() as u64 != member.byte_size || sha256(&bytes) != member.content_hash {
            return Err(export_error(
                "MCL_CORPUS_EXPORT_MEMBER_HASH_MISMATCH",
                format!(
                    "corpus export member `{}` differs from its manifest",
                    member.path
                ),
                "Quarantine the altered export and restore the exact member bytes.",
            ));
        }
        files.insert(member.path.clone(), bytes);
    }
    verify_embedded_schema_files(&files)?;
    let packet: Value = decode_canonical(
        required_export(&files, "mathcorpus/packet.json")?,
        "MathCorpus packet",
    )?;
    validate_packet(&packet)?;
    if packet
        .pointer("/hashes/packet_sha256")
        .and_then(Value::as_str)
        != Some(manifest.outputs.packet_sha256.as_str())
        || packet.get("packet_id").and_then(Value::as_str)
            != Some(manifest.curation.packet_id.as_str())
        || packet.get("domain")
            != Some(&serde_json::to_value(manifest.curation.domain).expect("domain serializes"))
        || packet.get("level")
            != Some(&serde_json::to_value(manifest.curation.level).expect("level serializes"))
        || packet.get("difficulty_bin")
            != Some(
                &serde_json::to_value(manifest.curation.difficulty_bin)
                    .expect("difficulty serializes"),
            )
    {
        return Err(binding_error(
            "MathCorpus packet identity differs from the export manifest",
        ));
    }
    validate_packet_policy(&packet, manifest.curation.policy)?;

    let mcip: Value =
        decode_canonical(required_export(&files, "mcip/bundle.json")?, "MCIP bundle")?;
    validate_mcip(
        &mcip,
        &manifest.curation,
        &manifest.source_release.environment_hash,
    )?;
    if sha256(required_export(&files, "mcip/bundle.json")?) != manifest.outputs.mcip_bundle_sha256 {
        return Err(binding_error(
            "MCIP bundle identity differs from the export manifest",
        ));
    }

    let module = required_export(&files, "lean/Submission.lean")?;
    if module != normalize_file_bytes(module)
        || std::str::from_utf8(module).is_err()
        || sha256(module) != manifest.outputs.module_sha256
    {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_MODULE_INVALID",
            "exported Lean module is not normalized UTF-8 or differs from its binding",
            "Restore the BOM-free LF-normalized module from the frozen release.",
        ));
    }
    let source_manifest: crate::domain::ReleaseManifest = decode_canonical(
        required_export(&files, "source-release/manifest.json")?,
        "copied source release manifest",
    )?;
    if source_manifest.manifest_hash()? != manifest.source_release.release_manifest_hash
        || source_manifest.profile != manifest.source_release.release_profile
        || source_manifest.publication.ingestion_receipt_hash
            != manifest.source_release.publication_receipt_hash
        || source_manifest.publication.environment_hash != manifest.source_release.environment_hash
        || source_manifest.publication.module_artifact_hash
            != manifest.source_release.module_artifact_hash
    {
        return Err(binding_error(
            "copied source release manifest differs from the corpus export binding",
        ));
    }
    Ok(VerifiedExport {
        manifest,
        manifest_hash,
        files,
    })
}

fn validate_packet_policy(packet: &Value, policy: CorpusExportPolicy) -> Result<(), AppError> {
    let (eligibility, split, public_claim_class, can_export) = match policy {
        CorpusExportPolicy::PrivateAuditOnly => (
            "private_audit_only",
            "private_audit_only",
            "private_only",
            false,
        ),
        CorpusExportPolicy::Quarantined => ("quarantined", "quarantined", "public_safe", true),
    };
    if packet
        .pointer("/training/eligibility")
        .and_then(Value::as_str)
        != Some(eligibility)
        || packet.pointer("/training/split").and_then(Value::as_str) != Some(split)
        || packet
            .pointer("/training/can_export_proof_body")
            .and_then(Value::as_bool)
            != Some(can_export)
        || packet
            .pointer("/trust/public_claim_class")
            .and_then(Value::as_str)
            != Some(public_claim_class)
    {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_POLICY_MISMATCH",
            "MathCorpus packet weakens or contradicts the manifest export policy",
            "Rebuild the packet with private_audit_only or quarantined fail-closed policy.",
        ));
    }
    Ok(())
}

fn validate_schema(instance: &Value, schema_bytes: &[u8], label: &str) -> Result<(), AppError> {
    let schema: Value = serde_json::from_slice(schema_bytes)
        .map_err(|error| schema_error(format!("pinned {label} schema is invalid JSON: {error}")))?;
    let validator = jsonschema::options()
        .with_retriever(PinnedSchemaRetriever::deny())
        .build(&schema)
        .map_err(|error| schema_error(format!("pinned {label} schema cannot compile: {error}")))?;
    validator
        .validate(instance)
        .map_err(|error| schema_error(format!("{label} schema validation failed: {error}")))
}

fn validate_manifest_schema(manifest: &CorpusExportManifest) -> Result<(), AppError> {
    let instance = serde_json::to_value(manifest)
        .map_err(|error| serialization_error("corpus export manifest", error))?;
    let schema = crate::domain::corpus_export_manifest_schema();
    let validator = jsonschema::options()
        .with_retriever(PinnedSchemaRetriever::deny())
        .build(&schema)
        .map_err(|error| {
            schema_error(format!(
                "MathOS corpus export manifest schema cannot compile: {error}"
            ))
        })?;
    validator.validate(&instance).map_err(|error| {
        schema_error(format!(
            "corpus export manifest schema validation failed: {error}"
        ))
    })
}

fn validate_mcip_schema(
    instance: &Value,
    schema_bytes: &[u8],
    label: &str,
) -> Result<(), AppError> {
    let schema: Value = serde_json::from_slice(schema_bytes)
        .map_err(|error| schema_error(format!("pinned {label} schema is invalid JSON: {error}")))?;
    let definitions: Value = serde_json::from_slice(MCIP_DEFS_SCHEMA).map_err(|error| {
        schema_error(format!(
            "pinned MCIP definitions schema is invalid JSON: {error}"
        ))
    })?;
    let validator = jsonschema::options()
        .with_retriever(PinnedSchemaRetriever::mcip(definitions))
        .build(&schema)
        .map_err(|error| schema_error(format!("pinned {label} schema cannot compile: {error}")))?;
    validator
        .validate(instance)
        .map_err(|error| schema_error(format!("{label} schema validation failed: {error}")))
}

#[derive(Clone)]
struct PinnedSchemaRetriever {
    resources: BTreeMap<String, Value>,
}

impl PinnedSchemaRetriever {
    fn deny() -> Self {
        Self {
            resources: BTreeMap::new(),
        }
    }

    fn mcip(definitions: Value) -> Self {
        Self {
            resources: BTreeMap::from([(
                "https://github.com/Mnehmos/mathcorpus/schema/mcip/v1/_defs.schema.json".to_owned(),
                definitions,
            )]),
        }
    }
}

impl Retrieve for PinnedSchemaRetriever {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.resources
            .get(uri.as_str())
            .cloned()
            .ok_or_else(|| format!("unpinned schema retrieval denied: {uri}").into())
    }
}

fn verify_pinned_contract() -> Result<(), AppError> {
    for (label, bytes, expected) in pinned_contract_files() {
        let observed = sha256(bytes);
        if observed != expected {
            return Err(export_error(
                "MCL_CORPUS_EXPORT_UPSTREAM_SCHEMA_MISMATCH",
                format!("vendored {label} hash {observed} differs from pinned {expected}"),
                "Restore the byte-exact schemas from the reviewed upstream commit.",
            ));
        }
    }
    Ok(())
}

fn verify_embedded_schema_files(files: &BTreeMap<String, Vec<u8>>) -> Result<(), AppError> {
    for (path, expected) in schema_output_files() {
        if required_export(files, path)? != expected {
            return Err(export_error(
                "MCL_CORPUS_EXPORT_UPSTREAM_SCHEMA_MISMATCH",
                format!("exported pinned contract `{path}` was substituted"),
                "Restore the exact vendored schema or license bytes.",
            ));
        }
    }
    Ok(())
}

fn pinned_contract_files() -> [(&'static str, &'static [u8], &'static str); 6] {
    [
        (
            "MathCorpus packet schema",
            PACKET_SCHEMA,
            PACKET_SCHEMA_HASH,
        ),
        (
            "MCIP definitions schema",
            MCIP_DEFS_SCHEMA,
            MCIP_DEFS_SCHEMA_HASH,
        ),
        (
            "MCIP bundle schema",
            MCIP_BUNDLE_SCHEMA,
            MCIP_BUNDLE_SCHEMA_HASH,
        ),
        (
            "MCIP PacketIdentity schema",
            MCIP_PACKET_IDENTITY_SCHEMA,
            MCIP_PACKET_IDENTITY_SCHEMA_HASH,
        ),
        (
            "MCIP ProofVariant schema",
            MCIP_PROOF_VARIANT_SCHEMA,
            MCIP_PROOF_VARIANT_SCHEMA_HASH,
        ),
        (
            "MCIP DependencyManifest schema",
            MCIP_DEPENDENCY_MANIFEST_SCHEMA,
            MCIP_DEPENDENCY_MANIFEST_SCHEMA_HASH,
        ),
    ]
}

fn schema_output_files() -> [(&'static str, &'static [u8]); 7] {
    [
        ("licenses/mathcorpus-apache-2.0.txt", UPSTREAM_LICENSE),
        ("schemas/mathcorpus/packet.schema.json", PACKET_SCHEMA),
        ("schemas/mcip/v1/_defs.schema.json", MCIP_DEFS_SCHEMA),
        ("schemas/mcip/v1/bundle.schema.json", MCIP_BUNDLE_SCHEMA),
        (
            "schemas/mcip/v1/dependency_manifest.schema.json",
            MCIP_DEPENDENCY_MANIFEST_SCHEMA,
        ),
        (
            "schemas/mcip/v1/packet_identity.schema.json",
            MCIP_PACKET_IDENTITY_SCHEMA,
        ),
        (
            "schemas/mcip/v1/proof_variant.schema.json",
            MCIP_PROOF_VARIANT_SCHEMA,
        ),
    ]
}

fn insert_pinned_schema_files(files: &mut BTreeMap<String, ExportFile>) {
    for (path, bytes) in schema_output_files().into_iter().skip(1) {
        insert_file(
            files,
            path,
            bytes.to_vec(),
            CorpusExportMemberKind::Schema,
            Some("Apache-2.0".to_owned()),
            ArtifactRestriction::Public,
        );
    }
}

fn upstream_binding() -> CorpusExportUpstreamBinding {
    CorpusExportUpstreamBinding {
        repository: UPSTREAM_REPOSITORY.to_owned(),
        commit_sha: UPSTREAM_COMMIT.to_owned(),
        tree_sha: UPSTREAM_TREE.to_owned(),
        license_expression: "Apache-2.0".to_owned(),
        packet_schema_sha256: PACKET_SCHEMA_HASH.to_owned(),
        mcip_defs_schema_sha256: MCIP_DEFS_SCHEMA_HASH.to_owned(),
        mcip_bundle_schema_sha256: MCIP_BUNDLE_SCHEMA_HASH.to_owned(),
        mcip_packet_identity_schema_sha256: MCIP_PACKET_IDENTITY_SCHEMA_HASH.to_owned(),
        mcip_proof_variant_schema_sha256: MCIP_PROOF_VARIANT_SCHEMA_HASH.to_owned(),
        mcip_dependency_manifest_schema_sha256: MCIP_DEPENDENCY_MANIFEST_SCHEMA_HASH.to_owned(),
    }
}

impl ExportFile {
    fn member(&self, path: String) -> CorpusExportMember {
        CorpusExportMember {
            path,
            kind: self.kind,
            content_hash: sha256(&self.bytes),
            byte_size: self.bytes.len() as u64,
            license_expression: self.license_expression.clone(),
            restriction: self.restriction,
        }
    }
}

fn insert_file(
    files: &mut BTreeMap<String, ExportFile>,
    path: &str,
    bytes: Vec<u8>,
    kind: CorpusExportMemberKind,
    license_expression: Option<String>,
    restriction: ArtifactRestriction,
) {
    assert!(
        files
            .insert(
                path.to_owned(),
                ExportFile {
                    bytes,
                    kind,
                    license_expression,
                    restriction,
                },
            )
            .is_none(),
        "closed export path inserted once"
    );
}

fn release_record<T: DeserializeOwned>(
    release: &ReleaseIntegrity,
    kind: RecordKind,
    reference: &ExactVersionReference,
) -> Result<(RecordSnapshot, T), AppError> {
    let path = format!(
        "objects/{}/{}@{}.json",
        kind.as_str(),
        reference.object_id,
        reference.version_hash
    );
    let bytes = required(release, &path)?;
    let record: RecordSnapshot = decode_canonical(bytes, &format!("source {kind} record"))?;
    if record.kind != kind
        || record.object_id != reference.object_id
        || record.version_hash != reference.version_hash
    {
        return Err(binding_error(format!(
            "source record `{path}` differs from its exact reference"
        )));
    }
    let payload = serde_json::from_value(record.payload.clone()).map_err(|error| {
        export_error(
            "MCL_CORPUS_EXPORT_SOURCE_SCHEMA_INVALID",
            format!("source {kind} payload is invalid: {error}"),
            "Quarantine the source release and restore its exact schema-valid records.",
        )
    })?;
    Ok((record, payload))
}

fn exact_reference(record: &RecordSnapshot) -> ExactVersionReference {
    ExactVersionReference {
        object_id: record.object_id.clone(),
        version_hash: record.version_hash.clone(),
    }
}

fn required<'a>(release: &'a ReleaseIntegrity, path: &str) -> Result<&'a [u8], AppError> {
    release
        .files
        .get(path)
        .map(Vec::as_slice)
        .ok_or_else(|| binding_error(format!("required source release member `{path}` is absent")))
}

fn required_export<'a>(
    files: &'a BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<&'a [u8], AppError> {
    files
        .get(path)
        .map(Vec::as_slice)
        .ok_or_else(|| binding_error(format!("required corpus export member `{path}` is absent")))
}

fn canonical_bytes<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, AppError> {
    let value = serde_json::to_value(value).map_err(|error| serialization_error(label, error))?;
    canonical_json(&value)
}

fn decode_canonical<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    label: &str,
) -> Result<T, AppError> {
    let decoded: T = serde_json::from_slice(bytes).map_err(|error| {
        export_error(
            "MCL_CORPUS_EXPORT_JSON_INVALID",
            format!("{label} is not closed valid JSON: {error}"),
            "Restore the exact canonical JSON member.",
        )
    })?;
    let canonical = canonical_bytes(&decoded, label)?;
    if canonical != bytes {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_JSON_NONCANONICAL",
            format!("{label} is not exact canonical JSON"),
            "Restore compact sorted UTF-8 JSON without unknown fields or whitespace.",
        ));
    }
    Ok(decoded)
}

fn normalize_file_bytes(bytes: &[u8]) -> Vec<u8> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                normalized.push(b'\n');
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
            }
            byte => normalized.push(byte),
        }
        index += 1;
    }
    normalized
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
        return Err(export_error(
            "MCL_CORPUS_EXPORT_OUTPUT_EXISTS",
            format!(
                "corpus export output already exists at {}",
                absolute.display()
            ),
            "Choose a new destination; corpus exports never overwrite paths.",
        ));
    }
    let parent = absolute.parent().ok_or_else(|| {
        export_error(
            "MCL_CORPUS_EXPORT_OUTPUT_UNSAFE",
            "corpus export output has no parent directory",
            "Choose a named directory beneath a real existing parent.",
        )
    })?;
    let parent = require_real_directory(parent, "corpus export output parent")?;
    let name = absolute.file_name().ok_or_else(|| {
        export_error(
            "MCL_CORPUS_EXPORT_OUTPUT_UNSAFE",
            "corpus export output has no plain directory name",
            "Choose a named directory beneath a real existing parent.",
        )
    })?;
    Ok((parent.clone(), parent.join(name)))
}

fn write_new_member(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), AppError> {
    let destination = safe_member_path(root, relative)?;
    let parent = destination.parent().expect("member has parent");
    fs::create_dir_all(parent)
        .map_err(|error| AppError::io("create corpus export member directory", error))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&destination)
        .map_err(|error| AppError::io("create corpus export member", error))?;
    file.write_all(bytes)
        .map_err(|error| AppError::io("write corpus export member", error))?;
    file.sync_all()
        .map_err(|error| AppError::io("sync corpus export member", error))
}

fn require_real_directory(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| AppError::io("inspect directory", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_PATH_UNSAFE",
            format!("{label} is not a real directory"),
            "Use a real directory tree without symbolic links.",
        ));
    }
    path.canonicalize()
        .map_err(|error| AppError::io("canonicalize corpus export directory", error))
}

fn safe_member_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || relative.contains('\\')
    {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_PATH_UNSAFE",
            format!("unsafe corpus export path `{relative}`"),
            "Use manifest-controlled relative paths without traversal or platform separators.",
        ));
    }
    Ok(root.join(path))
}

fn read_real_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io("inspect corpus export member", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_PATH_UNSAFE",
            format!(
                "corpus export member {} is unsafe or oversized",
                path.display()
            ),
            "Restore the exact bounded regular file.",
        ));
    }
    fs::read(path).map_err(|error| AppError::io("read corpus export member", error))
}

fn inventory(root: &Path) -> Result<BTreeSet<String>, AppError> {
    fn visit(
        root: &Path,
        directory: &Path,
        files: &mut BTreeSet<String>,
        entries: &mut usize,
    ) -> Result<(), AppError> {
        for entry in fs::read_dir(directory)
            .map_err(|error| AppError::io("read corpus export directory", error))?
        {
            let entry = entry.map_err(|error| AppError::io("read corpus export entry", error))?;
            *entries += 1;
            if *entries > MAX_TREE_ENTRIES {
                return Err(export_error(
                    "MCL_CORPUS_EXPORT_INVENTORY_MISMATCH",
                    "corpus export tree exceeds its bounded entry count",
                    "Restore the exact closed export tree.",
                ));
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| AppError::io("inspect corpus export entry", error))?;
            if metadata.file_type().is_symlink() {
                return Err(export_error(
                    "MCL_CORPUS_EXPORT_PATH_UNSAFE",
                    "corpus export tree contains a symbolic link",
                    "Use a copied export containing only real directories and files.",
                ));
            }
            if metadata.is_dir() {
                let relative = path.strip_prefix(root).expect("rooted walk");
                let components = relative
                    .components()
                    .map(|component| {
                        let Component::Normal(name) = component else {
                            return Err(binding_error("corpus export inventory path is unsafe"));
                        };
                        name.to_str().map(str::to_owned).ok_or_else(|| {
                            binding_error("corpus export inventory path is not UTF-8")
                        })
                    })
                    .collect::<Result<Vec<_>, AppError>>()?;
                files.insert(format!("{}/", components.join("/")));
                visit(root, &path, files, entries)?;
            } else if metadata.is_file() {
                let relative = path.strip_prefix(root).expect("rooted walk");
                let components = relative
                    .components()
                    .map(|component| {
                        let Component::Normal(name) = component else {
                            return Err(binding_error("corpus export inventory path is unsafe"));
                        };
                        name.to_str().map(str::to_owned).ok_or_else(|| {
                            binding_error("corpus export inventory path is not UTF-8")
                        })
                    })
                    .collect::<Result<Vec<_>, AppError>>()?;
                files.insert(components.join("/"));
            } else {
                return Err(export_error(
                    "MCL_CORPUS_EXPORT_PATH_UNSAFE",
                    "corpus export tree contains a non-file filesystem entry",
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

fn unix_timestamp_rfc3339(timestamp: i64) -> Result<String, AppError> {
    const MAX_TIMESTAMP: i64 = 253_402_300_799;
    if !(0..=MAX_TIMESTAMP).contains(&timestamp) {
        return Err(export_error(
            "MCL_CORPUS_EXPORT_TIMESTAMP_INVALID",
            format!("publication timestamp {timestamp} is outside the supported UTC range"),
            "Restore a publication receipt with a nonnegative timestamp through year 9999.",
        ));
    }
    let days = timestamp / 86_400;
    let seconds = timestamp % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    let second = seconds % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn require_hash(value: &str, code: &'static str, label: &str) -> Result<(), AppError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(export_error(
            code,
            format!("{label} is not a lowercase SHA-256 identity"),
            "Use the manifest hash emitted by the trusted build or publication channel.",
        ));
    }
    Ok(())
}

fn serialization_error(label: &str, error: serde_json::Error) -> AppError {
    export_error(
        "MCL_CORPUS_EXPORT_SERIALIZATION_FAILED",
        format!("{label} cannot be serialized: {error}"),
        "Report this deterministic corpus export serialization defect.",
    )
}

fn schema_error(message: impl Into<String>) -> AppError {
    export_error(
        "MCL_CORPUS_EXPORT_SCHEMA_INVALID",
        message,
        "Quarantine the export and restore data matching the pinned offline schemas.",
    )
}

fn binding_error(message: impl Into<String>) -> AppError {
    export_error(
        "MCL_CORPUS_EXPORT_BINDING_MISMATCH",
        message,
        "Quarantine the export and rebuild it from the exact receipt-bound release.",
    )
}

fn export_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        PublicationOutcome, ReleaseManifest, ReleaseMember, ReleaseMemberKind,
        ReleasePedagogyBinding, ReleasePedagogyMode, ReleasePublicationBinding,
        ReleaseReplayBinding,
    };

    use super::*;

    #[test]
    fn pinned_upstream_contract_is_byte_exact() {
        verify_pinned_contract().expect("pinned schemas");
    }

    #[test]
    fn timestamp_conversion_is_utc_and_deterministic() {
        assert_eq!(
            unix_timestamp_rfc3339(0).expect("epoch"),
            "1970-01-01T00:00:00Z"
        );
        assert_eq!(
            unix_timestamp_rfc3339(951_782_400).expect("leap day"),
            "2000-02-29T00:00:00Z"
        );
        assert_eq!(
            unix_timestamp_rfc3339(253_402_300_799).expect("upper bound"),
            "9999-12-31T23:59:59Z"
        );
        assert!(unix_timestamp_rfc3339(-1).is_err());
    }

    #[test]
    fn source_normalization_matches_upstream_file_hash_rules() {
        assert_eq!(
            normalize_file_bytes(b"\xef\xbb\xbfa\r\nb\rc\n"),
            b"a\nb\nc\n"
        );
    }

    #[test]
    fn official_schemas_accept_the_closed_minimal_shapes() {
        let packet = json!({
            "packet_id": "mathos.test.item.v1",
            "packet_version": "1.0.0",
            "title": "Test",
            "domain": "logic",
            "level": "L1_proof_basics",
            "kind": "theorem",
            "status": "kernel_verified",
            "source_provenance": {
                "source_kind": "private_audit",
                "authors": ["MathOS"],
                "statement_fidelity": "adapted_with_review"
            },
            "proof_body_redacted": false,
            "trust": {"rung": 1, "proof_authority": "lean_kernel"},
            "training": {"eligibility": "private_audit_only"},
            "hashes": {"packet_sha256": "a".repeat(64)}
        });
        validate_schema(&packet, PACKET_SCHEMA, "test packet").expect("packet schema");
    }

    #[test]
    fn private_projection_is_deterministic_and_fail_closed() {
        let source = synthetic_source_release();
        let curation = synthetic_curation();
        let first = project_release(&source, &curation).expect("first projection");
        let second = project_release(&source, &curation).expect("second projection");
        assert_eq!(first.manifest, second.manifest);
        assert_eq!(
            first
                .files
                .iter()
                .map(|(path, file)| (path, &file.bytes))
                .collect::<BTreeMap<_, _>>(),
            second
                .files
                .iter()
                .map(|(path, file)| (path, &file.bytes))
                .collect::<BTreeMap<_, _>>()
        );
        let packet: Value = serde_json::from_slice(
            &first
                .files
                .get("mathcorpus/packet.json")
                .expect("packet")
                .bytes,
        )
        .expect("packet JSON");
        validate_packet_policy(&packet, CorpusExportPolicy::PrivateAuditOnly)
            .expect("private packet policy");
        let bundle: Value =
            serde_json::from_slice(&first.files.get("mcip/bundle.json").expect("MCIP").bytes)
                .expect("MCIP JSON");
        assert!(
            bundle["records"]
                .as_array()
                .expect("records")
                .iter()
                .all(|record| record["export_eligibility"] == "private_only")
        );
    }

    #[test]
    fn public_projection_stays_quarantined_and_requires_source_license_authority() {
        let mut source = synthetic_source_release();
        make_source_release_public(&mut source, true);
        let mut curation = synthetic_curation();
        curation.policy = CorpusExportPolicy::Quarantined;
        let projection = project_release(&source, &curation).expect("public projection");
        let packet: Value = serde_json::from_slice(
            &projection
                .files
                .get("mathcorpus/packet.json")
                .expect("packet")
                .bytes,
        )
        .expect("packet JSON");
        validate_packet_policy(&packet, CorpusExportPolicy::Quarantined)
            .expect("quarantined packet policy");
        assert_eq!(packet["training"]["eligibility"], "quarantined");

        let mut unlicensed = synthetic_source_release();
        make_source_release_public(&mut unlicensed, false);
        assert_eq!(
            project_release(&unlicensed, &curation)
                .expect_err("missing source license blocked")
                .code,
            "MCL_CORPUS_EXPORT_PUBLIC_POLICY_BLOCKED"
        );
    }

    #[test]
    fn integrity_rejects_inventory_hash_and_schema_substitution() {
        let source = synthetic_source_release();
        let curation = synthetic_curation();

        let projection = project_release(&source, &curation).expect("projection");
        let extra_root = tempfile::tempdir().expect("extra root");
        materialize_projection(extra_root.path(), &projection);
        fs::write(extra_root.path().join("extra.txt"), b"substitution").expect("extra file");
        assert_eq!(
            verify_export_integrity(extra_root.path())
                .expect_err("extra file blocked")
                .code,
            "MCL_CORPUS_EXPORT_INVENTORY_MISMATCH"
        );

        let directory_root = tempfile::tempdir().expect("directory root");
        materialize_projection(directory_root.path(), &projection);
        fs::create_dir(directory_root.path().join("empty-extra")).expect("extra directory");
        assert_eq!(
            verify_export_integrity(directory_root.path())
                .expect_err("extra directory blocked")
                .code,
            "MCL_CORPUS_EXPORT_INVENTORY_MISMATCH"
        );

        let mut altered_packet = project_release(&source, &curation).expect("projection");
        let packet_path = "mathcorpus/packet.json";
        let mut packet: Value =
            serde_json::from_slice(&altered_packet.files.get(packet_path).expect("packet").bytes)
                .expect("packet JSON");
        packet["title"] = Value::String("coherent manifest substitution".to_owned());
        altered_packet
            .files
            .get_mut(packet_path)
            .expect("packet")
            .bytes = canonical_json(&packet).expect("altered packet bytes");
        let changed_packet_member = altered_packet
            .files
            .get(packet_path)
            .expect("changed packet")
            .member(packet_path.to_owned());
        *altered_packet
            .manifest
            .members
            .iter_mut()
            .find(|member| member.path == packet_path)
            .expect("packet member") = changed_packet_member;
        let packet_root = tempfile::tempdir().expect("packet root");
        materialize_projection(packet_root.path(), &altered_packet);
        assert_eq!(
            verify_export_integrity(packet_root.path())
                .expect_err("rehashed packet substitution blocked")
                .code,
            "MCL_CORPUS_EXPORT_PACKET_HASH_MISMATCH"
        );

        let mut substituted = project_release(&source, &curation).expect("projection");
        let schema_path = "schemas/mcip/v1/bundle.schema.json";
        substituted
            .files
            .get_mut(schema_path)
            .expect("schema file")
            .bytes = b"{}".to_vec();
        let changed_member = substituted
            .files
            .get(schema_path)
            .expect("changed schema")
            .member(schema_path.to_owned());
        *substituted
            .manifest
            .members
            .iter_mut()
            .find(|member| member.path == schema_path)
            .expect("schema member") = changed_member;
        substituted.manifest.validate().expect("updated manifest");
        let schema_root = tempfile::tempdir().expect("schema root");
        materialize_projection(schema_root.path(), &substituted);
        assert_eq!(
            verify_export_integrity(schema_root.path())
                .expect_err("schema substitution blocked")
                .code,
            "MCL_CORPUS_EXPORT_UPSTREAM_SCHEMA_MISMATCH"
        );
    }

    #[test]
    fn verification_requires_the_trusted_export_manifest_hash() {
        let source = synthetic_source_release();
        let projection = project_release(&source, &synthetic_curation()).expect("projection");
        let root = tempfile::tempdir().expect("export root");
        materialize_projection(root.path(), &projection);
        let observed = sha256(
            &canonical_bytes(&projection.manifest, "test manifest").expect("manifest bytes"),
        );
        let substituted = if observed == "a".repeat(64) {
            "b".repeat(64)
        } else {
            "a".repeat(64)
        };
        assert_eq!(
            verify_export(root.path(), &substituted, Path::new("unread-source"))
                .expect_err("untrusted expected hash blocked")
                .code,
            "MCL_CORPUS_EXPORT_MANIFEST_HASH_MISMATCH"
        );
    }

    #[test]
    fn output_paths_are_immutable() {
        let parent = tempfile::tempdir().expect("parent");
        let output = parent.path().join("already-exists");
        fs::create_dir(&output).expect("existing output");
        assert_eq!(
            resolve_new_output(&output)
                .expect_err("overwrite blocked")
                .code,
            "MCL_CORPUS_EXPORT_OUTPUT_EXISTS"
        );
    }

    fn synthetic_curation() -> CorpusExportCuration {
        CorpusExportCuration {
            packet_id: "mathos.logic.synthetic.v1".to_owned(),
            domain: MathCorpusDomain::Logic,
            level: MathCorpusLevel::L1ProofBasics,
            difficulty_bin: MathCorpusDifficultyBin::D1,
            policy: CorpusExportPolicy::PrivateAuditOnly,
        }
    }

    fn synthetic_source_release() -> ReleaseIntegrity {
        let hash = |character: char| character.to_string().repeat(64);
        let source_id = "00000000-0000-4000-8000-000000000001";
        let claim_id = "00000000-0000-4000-8000-000000000002";
        let formalization_id = "00000000-0000-4000-8000-000000000003";
        let authority_id = "00000000-0000-4000-8000-000000000004";
        let fidelity_id = "00000000-0000-4000-8000-000000000005";
        let source_version = hash('1');
        let claim_version = hash('2');
        let formalization_version = hash('3');
        let environment_hash = hash('e');
        let module = b"namespace Fixture\n\ntheorem theorem : True := by trivial\n\nend Fixture\n";
        let module_hash = sha256(module);
        let receipt_hash = hash('a');

        let publication = ReleasePublicationBinding {
            ingestion_receipt_hash: receipt_hash.clone(),
            authority_evidence_id: authority_id.to_owned(),
            authority_evidence_hash: hash('4'),
            fidelity_evidence_id: fidelity_id.to_owned(),
            fidelity_evidence_hash: hash('5'),
            fidelity_report_artifact_hash: hash('6'),
            stage_hash: hash('7'),
            report_artifact_hash: hash('8'),
            retained_closure_artifact_hash: hash('9'),
            attestation_bundle_artifact_hash: hash('a'),
            raw_verification_hash: hash('b'),
            request_hash: hash('c'),
            policy_hash: hash('d'),
            subject: ExactVersionReference {
                object_id: formalization_id.to_owned(),
                version_hash: formalization_version.clone(),
            },
            outcome: PublicationOutcome::Proof,
            environment_hash: environment_hash.clone(),
            module_artifact_hash: module_hash.clone(),
            declaration_name: "Fixture.theorem".to_owned(),
        };
        let fidelity_path = format!(
            "reports/fidelity/{}@{}.json",
            publication.fidelity_evidence_id, publication.fidelity_evidence_hash
        );
        let mut paths = vec![
            (
                format!("artifacts/{}", hash('f')),
                ReleaseMemberKind::Artifact,
            ),
            ("edges/edge.json".to_owned(), ReleaseMemberKind::Edge),
            (
                "environments/environment.json".to_owned(),
                ReleaseMemberKind::Environment,
            ),
            (
                "evidence/evidence.json".to_owned(),
                ReleaseMemberKind::Evidence,
            ),
            (
                "exports/pedagogy-path.json".to_owned(),
                ReleaseMemberKind::Export,
            ),
            ("licenses/index.json".to_owned(), ReleaseMemberKind::License),
            (
                "objects/source/source.json".to_owned(),
                ReleaseMemberKind::Object,
            ),
            (
                "replay/Submission.lean".to_owned(),
                ReleaseMemberKind::Replay,
            ),
            (
                "replay/environment.json".to_owned(),
                ReleaseMemberKind::Replay,
            ),
            ("replay/replay.json".to_owned(), ReleaseMemberKind::Replay),
            (
                "reports/attestation-bundle.json".to_owned(),
                ReleaseMemberKind::Report,
            ),
            (
                "reports/canonical-attestation-receipt.json".to_owned(),
                ReleaseMemberKind::Report,
            ),
            (
                "reports/publication-receipt.json".to_owned(),
                ReleaseMemberKind::Report,
            ),
            (
                "reports/publication-report.json".to_owned(),
                ReleaseMemberKind::Report,
            ),
            (
                "reports/publication-retained-closure.json".to_owned(),
                ReleaseMemberKind::Report,
            ),
            (
                "reports/publication-stage.json".to_owned(),
                ReleaseMemberKind::Report,
            ),
            (
                "reports/raw-attestation-verification.json".to_owned(),
                ReleaseMemberKind::Report,
            ),
            (fidelity_path, ReleaseMemberKind::Report),
        ];
        paths.sort_by(|left, right| left.0.cmp(&right.0));
        let members = paths
            .into_iter()
            .map(|(path, kind)| {
                let content_hash = if kind == ReleaseMemberKind::Artifact {
                    path.strip_prefix("artifacts/")
                        .expect("artifact path")
                        .to_owned()
                } else if path == "replay/Submission.lean" {
                    module_hash.clone()
                } else {
                    hash('0')
                };
                ReleaseMember {
                    path,
                    kind,
                    content_hash,
                    byte_size: 1,
                    license_expression: None,
                    restriction: ArtifactRestriction::Private,
                    artifact_metadata: None,
                }
            })
            .collect();
        let manifest = ReleaseManifest {
            schema_version: crate::domain::RELEASE_MANIFEST_SCHEMA_VERSION.to_owned(),
            profile: ReleaseProfile::Private,
            publication,
            pedagogy: ReleasePedagogyBinding {
                mode: ReleasePedagogyMode::Prerequisites,
                include_soft: false,
                root: ExactVersionReference {
                    object_id: "00000000-0000-4000-8000-000000000006".to_owned(),
                    version_hash: hash('6'),
                },
                unit_order: vec![ExactVersionReference {
                    object_id: "00000000-0000-4000-8000-000000000006".to_owned(),
                    version_hash: hash('6'),
                }],
                edge_ids: Vec::new(),
            },
            replay: ReleaseReplayBinding {
                module_path: "replay/Submission.lean".to_owned(),
                environment_path: "replay/environment.json".to_owned(),
                declaration_name: "Fixture.theorem".to_owned(),
            },
            members,
        };
        let manifest_hash = manifest.manifest_hash().expect("valid synthetic manifest");

        let source_record = RecordSnapshot {
            object_id: source_id.to_owned(),
            kind: RecordKind::Source,
            version_hash: source_version.clone(),
            schema_version: "source/1".to_owned(),
            payload: json!({
                "source_type": "repository",
                "title_or_label": "Synthetic private source",
                "authors_or_origin": ["MathOS test fixture"],
                "canonical_locator": "fixture:synthetic",
                "acquisition_date": "2026-07-22",
                "license_expression": null,
                "redistribution_status": "restricted",
                "content_hash": module_hash,
                "citation_metadata": {},
                "redaction_class": "private",
                "provenance_notes": "Synthetic closed fixture.",
                "original_text": "True"
            }),
            predecessor_hash: None,
            created_at: 1_700_000_000,
            created_by: "fixture".to_owned(),
        };
        let claim_record = RecordSnapshot {
            object_id: claim_id.to_owned(),
            kind: RecordKind::Claim,
            version_hash: claim_version.clone(),
            schema_version: "claim/1".to_owned(),
            payload: json!({
                "source_reference": {"object_id": source_id, "version_hash": source_version},
                "normalized_informal_statement": "True.",
                "claim_kind": "universal",
                "logical_shape": "True",
                "assumptions": [],
                "variables": [],
                "concept_links": [],
                "source_citations": [],
                "ambiguity_notes": []
            }),
            predecessor_hash: None,
            created_at: 1_700_000_000,
            created_by: "fixture".to_owned(),
        };
        let formalization_record = RecordSnapshot {
            object_id: formalization_id.to_owned(),
            kind: RecordKind::Formalization,
            version_hash: formalization_version.clone(),
            schema_version: "formalization/1".to_owned(),
            payload: json!({
                "claim_version": {"object_id": claim_id, "version_hash": claim_version},
                "formal_system": "lean4",
                "claim_polarity": "claim",
                "environment_hash": environment_hash,
                "module_artifact_hash": module_hash,
                "declaration_name": "Fixture.theorem",
                "exact_theorem_type": "True",
                "declaration_hash": hash('7'),
                "import_manifest": [],
                "formalization_notes": "Synthetic exact theorem.",
                "fidelity_evidence_references": [],
                "verification_evidence_references": []
            }),
            predecessor_hash: None,
            created_at: 1_700_000_000,
            created_by: "fixture".to_owned(),
        };
        let environment = json!({
            "environment_hash": environment_hash,
            "manifest": {
                "schema_version": "environment/1",
                "formal_system": "lean4",
                "lean_toolchain": "leanprover/lean4:v4.32.0",
                "dependencies": [],
                "import_manifest": [],
                "project_configuration_hashes": {},
                "platform": "linux_x86_64",
                "trust_profile": "local",
                "verifier_command": {"executable": "lean", "arguments": ["{module_path}"]},
                "resource_limits": {
                    "timeout_seconds": 120,
                    "max_output_bytes": 1048576,
                    "max_memory_bytes": null,
                    "concurrency": 1
                },
                "network_access": false,
                "working_directory_policy": "temporary_workspace"
            },
            "created_at": 1700000000,
            "created_by": "fixture"
        });
        let receipt = json!({
            "receipt_hash": receipt_hash,
            "stage_hash": hash('7'),
            "verification": {
                "schema_version": "publication_attestation_verification/1",
                "report_content_hash": hash('8'),
                "report_artifact_hash": hash('8'),
                "attestation_bundle_hash": hash('a'),
                "raw_verification_hash": hash('b'),
                "verifier_name": "gh",
                "verifier_version": "2.0.0",
                "verifier_binary_sha256": hash('c'),
                "repository": "Mnehmos/MathOS",
                "signer_workflow": "Mnehmos/MathOS/.github/workflows/publication.yml",
                "certificate_identity": "fixture",
                "source_ref": "refs/heads/main",
                "source_commit_sha": "a".repeat(40),
                "predicate_type": "https://slsa.dev/provenance/v1",
                "self_hosted_runners_denied": true,
                "verified_attestation_count": 1,
                "verified_timestamp_count": 1,
                "authoritative": false
            },
            "raw_verification_byte_size": 1,
            "receipt_byte_size": 1,
            "created_at": 1700000000,
            "created_by": "fixture"
        });
        let mut files = BTreeMap::new();
        for record in [&source_record, &claim_record, &formalization_record] {
            files.insert(
                format!(
                    "objects/{}/{}@{}.json",
                    record.kind.as_str(),
                    record.object_id,
                    record.version_hash
                ),
                canonical_bytes(record, "synthetic record").expect("record bytes"),
            );
        }
        files.insert(
            "reports/publication-receipt.json".to_owned(),
            canonical_json(&receipt).expect("receipt bytes"),
        );
        files.insert(
            "replay/environment.json".to_owned(),
            canonical_json(&environment).expect("environment bytes"),
        );
        files.insert("replay/Submission.lean".to_owned(), module.to_vec());
        ReleaseIntegrity {
            manifest,
            manifest_hash,
            files,
        }
    }

    fn materialize_projection(root: &Path, projection: &Projection) {
        for (path, file) in &projection.files {
            write_new_member(root, path, &file.bytes).expect("write projected member");
        }
        let manifest =
            canonical_bytes(&projection.manifest, "test manifest").expect("manifest bytes");
        write_new_member(root, "manifest.json", &manifest).expect("write manifest");
    }

    fn make_source_release_public(source: &mut ReleaseIntegrity, source_has_license: bool) {
        source.manifest.profile = ReleaseProfile::Public;
        for member in &mut source.manifest.members {
            member.restriction = ArtifactRestriction::Public;
            member.license_expression = Some("Apache-2.0".to_owned());
        }
        let source_path = source
            .files
            .keys()
            .find(|path| path.starts_with("objects/source/"))
            .expect("source path")
            .clone();
        let mut record: RecordSnapshot =
            serde_json::from_slice(source.files.get(&source_path).expect("source record bytes"))
                .expect("source record");
        record.payload["redistribution_status"] = Value::String("allowed".to_owned());
        record.payload["redaction_class"] = Value::String("public".to_owned());
        record.payload["license_expression"] = if source_has_license {
            Value::String("Apache-2.0".to_owned())
        } else {
            Value::Null
        };
        source.files.insert(
            source_path,
            canonical_bytes(&record, "public source record").expect("source bytes"),
        );
        source.manifest_hash = source
            .manifest
            .manifest_hash()
            .expect("valid public source manifest");
    }
}
