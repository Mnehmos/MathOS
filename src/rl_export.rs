use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::canonical::{canonical_json, value_hash};
use crate::domain::schemas::{
    ClaimPayload, ExactVersionReference, FormalizationPayload, LearningUnitPayload,
    LearningUnitReviewState, LearningUnitTrainingStatus, SourcePayload,
};
use crate::domain::{
    ArtifactRestriction, ClaimRepairEdgePayload, CounterexamplePackage, EdgeKind, EdgeSnapshot,
    EvidenceAuthorityClass, EvidenceKind, EvidenceResult, EvidenceSnapshot,
    PublicationIngestionReceiptSnapshot, RL_EXPORT_MANIFEST_SCHEMA_VERSION,
    RL_LEAKAGE_REPORT_SCHEMA_VERSION, RL_TASK_SCHEMA_VERSION, RecordKind, RecordSnapshot,
    ReleaseMember, ReleaseMemberKind, ReleaseProfile, RlExportManifest, RlExportMember,
    RlExportMemberKind, RlExportPlan, RlExportSourceBinding, RlLeakageComponent, RlLeakageReport,
    RlPlanRelease, RlSplit, RlTask, RlTaskEvidenceReference, RlTaskFamily, RlTaskFamilySummary,
    RlTaskPolicy, RlTaskTrust,
};
use crate::error::AppError;
use crate::release::{GENERATED_RELEASE_LICENSE, ReleaseIntegrity};

const PLAN_SCHEMA: &[u8] = include_bytes!("../schemas/release/rl-export-plan-1.schema.json");
const TASK_SCHEMA: &[u8] = include_bytes!("../schemas/release/rl-task-1.schema.json");
const LEAKAGE_SCHEMA: &[u8] = include_bytes!("../schemas/release/rl-leakage-report-1.schema.json");
const MANIFEST_SCHEMA: &[u8] =
    include_bytes!("../schemas/release/rl-export-manifest-1.schema.json");

const MAX_PLAN_BYTES: u64 = 1_048_576;
const MAX_MANIFEST_BYTES: u64 = 4 * 1_048_576;
const MAX_MEMBER_BYTES: u64 = 256 * 1_048_576;
const MAX_TREE_ENTRIES: usize = 16_385;

#[derive(Clone, Debug)]
struct ExportFile {
    bytes: Vec<u8>,
    kind: RlExportMemberKind,
    license_expression: Option<String>,
    restriction: ArtifactRestriction,
}

#[derive(Debug)]
struct Projection {
    manifest: RlExportManifest,
    files: BTreeMap<String, ExportFile>,
}

#[derive(Debug)]
struct VerifiedExport {
    manifest: RlExportManifest,
    manifest_hash: String,
    files: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug)]
struct RecordInfo {
    path: String,
    record: RecordSnapshot,
}

#[derive(Clone, Debug)]
struct EvidenceInfo {
    path: String,
    evidence: EvidenceSnapshot,
}

#[derive(Clone, Debug)]
struct EdgeInfo {
    path: String,
    edge: EdgeSnapshot,
}

#[derive(Debug)]
struct BoundRelease {
    plan: RlPlanRelease,
    source: ReleaseIntegrity,
    records: Vec<RecordInfo>,
    evidence: Vec<EvidenceInfo>,
    edges: Vec<EdgeInfo>,
    leakage_keys: BTreeSet<String>,
    component_id: String,
}

#[derive(Clone, Debug)]
struct ComponentSeed {
    component_id: String,
    split: RlSplit,
    release_ids: Vec<String>,
    leakage_keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RlExportOutcome {
    pub dry_run: bool,
    pub manifest_hash: String,
    pub export_path: PathBuf,
    pub plan_hash: String,
    pub source_release_count: usize,
    pub component_count: usize,
    pub task_count: usize,
    pub member_count: usize,
    pub total_member_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct RlExportVerificationReport {
    pub manifest_hash: String,
    pub plan_hash: String,
    pub source_release_count: usize,
    pub component_count: usize,
    pub task_count: usize,
    pub member_count: usize,
    pub total_member_bytes: u64,
    pub database_independent: bool,
    pub inventory_verified: bool,
    pub hashes_verified: bool,
    pub schemas_verified: bool,
    pub split_isolation_verified: bool,
    pub temporal_policy_verified: bool,
    pub source_releases_verified: bool,
    pub deterministic_reprojection_verified: bool,
}

pub struct RlExportRequest<'a> {
    pub plan_path: &'a Path,
    pub source_root: &'a Path,
    pub output_dir: &'a Path,
    pub dry_run: bool,
}

pub fn export_rl(request: RlExportRequest<'_>) -> Result<RlExportOutcome, AppError> {
    let plan = load_plan(request.plan_path)?;
    let mut releases = load_releases(request.source_root, &plan)?;
    let components = assign_components(&mut releases)?;
    let projection = project(&plan, &releases, &components)?;
    validate_projection(&projection)?;

    let (parent, destination) = resolve_new_output(request.output_dir)?;
    let manifest_bytes = canonical_bytes(&projection.manifest, "RL export manifest")?;
    let manifest_hash = sha256(&manifest_bytes);
    let outcome = RlExportOutcome {
        dry_run: request.dry_run,
        manifest_hash: manifest_hash.clone(),
        export_path: destination.clone(),
        plan_hash: projection.manifest.plan_hash.clone(),
        source_release_count: projection.manifest.source_releases.len(),
        component_count: projection.manifest.component_count as usize,
        task_count: projection.manifest.task_count as usize,
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
        .prefix(".mcl-rl-export-")
        .tempdir_in(&parent)
        .map_err(|error| AppError::io("create RL export staging directory", error))?;
    for (path, file) in &projection.files {
        write_new_member(temporary.path(), path, &file.bytes)?;
    }
    write_new_member(temporary.path(), "manifest.json", &manifest_bytes)?;
    let staged = verify_export_integrity(temporary.path())?;
    let projected_files = projection
        .files
        .iter()
        .map(|(path, file)| (path.clone(), file.bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    if staged.manifest_hash != manifest_hash
        || staged.manifest != projection.manifest
        || staged.files != projected_files
    {
        return Err(rl_error(
            "MCL_RL_EXPORT_STAGING_MISMATCH",
            "staged RL export changed before atomic publication",
            "Quarantine the staging directory and retry from unchanged inputs.",
        ));
    }
    fs::rename(temporary.path(), &destination)
        .map_err(|error| AppError::io("atomically publish RL export directory", error))?;
    Ok(outcome)
}

pub fn verify_rl_export(
    export_dir: &Path,
    expected_manifest_hash: &str,
    plan_path: &Path,
    source_root: &Path,
) -> Result<RlExportVerificationReport, AppError> {
    require_hash(expected_manifest_hash, "expected RL export manifest hash")?;
    let observed = verify_export_integrity(export_dir)?;
    if observed.manifest_hash != expected_manifest_hash {
        return Err(rl_error(
            "MCL_RL_EXPORT_MANIFEST_HASH_MISMATCH",
            format!(
                "RL export manifest hash {} differs from expected {expected_manifest_hash}",
                observed.manifest_hash
            ),
            "Quarantine the substituted export and restore the exact trusted projection.",
        ));
    }
    let plan = load_plan(plan_path)?;
    if plan.plan_hash()? != observed.manifest.plan_hash {
        return Err(binding_error(
            "independent RL plan differs from the export binding",
        ));
    }
    let mut releases = load_releases(source_root, &plan)?;
    let components = assign_components(&mut releases)?;
    let expected = project(&plan, &releases, &components)?;
    let expected_files = expected
        .files
        .iter()
        .map(|(path, file)| (path.clone(), file.bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    if expected.manifest != observed.manifest || expected_files != observed.files {
        return Err(rl_error(
            "MCL_RL_EXPORT_REPROJECTION_MISMATCH",
            "RL export is not the unique deterministic projection of its bound releases and plan",
            "Quarantine the export and rebuild it from the exact plan and frozen releases.",
        ));
    }
    Ok(RlExportVerificationReport {
        manifest_hash: observed.manifest_hash,
        plan_hash: observed.manifest.plan_hash,
        source_release_count: observed.manifest.source_releases.len(),
        component_count: observed.manifest.component_count as usize,
        task_count: observed.manifest.task_count as usize,
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
        split_isolation_verified: true,
        temporal_policy_verified: true,
        source_releases_verified: true,
        deterministic_reprojection_verified: true,
    })
}

fn load_plan(path: &Path) -> Result<RlExportPlan, AppError> {
    let bytes = read_real_file(path, MAX_PLAN_BYTES)?;
    let plan: RlExportPlan = serde_json::from_slice(&bytes).map_err(|error| {
        rl_error(
            "MCL_RL_PLAN_JSON_INVALID",
            format!("RL export plan is not closed valid JSON: {error}"),
            "Use the committed rl_export_plan/1 schema without unknown fields.",
        )
    })?;
    plan.validate()?;
    validate_schema_value(
        &serde_json::to_value(&plan).map_err(serialization_error)?,
        PLAN_SCHEMA,
        "RL export plan",
    )?;
    Ok(plan)
}

fn load_releases(source_root: &Path, plan: &RlExportPlan) -> Result<Vec<BoundRelease>, AppError> {
    let root = require_real_directory(source_root, "RL source root")?;
    let mut releases = Vec::with_capacity(plan.releases.len());
    for entry in &plan.releases {
        let path = safe_member_path(&root, &entry.release_id)?;
        let source = crate::release::verify_release_bundle_integrity(&path)?;
        if source.manifest_hash != entry.expected_manifest_hash {
            return Err(rl_error(
                "MCL_RL_SOURCE_HASH_MISMATCH",
                format!(
                    "source release {} hash {} differs from expected {}",
                    entry.release_id, source.manifest_hash, entry.expected_manifest_hash
                ),
                "Restore the exact trusted frozen release beneath the named source root.",
            ));
        }
        if source.manifest.profile == ReleaseProfile::Private
            && entry.split != RlSplit::HeldOutEvaluation
        {
            return Err(rl_error(
                "MCL_RL_PRIVATE_SPLIT_BLOCKED",
                format!(
                    "private release {} is not held-out evaluation",
                    entry.release_id
                ),
                "Assign private releases only to held_out_evaluation or publish a fully licensed public release.",
            ));
        }
        let receipt: PublicationIngestionReceiptSnapshot = decode_canonical(
            required(&source, "reports/publication-receipt.json")?,
            "source publication receipt",
        )?;
        let observed_date = unix_timestamp_date(receipt.created_at)?;
        if observed_date != entry.published_on {
            return Err(rl_error(
                "MCL_RL_PUBLICATION_DATE_MISMATCH",
                format!(
                    "release {} plan date {} differs from signed receipt date {observed_date}",
                    entry.release_id, entry.published_on
                ),
                "Bind published_on to the exact publication receipt rather than caller-supplied chronology.",
            ));
        }
        let records = read_records(&source)?;
        let evidence = read_evidence(&source)?;
        let edges = read_edges(&source)?;
        let leakage_keys = derive_leakage_keys(entry, &records, &evidence, &edges)?;
        releases.push(BoundRelease {
            plan: entry.clone(),
            source,
            records,
            evidence,
            edges,
            leakage_keys,
            component_id: String::new(),
        });
    }
    Ok(releases)
}

fn read_records(source: &ReleaseIntegrity) -> Result<Vec<RecordInfo>, AppError> {
    let mut records = Vec::new();
    for (path, bytes) in source
        .files
        .iter()
        .filter(|(path, _)| path.starts_with("objects/"))
    {
        records.push(RecordInfo {
            path: path.clone(),
            record: decode_canonical(bytes, "source release record")?,
        });
    }
    Ok(records)
}

fn read_evidence(source: &ReleaseIntegrity) -> Result<Vec<EvidenceInfo>, AppError> {
    let mut evidence = Vec::new();
    for (path, bytes) in source
        .files
        .iter()
        .filter(|(path, _)| path.starts_with("evidence/"))
    {
        evidence.push(EvidenceInfo {
            path: path.clone(),
            evidence: decode_canonical(bytes, "source release evidence")?,
        });
    }
    Ok(evidence)
}

fn read_edges(source: &ReleaseIntegrity) -> Result<Vec<EdgeInfo>, AppError> {
    let mut edges = Vec::new();
    for (path, bytes) in source
        .files
        .iter()
        .filter(|(path, _)| path.starts_with("edges/"))
    {
        edges.push(EdgeInfo {
            path: path.clone(),
            edge: decode_canonical(bytes, "source release edge")?,
        });
    }
    Ok(edges)
}

fn derive_leakage_keys(
    entry: &RlPlanRelease,
    records: &[RecordInfo],
    evidence: &[EvidenceInfo],
    edges: &[EdgeInfo],
) -> Result<BTreeSet<String>, AppError> {
    let mut keys = BTreeSet::new();
    for (dimension, labels) in [
        (
            "dependency",
            &entry.leakage_labels.theorem_dependency_components,
        ),
        (
            "equivalence",
            &entry.leakage_labels.equivalent_formalizations,
        ),
        ("source", &entry.leakage_labels.shared_sources),
        ("certificate", &entry.leakage_labels.certificate_families),
        ("proof", &entry.leakage_labels.proof_variants),
    ] {
        for label in labels {
            keys.insert(hashed_key("declared", dimension, label)?);
        }
    }
    keys.insert(hashed_key(
        "declared",
        "benchmark",
        &entry.benchmark_identity,
    )?);
    for record in records {
        keys.insert(format!(
            "record:{}:{}@{}",
            record.record.kind.as_str(),
            record.record.object_id,
            record.record.version_hash
        ));
        match record.record.kind {
            RecordKind::Source => {
                let payload: SourcePayload = decode_value(&record.record.payload, "source")?;
                keys.insert(hashed_key(
                    "derived",
                    "source-locator",
                    &payload.canonical_locator,
                )?);
                if let Some(hash) = payload.content_hash {
                    keys.insert(format!("derived:source-content:{hash}"));
                }
            }
            RecordKind::Formalization => {
                let payload: FormalizationPayload =
                    decode_value(&record.record.payload, "formalization")?;
                keys.insert(format!(
                    "derived:equivalent-claim:{}@{}",
                    payload.claim_version.object_id, payload.claim_version.version_hash
                ));
                keys.insert(format!(
                    "derived:proof-module:{}",
                    payload.module_artifact_hash
                ));
                keys.insert(format!(
                    "derived:proof-declaration:{}",
                    payload.declaration_hash
                ));
            }
            _ => {}
        }
    }
    for item in evidence {
        let payload = &item.evidence.payload;
        if let Some(run_id) = &payload.producing_run_id {
            keys.insert(format!("derived:certificate-run:{run_id}"));
        }
    }
    for item in edges {
        let edge = &item.edge;
        let source_ref = format!("{}@{}", edge.source_object_id, edge.source_version_hash);
        let target_ref = format!("{}@{}", edge.target_object_id, edge.target_version_hash);
        match edge.kind {
            EdgeKind::LogicDependsOn
            | EdgeKind::LogicEquivalentTo
            | EdgeKind::LogicFormalizes
            | EdgeKind::ResearchRepairs
            | EdgeKind::ResearchReducesTo
            | EdgeKind::ImplementationImports
            | EdgeKind::ImplementationGeneratedFrom => {
                keys.insert(format!("derived:edge-endpoint:{source_ref}"));
                keys.insert(format!("derived:edge-endpoint:{target_ref}"));
            }
            _ => {}
        }
        if edge.kind == EdgeKind::ResearchRepairs {
            let repair: ClaimRepairEdgePayload = decode_value(&edge.payload, "claim repair edge")?;
            keys.insert(format!(
                "derived:certificate-package:{}",
                repair.counterexample_package_artifact_hash
            ));
            keys.insert(format!(
                "derived:certificate-run:{}",
                repair.counterexample_search_run_id
            ));
        }
    }
    Ok(keys)
}

fn assign_components(releases: &mut [BoundRelease]) -> Result<Vec<ComponentSeed>, AppError> {
    let mut parents = (0..releases.len()).collect::<Vec<_>>();
    for left in 0..releases.len() {
        for right in left + 1..releases.len() {
            if releases[left]
                .leakage_keys
                .intersection(&releases[right].leakage_keys)
                .next()
                .is_some()
            {
                union(&mut parents, left, right);
            }
        }
    }
    let mut groups = BTreeMap::<usize, Vec<usize>>::new();
    for index in 0..releases.len() {
        let root = find(&mut parents, index);
        groups.entry(root).or_default().push(index);
    }
    let mut components = Vec::new();
    for indices in groups.values() {
        let splits = indices
            .iter()
            .map(|index| releases[*index].plan.split)
            .collect::<BTreeSet<_>>();
        if splits.len() != 1 {
            let ids = indices
                .iter()
                .map(|index| releases[*index].plan.release_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(rl_error(
                "MCL_RL_SPLIT_LEAKAGE",
                format!("leakage-connected releases cross split boundaries: {ids}"),
                "Assign every release in one dependency/equivalence/source/certificate/proof/benchmark component to one split.",
            ));
        }
        let mut release_ids = indices
            .iter()
            .map(|index| releases[*index].plan.release_id.clone())
            .collect::<Vec<_>>();
        release_ids.sort();
        let mut release_hashes = indices
            .iter()
            .map(|index| releases[*index].source.manifest_hash.clone())
            .collect::<Vec<_>>();
        release_hashes.sort();
        let leakage_keys = indices
            .iter()
            .flat_map(|index| releases[*index].leakage_keys.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let component_hash = value_hash(&json!({
            "release_manifest_hashes": release_hashes,
            "leakage_keys": leakage_keys,
        }))?;
        let component_id = format!("rl_component_{component_hash}");
        for index in indices {
            releases[*index].component_id = component_id.clone();
        }
        components.push(ComponentSeed {
            component_id,
            split: *splits.iter().next().expect("nonempty component"),
            release_ids,
            leakage_keys,
        });
    }
    components.sort_by(|left, right| left.component_id.cmp(&right.component_id));
    Ok(components)
}

#[derive(Clone, Debug)]
struct FormalizationCandidate {
    formalization_info: RecordInfo,
    formalization: FormalizationPayload,
    claim_info: RecordInfo,
    claim: ClaimPayload,
    source_info: RecordInfo,
    fidelity: EvidenceInfo,
    authority: EvidenceInfo,
}

fn project(
    plan: &RlExportPlan,
    releases: &[BoundRelease],
    component_seeds: &[ComponentSeed],
) -> Result<Projection, AppError> {
    let plan_hash = plan.plan_hash()?;
    let has_private = releases
        .iter()
        .any(|release| release.source.manifest.profile == ReleaseProfile::Private);
    let collection_restriction = if has_private {
        ArtifactRestriction::Private
    } else {
        ArtifactRestriction::Public
    };
    let collection_license = (!has_private).then(|| GENERATED_RELEASE_LICENSE.to_owned());
    let mut files = BTreeMap::new();
    insert_file(
        &mut files,
        "plan/plan.json",
        canonical_bytes(plan, "RL export plan")?,
        RlExportMemberKind::Plan,
        collection_license.clone(),
        collection_restriction,
    )?;
    for (path, bytes) in schema_files() {
        insert_file(
            &mut files,
            path,
            bytes.to_vec(),
            RlExportMemberKind::Schema,
            Some(GENERATED_RELEASE_LICENSE.to_owned()),
            ArtifactRestriction::Public,
        )?;
    }
    for release in releases {
        let path = format!("source-releases/{}/manifest.json", release.plan.release_id);
        insert_file(
            &mut files,
            &path,
            canonical_bytes(&release.source.manifest, "source release manifest")?,
            RlExportMemberKind::SourceReleaseManifest,
            (release.source.manifest.profile == ReleaseProfile::Public)
                .then(|| GENERATED_RELEASE_LICENSE.to_owned()),
            profile_restriction(release.source.manifest.profile),
        )?;
    }

    let mut tasks = Vec::new();
    for release in releases {
        project_release_tasks(release, &mut files, &mut tasks)?;
    }
    tasks.sort_by(|left, right| left.task_id.cmp(&right.task_id));
    if tasks
        .windows(2)
        .any(|pair| pair[0].task_id == pair[1].task_id)
    {
        return Err(binding_error(
            "RL task projection produced a duplicate canonical task",
        ));
    }
    for task in &tasks {
        task.validate()?;
        validate_schema_value(
            &serde_json::to_value(task).map_err(serialization_error)?,
            TASK_SCHEMA,
            "RL task",
        )?;
        let path = format!(
            "tasks/{}/{}/{}.json",
            task.split.as_str(),
            task.family.as_str(),
            task.task_id
        );
        insert_file(
            &mut files,
            &path,
            canonical_bytes(task, "RL task")?,
            RlExportMemberKind::Task,
            (task.policy.restriction == ArtifactRestriction::Public)
                .then(|| GENERATED_RELEASE_LICENSE.to_owned()),
            task.policy.restriction,
        )?;
    }

    let mut components = Vec::new();
    for seed in component_seeds {
        let mut task_ids = tasks
            .iter()
            .filter(|task| task.leakage_component_id == seed.component_id)
            .map(|task| task.task_id.clone())
            .collect::<Vec<_>>();
        task_ids.sort();
        components.push(RlLeakageComponent {
            component_id: seed.component_id.clone(),
            split: seed.split,
            release_ids: seed.release_ids.clone(),
            leakage_keys: seed.leakage_keys.clone(),
            task_ids,
        });
    }
    let task_families = RlTaskFamily::ALL
        .into_iter()
        .map(|family| {
            let count = tasks.iter().filter(|task| task.family == family).count() as u64;
            let skip_reason = (count == 0).then(|| family_skip_reason(family).to_owned());
            RlTaskFamilySummary {
                family,
                emitted_task_count: count,
                skip_reason,
            }
        })
        .collect::<Vec<_>>();
    let report = RlLeakageReport {
        schema_version: RL_LEAKAGE_REPORT_SCHEMA_VERSION.to_owned(),
        plan_hash: plan_hash.clone(),
        components,
        task_families,
        cross_split_overlap_count: 0,
        temporal_policy_verified: true,
    };
    report.validate()?;
    validate_schema_value(
        &serde_json::to_value(&report).map_err(serialization_error)?,
        LEAKAGE_SCHEMA,
        "RL leakage report",
    )?;
    let report_bytes = canonical_bytes(&report, "RL leakage report")?;
    let leakage_report_sha256 = sha256(&report_bytes);
    insert_file(
        &mut files,
        "leakage/report.json",
        report_bytes,
        RlExportMemberKind::LeakageReport,
        collection_license,
        collection_restriction,
    )?;

    let source_releases = releases
        .iter()
        .map(|release| RlExportSourceBinding {
            release_id: release.plan.release_id.clone(),
            release_manifest_hash: release.source.manifest_hash.clone(),
            release_profile: release.source.manifest.profile,
            split: release.plan.split,
            published_on: release.plan.published_on.clone(),
            leakage_component_id: release.component_id.clone(),
        })
        .collect::<Vec<_>>();
    let members = files
        .iter()
        .map(|(path, file)| file.member(path.clone()))
        .collect::<Vec<_>>();
    let manifest = RlExportManifest {
        schema_version: RL_EXPORT_MANIFEST_SCHEMA_VERSION.to_owned(),
        plan_hash,
        publication_cutoff: plan.publication_cutoff.clone(),
        source_releases,
        leakage_report_sha256,
        task_count: tasks.len() as u64,
        component_count: component_seeds.len() as u64,
        members,
    };
    manifest.validate()?;
    validate_schema_value(
        &serde_json::to_value(&manifest).map_err(serialization_error)?,
        MANIFEST_SCHEMA,
        "RL export manifest",
    )?;
    Ok(Projection { manifest, files })
}

fn project_release_tasks(
    release: &BoundRelease,
    files: &mut BTreeMap<String, ExportFile>,
    tasks: &mut Vec<RlTask>,
) -> Result<(), AppError> {
    let candidates = formalization_candidates(release)?;
    for candidate in &candidates {
        let common_paths = candidate_paths(release, candidate);
        tasks.push(build_task(
            release,
            RlTaskFamily::Formalization,
            json!({
                "claim": exact_reference(&candidate.claim_info.record),
                "informal_statement": candidate.claim.normalized_informal_statement,
                "logical_shape": candidate.claim.logical_shape,
                "assumptions": candidate.claim.assumptions,
                "source": exact_reference(&candidate.source_info.record),
            }),
            json!({
                "formalization": exact_reference(&candidate.formalization_info.record),
                "formal_system": candidate.formalization.formal_system,
                "claim_polarity": candidate.formalization.claim_polarity,
                "exact_theorem_type": candidate.formalization.exact_theorem_type,
            }),
            &common_paths,
            &candidate.authority,
            &candidate.fidelity,
            files,
        )?);
        tasks.push(build_task(
            release,
            RlTaskFamily::DeclarationRetrieval,
            json!({
                "obligation": candidate.formalization.exact_theorem_type,
                "formal_system": candidate.formalization.formal_system,
                "imports": candidate.formalization.import_manifest,
                "environment_hash": candidate.formalization.environment_hash,
            }),
            json!({
                "declaration_name": candidate.formalization.declaration_name,
                "declaration_hash": candidate.formalization.declaration_hash,
            }),
            &common_paths,
            &candidate.authority,
            &candidate.fidelity,
            files,
        )?);
        tasks.push(build_task(
            release,
            RlTaskFamily::ProofGeneration,
            json!({
                "obligation": candidate.formalization.exact_theorem_type,
                "formal_system": candidate.formalization.formal_system,
                "imports": candidate.formalization.import_manifest,
                "environment_hash": candidate.formalization.environment_hash,
            }),
            json!({
                "accepted_artifact_path": format!("artifacts/{}", candidate.formalization.module_artifact_hash),
                "module_artifact_hash": candidate.formalization.module_artifact_hash,
                "declaration_name": candidate.formalization.declaration_name,
                "declaration_hash": candidate.formalization.declaration_hash,
            }),
            &common_paths,
            &candidate.authority,
            &candidate.fidelity,
            files,
        )?);
    }

    for candidate in &candidates {
        let mut variants = candidates
            .iter()
            .filter(|variant| variant.claim.source_reference == candidate.claim.source_reference)
            .collect::<Vec<_>>();
        variants.sort_by(|left, right| {
            left.formalization_info
                .record
                .version_hash
                .cmp(&right.formalization_info.record.version_hash)
        });
        let mut paths = candidate_paths(release, candidate);
        paths.extend(
            variants
                .iter()
                .flat_map(|variant| {
                    [
                        variant.formalization_info.path.clone(),
                        variant.fidelity.path.clone(),
                    ]
                })
                .collect::<Vec<_>>(),
        );
        let variant_values = variants
            .iter()
            .map(|variant| {
                json!({
                    "formalization": exact_reference(&variant.formalization_info.record),
                    "claim": exact_reference(&variant.claim_info.record),
                    "claim_polarity": variant.formalization.claim_polarity,
                    "exact_theorem_type": variant.formalization.exact_theorem_type,
                })
            })
            .collect::<Vec<_>>();
        tasks.push(build_task(
            release,
            RlTaskFamily::FidelitySelection,
            json!({
                "source_claim": exact_reference(&candidate.claim_info.record),
                "informal_statement": candidate.claim.normalized_informal_statement,
                "variants": variant_values,
            }),
            json!({
                "selected_formalization": exact_reference(&candidate.formalization_info.record),
                "fidelity_evidence_id": candidate.fidelity.evidence.evidence_id,
                "fidelity_evidence_hash": candidate.fidelity.evidence.evidence_hash,
            }),
            &paths,
            &candidate.authority,
            &candidate.fidelity,
            files,
        )?);
    }

    project_counterexample_tasks(release, &candidates, files, tasks)?;
    project_pedagogy_tasks(release, &candidates, files, tasks)?;
    Ok(())
}

fn formalization_candidates(
    release: &BoundRelease,
) -> Result<Vec<FormalizationCandidate>, AppError> {
    let mut candidates = Vec::new();
    for formalization_info in release
        .records
        .iter()
        .filter(|record| record.record.kind == RecordKind::Formalization)
    {
        let formalization: FormalizationPayload =
            decode_value(&formalization_info.record.payload, "formalization")?;
        let reference = exact_reference(&formalization_info.record);
        let Some(fidelity) = select_evidence(
            release,
            &reference,
            EvidenceKind::StatementFidelityReview,
            EvidenceAuthorityClass::Reviewed,
        )?
        else {
            continue;
        };
        let authorities = release
            .evidence
            .iter()
            .filter(|item| {
                item.evidence.payload.subject == reference
                    && matches!(
                        item.evidence.payload.evidence_kind,
                        EvidenceKind::LeanKernelProof | EvidenceKind::LeanKernelRefutation
                    )
                    && item.evidence.payload.result == EvidenceResult::Accepted
                    && item.evidence.payload.authority_class
                        == EvidenceAuthorityClass::Authoritative
                    && !item.evidence.payload.stale
                    && item.evidence.payload.publication_authority.is_some()
            })
            .collect::<Vec<_>>();
        if authorities.is_empty() {
            continue;
        }
        if authorities.len() != 1 {
            return Err(binding_error(
                "formalization has multiple current authoritative publication witnesses",
            ));
        }
        let claim_info =
            record_by_reference(release, RecordKind::Claim, &formalization.claim_version)?;
        let claim: ClaimPayload = decode_value(&claim_info.record.payload, "claim")?;
        let source_info =
            record_by_reference(release, RecordKind::Source, &claim.source_reference)?;
        let _: SourcePayload = decode_value(&source_info.record.payload, "source")?;
        candidates.push(FormalizationCandidate {
            formalization_info: formalization_info.clone(),
            formalization,
            claim_info: claim_info.clone(),
            claim,
            source_info: source_info.clone(),
            fidelity: fidelity.clone(),
            authority: (*authorities[0]).clone(),
        });
    }
    candidates.sort_by(|left, right| {
        left.formalization_info
            .record
            .version_hash
            .cmp(&right.formalization_info.record.version_hash)
    });
    Ok(candidates)
}

fn select_evidence<'a>(
    release: &'a BoundRelease,
    subject: &ExactVersionReference,
    kind: EvidenceKind,
    authority: EvidenceAuthorityClass,
) -> Result<Option<&'a EvidenceInfo>, AppError> {
    let matches = release
        .evidence
        .iter()
        .filter(|item| {
            item.evidence.payload.subject == *subject
                && item.evidence.payload.evidence_kind == kind
                && item.evidence.payload.result == EvidenceResult::Accepted
                && item.evidence.payload.authority_class == authority
                && !item.evidence.payload.stale
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(binding_error(
            "subject has multiple current accepted evidence records for one RL trust role",
        ));
    }
    Ok(matches.into_iter().next())
}

fn candidate_paths(release: &BoundRelease, candidate: &FormalizationCandidate) -> Vec<String> {
    vec![
        candidate.source_info.path.clone(),
        candidate.claim_info.path.clone(),
        candidate.formalization_info.path.clone(),
        candidate.fidelity.path.clone(),
        candidate.authority.path.clone(),
        format!("artifacts/{}", candidate.formalization.module_artifact_hash),
        release.source.manifest.replay.environment_path.clone(),
    ]
}

fn project_counterexample_tasks(
    release: &BoundRelease,
    candidates: &[FormalizationCandidate],
    files: &mut BTreeMap<String, ExportFile>,
    tasks: &mut Vec<RlTask>,
) -> Result<(), AppError> {
    for edge_info in release
        .edges
        .iter()
        .filter(|edge| edge.edge.kind == EdgeKind::ResearchRepairs)
    {
        let repair: ClaimRepairEdgePayload =
            decode_value(&edge_info.edge.payload, "claim repair edge")?;
        let package_path = format!("artifacts/{}", repair.counterexample_package_artifact_hash);
        let package: CounterexamplePackage = decode_canonical(
            required(&release.source, &package_path)?,
            "counterexample package",
        )?;
        package.validate()?;
        if package.search_provenance.run_id != repair.counterexample_search_run_id
            || package.search_provenance.event_head_hash
                != repair.counterexample_search_run_head_hash
            || package.refutation_witness.formalization != repair.refutation_formalization
            || package.proposed_repaired_claim.version_hash != edge_info.edge.source_version_hash
            || package.original_claim.object_id != edge_info.edge.target_object_id
            || package.original_claim.version_hash != edge_info.edge.target_version_hash
        {
            return Err(binding_error(
                "counterexample package differs from its exact repair edge",
            ));
        }
        let Some(candidate) = candidates.iter().find(|candidate| {
            exact_reference(&candidate.formalization_info.record) == repair.refutation_formalization
        }) else {
            return Err(binding_error(
                "repair edge refutation lacks replayed fidelity and authority evidence",
            ));
        };
        let original = record_by_reference(release, RecordKind::Claim, &package.original_claim)?;
        let repaired_reference = ExactVersionReference {
            object_id: edge_info.edge.source_object_id.clone(),
            version_hash: edge_info.edge.source_version_hash.clone(),
        };
        let repaired = record_by_reference(release, RecordKind::Claim, &repaired_reference)?;
        let mut paths = candidate_paths(release, candidate);
        paths.extend([
            edge_info.path.clone(),
            original.path.clone(),
            repaired.path.clone(),
            package_path,
        ]);
        tasks.push(build_task(
            release,
            RlTaskFamily::Counterexample,
            json!({
                "universal_claim": exact_reference(&original.record),
                "claim_payload": original.record.payload,
            }),
            json!({
                "witness": package.witness,
                "checker": package.checker,
                "minimization": package.minimization,
                "refutation_formalization": package.refutation_witness.formalization,
            }),
            &paths,
            &candidate.authority,
            &candidate.fidelity,
            files,
        )?);
        tasks.push(build_task(
            release,
            RlTaskFamily::StatementRepair,
            json!({
                "false_claim": exact_reference(&original.record),
                "claim_payload": original.record.payload,
                "witness": package.witness,
                "failing_assumption_explanation": package.failing_assumption_explanation,
            }),
            json!({
                "repair_operation": package.repair_operation,
                "repaired_claim": exact_reference(&repaired.record),
                "repaired_claim_payload": package.proposed_repaired_claim.payload,
            }),
            &paths,
            &candidate.authority,
            &candidate.fidelity,
            files,
        )?);
    }
    Ok(())
}

fn project_pedagogy_tasks(
    release: &BoundRelease,
    candidates: &[FormalizationCandidate],
    files: &mut BTreeMap<String, ExportFile>,
    tasks: &mut Vec<RlTask>,
) -> Result<(), AppError> {
    let Some(authority) = release.evidence.iter().find(|item| {
        item.evidence.evidence_id == release.source.manifest.publication.authority_evidence_id
    }) else {
        return Err(binding_error("release authority evidence is absent"));
    };
    let Some(fidelity) = release.evidence.iter().find(|item| {
        item.evidence.evidence_id == release.source.manifest.publication.fidelity_evidence_id
    }) else {
        return Err(binding_error("release fidelity evidence is absent"));
    };
    let mut eligible = Vec::new();
    for reference in &release.source.manifest.pedagogy.unit_order {
        let info = record_by_reference(release, RecordKind::LearningUnit, reference)?;
        let payload: LearningUnitPayload = decode_value(&info.record.payload, "learning unit")?;
        let status_ok = match release.source.manifest.profile {
            ReleaseProfile::Public => {
                payload.training_status == LearningUnitTrainingStatus::EligiblePublic
            }
            ReleaseProfile::Private => {
                payload.training_status == LearningUnitTrainingStatus::HeldOutEvaluation
            }
        };
        if payload.review.state == LearningUnitReviewState::Reviewed && status_ok {
            eligible.push((info.clone(), payload));
        }
    }
    for (info, unit) in eligible
        .iter()
        .filter(|(_, unit)| unit.unit_kind == crate::domain::schemas::LearningUnitKind::Explanation)
    {
        let mut paths = vec![
            info.path.clone(),
            format!("artifacts/{}", unit.content_artifact_hash),
        ];
        for reference in &unit.formalization_references {
            if let Ok(record) = record_by_reference(release, RecordKind::Formalization, reference) {
                paths.push(record.path.clone());
            }
        }
        tasks.push(build_task(
            release,
            RlTaskFamily::Explanation,
            json!({
                "target": unit.target,
                "formalization_references": unit.formalization_references,
                "audience_track": unit.audience_track,
                "learning_objectives": unit.learning_objectives,
            }),
            json!({
                "learning_unit": exact_reference(&info.record),
                "content_artifact_path": format!("artifacts/{}", unit.content_artifact_hash),
            }),
            &paths,
            authority,
            fidelity,
            files,
        )?);
    }
    if eligible.len() >= 2 && eligible.len() == release.source.manifest.pedagogy.unit_order.len() {
        let mut paths = eligible
            .iter()
            .flat_map(|(info, unit)| {
                [
                    info.path.clone(),
                    format!("artifacts/{}", unit.content_artifact_hash),
                ]
            })
            .collect::<Vec<_>>();
        for edge_id in &release.source.manifest.pedagogy.edge_ids {
            paths.push(format!("edges/{edge_id}.json"));
        }
        tasks.push(build_task(
            release,
            RlTaskFamily::CurriculumOrdering,
            json!({
                "learning_units": eligible.iter().map(|(info, unit)| json!({
                    "unit": exact_reference(&info.record),
                    "target": unit.target,
                    "learning_objectives": unit.learning_objectives,
                })).collect::<Vec<_>>(),
            }),
            json!({
                "mode": release.source.manifest.pedagogy.mode,
                "include_soft": release.source.manifest.pedagogy.include_soft,
                "root": release.source.manifest.pedagogy.root,
                "unit_order": release.source.manifest.pedagogy.unit_order,
                "edge_ids": release.source.manifest.pedagogy.edge_ids,
            }),
            &paths,
            authority,
            fidelity,
            files,
        )?);
    }
    let _ = candidates;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_task(
    release: &BoundRelease,
    family: RlTaskFamily,
    input: Value,
    target: Value,
    source_paths: &[String],
    authority: &EvidenceInfo,
    fidelity: &EvidenceInfo,
    files: &mut BTreeMap<String, ExportFile>,
) -> Result<RlTask, AppError> {
    let (evidence, policy) = materialize_task_evidence(release, source_paths, files)?;
    let mut authority_evidence_ids = vec![authority.evidence.evidence_id.clone()];
    authority_evidence_ids.sort();
    authority_evidence_ids.dedup();
    let mut fidelity_evidence_ids = vec![fidelity.evidence.evidence_id.clone()];
    fidelity_evidence_ids.sort();
    fidelity_evidence_ids.dedup();
    let mut task = RlTask {
        schema_version: RL_TASK_SCHEMA_VERSION.to_owned(),
        task_id: format!("rl_task_{}", "0".repeat(64)),
        family,
        split: release.plan.split,
        leakage_component_id: release.component_id.clone(),
        input,
        target,
        evidence,
        trust: RlTaskTrust {
            source_release_id: release.plan.release_id.clone(),
            source_release_manifest_hash: release.source.manifest_hash.clone(),
            publication_receipt_hash: release
                .source
                .manifest
                .publication
                .ingestion_receipt_hash
                .clone(),
            authority_evidence_ids,
            fidelity_evidence_ids,
            kernel_verified: true,
        },
        policy,
    };
    task.task_id = task.expected_task_id()?;
    task.validate()?;
    Ok(task)
}

fn materialize_task_evidence(
    release: &BoundRelease,
    source_paths: &[String],
    files: &mut BTreeMap<String, ExportFile>,
) -> Result<(Vec<RlTaskEvidenceReference>, RlTaskPolicy), AppError> {
    let mut paths = source_paths.to_vec();
    paths.sort();
    paths.dedup();
    let manifest_path = format!("source-releases/{}/manifest.json", release.plan.release_id);
    let mut evidence = vec![RlTaskEvidenceReference {
        path: manifest_path,
        content_hash: release.source.manifest_hash.clone(),
    }];
    let mut licenses = BTreeSet::new();
    let mut license_complete = release.source.manifest.profile == ReleaseProfile::Public;
    if release.source.manifest.profile == ReleaseProfile::Public {
        licenses.insert(GENERATED_RELEASE_LICENSE.to_owned());
    }
    for source_path in paths {
        let bytes = required(&release.source, &source_path)?.to_vec();
        let member = source_member(&release.source, &source_path)?;
        let export_path = if member.kind == ReleaseMemberKind::Artifact {
            format!("artifacts/{}", member.content_hash)
        } else {
            format!("source-members/{}/{}", release.plan.release_id, source_path)
        };
        let kind = if member.kind == ReleaseMemberKind::Artifact {
            RlExportMemberKind::Artifact
        } else {
            RlExportMemberKind::SourceMember
        };
        insert_file(
            files,
            &export_path,
            bytes,
            kind,
            member.license_expression.clone(),
            member.restriction,
        )?;
        if let Some(license) = &member.license_expression {
            licenses.insert(license.clone());
        } else {
            license_complete = false;
        }
        evidence.push(RlTaskEvidenceReference {
            path: export_path,
            content_hash: member.content_hash.clone(),
        });
    }
    evidence.sort_by(|left, right| left.path.cmp(&right.path));
    evidence.dedup_by(|left, right| left.path == right.path);
    let restriction = profile_restriction(release.source.manifest.profile);
    Ok((
        evidence,
        RlTaskPolicy {
            restriction,
            license_expressions: licenses.into_iter().collect(),
            license_complete,
        },
    ))
}

fn record_by_reference<'a>(
    release: &'a BoundRelease,
    kind: RecordKind,
    reference: &ExactVersionReference,
) -> Result<&'a RecordInfo, AppError> {
    release
        .records
        .iter()
        .find(|record| {
            record.record.kind == kind
                && record.record.object_id == reference.object_id
                && record.record.version_hash == reference.version_hash
        })
        .ok_or_else(|| {
            binding_error(format!(
                "source release omits exact {} {}@{}",
                kind, reference.object_id, reference.version_hash
            ))
        })
}

fn source_member<'a>(
    source: &'a ReleaseIntegrity,
    path: &str,
) -> Result<&'a ReleaseMember, AppError> {
    source
        .manifest
        .members
        .iter()
        .find(|member| member.path == path)
        .ok_or_else(|| binding_error(format!("source release member `{path}` is unbound")))
}

fn family_skip_reason(family: RlTaskFamily) -> &'static str {
    match family {
        RlTaskFamily::Explanation | RlTaskFamily::CurriculumOrdering => {
            "reviewed_training_eligible_pedagogy_absent"
        }
        RlTaskFamily::Decomposition
        | RlTaskFamily::ProofRepair
        | RlTaskFamily::Generalization
        | RlTaskFamily::FrontierSelection => "not_projected_by_rl_task_v1",
        _ => "required_verified_evidence_absent",
    }
}

impl ExportFile {
    fn member(&self, path: String) -> RlExportMember {
        RlExportMember {
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
    kind: RlExportMemberKind,
    license_expression: Option<String>,
    restriction: ArtifactRestriction,
) -> Result<(), AppError> {
    let file = ExportFile {
        bytes,
        kind,
        license_expression,
        restriction,
    };
    if let Some(existing) = files.get(path) {
        if existing.bytes != file.bytes
            || existing.kind != file.kind
            || existing.license_expression != file.license_expression
            || existing.restriction != file.restriction
        {
            return Err(rl_error(
                "MCL_RL_EXPORT_MEMBER_CONFLICT",
                format!("RL projection maps incompatible content or policy to `{path}`"),
                "Keep content-addressed artifacts and their policy identical across the cohort.",
            ));
        }
        return Ok(());
    }
    files.insert(path.to_owned(), file);
    Ok(())
}

fn schema_files() -> [(&'static str, &'static [u8]); 4] {
    [
        ("schemas/rl-export-manifest-1.schema.json", MANIFEST_SCHEMA),
        ("schemas/rl-export-plan-1.schema.json", PLAN_SCHEMA),
        ("schemas/rl-leakage-report-1.schema.json", LEAKAGE_SCHEMA),
        ("schemas/rl-task-1.schema.json", TASK_SCHEMA),
    ]
}

fn profile_restriction(profile: ReleaseProfile) -> ArtifactRestriction {
    match profile {
        ReleaseProfile::Private => ArtifactRestriction::Private,
        ReleaseProfile::Public => ArtifactRestriction::Public,
    }
}

fn validate_projection(projection: &Projection) -> Result<(), AppError> {
    projection.manifest.validate()?;
    validate_schema_value(
        &serde_json::to_value(&projection.manifest).map_err(serialization_error)?,
        MANIFEST_SCHEMA,
        "RL export manifest",
    )?;
    let expected = projection
        .manifest
        .members
        .iter()
        .map(|member| member.path.as_str())
        .collect::<BTreeSet<_>>();
    if expected != projection.files.keys().map(String::as_str).collect() {
        return Err(rl_error(
            "MCL_RL_EXPORT_BUILD_INVENTORY_MISMATCH",
            "projected RL files differ from the manifest inventory",
            "Rebuild the manifest and files from the same verified cohort.",
        ));
    }
    for member in &projection.manifest.members {
        let file = projection.files.get(&member.path).expect("inventory equal");
        if file.member(member.path.clone()) != *member {
            return Err(rl_error(
                "MCL_RL_EXPORT_BUILD_MEMBER_MISMATCH",
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
    let root = require_real_directory(export_dir, "RL export")?;
    let manifest_bytes = read_real_file(&root.join("manifest.json"), MAX_MANIFEST_BYTES)?;
    let manifest: RlExportManifest = decode_canonical(&manifest_bytes, "RL export manifest")?;
    manifest.validate()?;
    validate_schema_value(
        &serde_json::to_value(&manifest).map_err(serialization_error)?,
        MANIFEST_SCHEMA,
        "RL export manifest",
    )?;
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
        return Err(rl_error(
            "MCL_RL_EXPORT_INVENTORY_MISMATCH",
            "RL export tree has missing, extra, renamed, or empty members",
            "Restore the exact manifest-controlled directory without extra entries.",
        ));
    }
    let mut files = BTreeMap::new();
    for member in &manifest.members {
        let bytes = read_real_file(&safe_member_path(&root, &member.path)?, MAX_MEMBER_BYTES)?;
        if bytes.len() as u64 != member.byte_size || sha256(&bytes) != member.content_hash {
            return Err(rl_error(
                "MCL_RL_EXPORT_MEMBER_HASH_MISMATCH",
                format!(
                    "RL export member `{}` differs from its manifest",
                    member.path
                ),
                "Quarantine the altered export and restore exact member bytes.",
            ));
        }
        files.insert(member.path.clone(), bytes);
    }
    for (path, expected) in schema_files() {
        if required_export(&files, path)? != expected {
            return Err(rl_error(
                "MCL_RL_EXPORT_SCHEMA_SUBSTITUTED",
                format!("RL export schema `{path}` differs from the compiled contract"),
                "Restore the exact committed RL schemas.",
            ));
        }
    }
    let plan: RlExportPlan = decode_canonical(
        required_export(&files, "plan/plan.json")?,
        "copied RL export plan",
    )?;
    plan.validate()?;
    validate_schema_value(
        &serde_json::to_value(&plan).map_err(serialization_error)?,
        PLAN_SCHEMA,
        "RL export plan",
    )?;
    if plan.plan_hash()? != manifest.plan_hash
        || plan.publication_cutoff != manifest.publication_cutoff
        || plan.releases.len() != manifest.source_releases.len()
    {
        return Err(binding_error(
            "copied RL plan differs from the export manifest",
        ));
    }
    let mut source_receipts = BTreeMap::new();
    for (planned, bound) in plan.releases.iter().zip(&manifest.source_releases) {
        if planned.release_id != bound.release_id
            || planned.expected_manifest_hash != bound.release_manifest_hash
            || planned.split != bound.split
            || planned.published_on != bound.published_on
        {
            return Err(binding_error(
                "RL plan release assignment differs from the export binding",
            ));
        }
        let source_manifest: crate::domain::ReleaseManifest = decode_canonical(
            required_export(
                &files,
                &format!("source-releases/{}/manifest.json", bound.release_id),
            )?,
            "copied source release manifest",
        )?;
        if source_manifest.manifest_hash()? != bound.release_manifest_hash
            || source_manifest.profile != bound.release_profile
        {
            return Err(binding_error(
                "copied source release manifest differs from its RL binding",
            ));
        }
        source_receipts.insert(
            bound.release_id.clone(),
            source_manifest.publication.ingestion_receipt_hash,
        );
    }
    let report: RlLeakageReport = decode_canonical(
        required_export(&files, "leakage/report.json")?,
        "RL leakage report",
    )?;
    report.validate()?;
    validate_schema_value(
        &serde_json::to_value(&report).map_err(serialization_error)?,
        LEAKAGE_SCHEMA,
        "RL leakage report",
    )?;
    if report.plan_hash != manifest.plan_hash
        || report.components.len() as u64 != manifest.component_count
        || sha256(required_export(&files, "leakage/report.json")?) != manifest.leakage_report_sha256
    {
        return Err(binding_error(
            "RL leakage report differs from its manifest binding",
        ));
    }
    let component_splits = report
        .components
        .iter()
        .map(|component| (component.component_id.as_str(), component.split))
        .collect::<BTreeMap<_, _>>();
    let mut task_ids = BTreeSet::new();
    let mut family_counts = BTreeMap::<RlTaskFamily, u64>::new();
    for member in manifest
        .members
        .iter()
        .filter(|member| member.kind == RlExportMemberKind::Task)
    {
        let task: RlTask = decode_canonical(required_export(&files, &member.path)?, "RL task")?;
        task.validate()?;
        validate_schema_value(
            &serde_json::to_value(&task).map_err(serialization_error)?,
            TASK_SCHEMA,
            "RL task",
        )?;
        let expected_path = format!(
            "tasks/{}/{}/{}.json",
            task.split.as_str(),
            task.family.as_str(),
            task.task_id
        );
        let source_binding = manifest
            .source_releases
            .iter()
            .find(|source| source.release_id == task.trust.source_release_id)
            .ok_or_else(|| binding_error("RL task names an unbound source release"))?;
        if member.path != expected_path
            || component_splits.get(task.leakage_component_id.as_str()) != Some(&task.split)
            || !task_ids.insert(task.task_id.clone())
            || task.trust.source_release_manifest_hash != source_binding.release_manifest_hash
            || source_receipts.get(&task.trust.source_release_id)
                != Some(&task.trust.publication_receipt_hash)
            || task.split != source_binding.split
            || task.leakage_component_id != source_binding.leakage_component_id
            || task.policy.restriction != profile_restriction(source_binding.release_profile)
            || member.restriction != task.policy.restriction
            || (task.policy.restriction == ArtifactRestriction::Public
                && member.license_expression.as_deref() != Some(GENERATED_RELEASE_LICENSE))
        {
            return Err(binding_error(
                "RL task path, component, identity, or public policy differs from the manifest",
            ));
        }
        for evidence in &task.evidence {
            if files.get(&evidence.path).map(|bytes| sha256(bytes))
                != Some(evidence.content_hash.clone())
            {
                return Err(binding_error(
                    "RL task evidence reference is absent or hash-substituted",
                ));
            }
        }
        *family_counts.entry(task.family).or_default() += 1;
    }
    let reported_task_ids = report
        .components
        .iter()
        .flat_map(|component| component.task_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    if task_ids != reported_task_ids || task_ids.len() as u64 != manifest.task_count {
        return Err(binding_error(
            "RL task inventory differs from the leakage report",
        ));
    }
    for summary in &report.task_families {
        if summary.emitted_task_count != family_counts.get(&summary.family).copied().unwrap_or(0) {
            return Err(binding_error(
                "RL task-family audit differs from the task inventory",
            ));
        }
    }
    Ok(VerifiedExport {
        manifest,
        manifest_hash,
        files,
    })
}

fn validate_schema_value(
    instance: &Value,
    schema_bytes: &[u8],
    label: &str,
) -> Result<(), AppError> {
    let schema: Value = serde_json::from_slice(schema_bytes).map_err(|error| {
        schema_error(format!("committed {label} schema is invalid JSON: {error}"))
    })?;
    let validator = jsonschema::options().build(&schema).map_err(|error| {
        schema_error(format!("committed {label} schema cannot compile: {error}"))
    })?;
    validator
        .validate(instance)
        .map_err(|error| schema_error(format!("{label} schema validation failed: {error}")))
}

fn hashed_key(scope: &str, dimension: &str, value: &str) -> Result<String, AppError> {
    Ok(format!(
        "{scope}:{dimension}:{}",
        value_hash(&json!({"value": value}))?
    ))
}

fn exact_reference(record: &RecordSnapshot) -> ExactVersionReference {
    ExactVersionReference {
        object_id: record.object_id.clone(),
        version_hash: record.version_hash.clone(),
    }
}

fn required<'a>(source: &'a ReleaseIntegrity, path: &str) -> Result<&'a [u8], AppError> {
    source
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
        .ok_or_else(|| binding_error(format!("required RL export member `{path}` is absent")))
}

fn canonical_bytes<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, AppError> {
    let value = serde_json::to_value(value).map_err(|error| {
        rl_error(
            "MCL_RL_SERIALIZATION_FAILED",
            format!("{label} cannot be serialized: {error}"),
            "Report this deterministic RL serialization defect.",
        )
    })?;
    canonical_json(&value)
}

fn decode_canonical<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    label: &str,
) -> Result<T, AppError> {
    let decoded: T = serde_json::from_slice(bytes).map_err(|error| {
        rl_error(
            "MCL_RL_JSON_INVALID",
            format!("{label} is not closed valid JSON: {error}"),
            "Restore the exact canonical JSON member.",
        )
    })?;
    if canonical_bytes(&decoded, label)? != bytes {
        return Err(rl_error(
            "MCL_RL_JSON_NONCANONICAL",
            format!("{label} is not exact canonical JSON"),
            "Restore compact sorted UTF-8 JSON without unknown fields or whitespace.",
        ));
    }
    Ok(decoded)
}

fn decode_value<T: DeserializeOwned>(value: &Value, label: &str) -> Result<T, AppError> {
    serde_json::from_value(value.clone()).map_err(|error| {
        rl_error(
            "MCL_RL_SOURCE_SCHEMA_INVALID",
            format!("source {label} is invalid: {error}"),
            "Quarantine the source release and restore exact schema-valid records.",
        )
    })
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
        return Err(rl_error(
            "MCL_RL_EXPORT_OUTPUT_EXISTS",
            format!("RL export output already exists at {}", absolute.display()),
            "Choose a new destination; RL exports never overwrite paths.",
        ));
    }
    let parent = absolute.parent().ok_or_else(|| {
        rl_error(
            "MCL_RL_EXPORT_OUTPUT_UNSAFE",
            "RL export output has no parent directory",
            "Choose a named directory beneath a real existing parent.",
        )
    })?;
    let parent = require_real_directory(parent, "RL export output parent")?;
    let name = absolute.file_name().ok_or_else(|| {
        rl_error(
            "MCL_RL_EXPORT_OUTPUT_UNSAFE",
            "RL export output has no plain directory name",
            "Choose a named directory beneath a real existing parent.",
        )
    })?;
    Ok((parent.clone(), parent.join(name)))
}

fn write_new_member(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), AppError> {
    let destination = safe_member_path(root, relative)?;
    let parent = destination.parent().expect("member has parent");
    fs::create_dir_all(parent)
        .map_err(|error| AppError::io("create RL export member directory", error))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&destination)
        .map_err(|error| AppError::io("create RL export member", error))?;
    file.write_all(bytes)
        .map_err(|error| AppError::io("write RL export member", error))?;
    file.sync_all()
        .map_err(|error| AppError::io("sync RL export member", error))
}

fn require_real_directory(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| AppError::io("inspect directory", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(rl_error(
            "MCL_RL_EXPORT_PATH_UNSAFE",
            format!("{label} is not a real directory"),
            "Use a real directory tree without symbolic links.",
        ));
    }
    path.canonicalize()
        .map_err(|error| AppError::io("canonicalize RL directory", error))
}

fn safe_member_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || relative.contains('\\')
    {
        return Err(rl_error(
            "MCL_RL_EXPORT_PATH_UNSAFE",
            format!("unsafe RL path `{relative}`"),
            "Use manifest-controlled relative paths without traversal or platform separators.",
        ));
    }
    Ok(root.join(path))
}

fn read_real_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, AppError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| AppError::io("inspect RL member", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(rl_error(
            "MCL_RL_EXPORT_PATH_UNSAFE",
            format!("RL member {} is unsafe or oversized", path.display()),
            "Restore the exact bounded regular file.",
        ));
    }
    fs::read(path).map_err(|error| AppError::io("read RL member", error))
}

fn inventory(root: &Path) -> Result<BTreeSet<String>, AppError> {
    fn visit(
        root: &Path,
        directory: &Path,
        items: &mut BTreeSet<String>,
        entries: &mut usize,
    ) -> Result<(), AppError> {
        for entry in fs::read_dir(directory)
            .map_err(|error| AppError::io("read RL export directory", error))?
        {
            let entry = entry.map_err(|error| AppError::io("read RL export entry", error))?;
            *entries += 1;
            if *entries > MAX_TREE_ENTRIES {
                return Err(rl_error(
                    "MCL_RL_EXPORT_INVENTORY_MISMATCH",
                    "RL export tree exceeds its bounded entry count",
                    "Restore the exact manifest-controlled export tree.",
                ));
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| AppError::io("inspect RL export entry", error))?;
            if metadata.file_type().is_symlink() {
                return Err(rl_error(
                    "MCL_RL_EXPORT_PATH_UNSAFE",
                    "RL export tree contains a symbolic link",
                    "Use a copied export containing only real directories and files.",
                ));
            }
            let relative = path.strip_prefix(root).expect("rooted walk");
            let components = relative
                .components()
                .map(|component| {
                    let Component::Normal(name) = component else {
                        return Err(binding_error("RL inventory path is unsafe"));
                    };
                    name.to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| binding_error("RL inventory path is not UTF-8"))
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            if metadata.is_dir() {
                items.insert(format!("{}/", components.join("/")));
                visit(root, &path, items, entries)?;
            } else if metadata.is_file() {
                items.insert(components.join("/"));
            } else {
                return Err(rl_error(
                    "MCL_RL_EXPORT_PATH_UNSAFE",
                    "RL export tree contains a non-file filesystem entry",
                    "Use only regular directories and files.",
                ));
            }
        }
        Ok(())
    }
    let mut items = BTreeSet::new();
    let mut entries = 0;
    visit(root, root, &mut items, &mut entries)?;
    Ok(items)
}

fn unix_timestamp_date(timestamp: i64) -> Result<String, AppError> {
    const MAX_TIMESTAMP: i64 = 253_402_300_799;
    if !(0..=MAX_TIMESTAMP).contains(&timestamp) {
        return Err(rl_error(
            "MCL_RL_PUBLICATION_DATE_INVALID",
            format!("publication timestamp {timestamp} is outside the supported UTC range"),
            "Restore a receipt with a nonnegative timestamp through year 9999.",
        ));
    }
    let (year, month, day) = civil_from_days(timestamp / 86_400);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
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

fn require_hash(value: &str, label: &str) -> Result<(), AppError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(rl_error(
            "MCL_RL_EXPECTED_HASH_INVALID",
            format!("{label} is not a lowercase SHA-256 identity"),
            "Use the manifest hash emitted by the trusted export channel.",
        ));
    }
    Ok(())
}

fn serialization_error(error: serde_json::Error) -> AppError {
    rl_error(
        "MCL_RL_SERIALIZATION_FAILED",
        error.to_string(),
        "Report this deterministic RL serialization defect.",
    )
}

fn schema_error(message: impl Into<String>) -> AppError {
    rl_error(
        "MCL_RL_SCHEMA_INVALID",
        message,
        "Quarantine the export and restore data matching the committed offline schemas.",
    )
}

fn binding_error(message: impl Into<String>) -> AppError {
    rl_error(
        "MCL_RL_BINDING_MISMATCH",
        message,
        "Quarantine the export and rebuild it from the exact receipt-bound releases and plan.",
    )
}

fn rl_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

fn find(parents: &mut [usize], index: usize) -> usize {
    if parents[index] != index {
        parents[index] = find(parents, parents[index]);
    }
    parents[index]
}

fn union(parents: &mut [usize], left: usize, right: usize) {
    let left = find(parents, left);
    let right = find(parents, right);
    if left != right {
        let (small, large) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        parents[large] = small;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        PublicationOutcome, ReleaseManifest, ReleasePedagogyBinding, ReleasePedagogyMode,
        ReleasePublicationBinding, ReleaseReplayBinding,
    };

    fn plan_release(id: &str, hash: char, split: RlSplit) -> RlPlanRelease {
        RlPlanRelease {
            release_id: id.to_owned(),
            expected_manifest_hash: hash.to_string().repeat(64),
            split,
            published_on: if split == RlSplit::Train {
                "2026-07-20".to_owned()
            } else {
                "2026-07-22".to_owned()
            },
            benchmark_identity: format!("benchmark-{id}"),
            leakage_labels: crate::domain::RlLeakageLabels {
                theorem_dependency_components: vec![format!("dependency-{id}")],
                equivalent_formalizations: vec![format!("equivalence-{id}")],
                shared_sources: vec![format!("source-{id}")],
                certificate_families: vec![format!("certificate-{id}")],
                proof_variants: vec![format!("proof-{id}")],
            },
        }
    }

    fn dummy_release(entry: RlPlanRelease, key: &str) -> BoundRelease {
        let hash = entry.expected_manifest_hash.clone();
        let uuid = |seed| uuid::Uuid::from_u128(seed).to_string();
        BoundRelease {
            plan: entry,
            source: ReleaseIntegrity {
                manifest: ReleaseManifest {
                    schema_version: crate::domain::RELEASE_MANIFEST_SCHEMA_VERSION.to_owned(),
                    profile: ReleaseProfile::Public,
                    publication: ReleasePublicationBinding {
                        ingestion_receipt_hash: "1".repeat(64),
                        authority_evidence_id: uuid(1),
                        authority_evidence_hash: "2".repeat(64),
                        fidelity_evidence_id: uuid(2),
                        fidelity_evidence_hash: "3".repeat(64),
                        fidelity_report_artifact_hash: "4".repeat(64),
                        stage_hash: "5".repeat(64),
                        report_artifact_hash: "6".repeat(64),
                        retained_closure_artifact_hash: "7".repeat(64),
                        attestation_bundle_artifact_hash: "8".repeat(64),
                        raw_verification_hash: "9".repeat(64),
                        request_hash: "a".repeat(64),
                        policy_hash: "b".repeat(64),
                        subject: ExactVersionReference {
                            object_id: uuid(3),
                            version_hash: "c".repeat(64),
                        },
                        outcome: PublicationOutcome::Proof,
                        environment_hash: "d".repeat(64),
                        module_artifact_hash: "e".repeat(64),
                        declaration_name: "Fixture.theorem".to_owned(),
                    },
                    pedagogy: ReleasePedagogyBinding {
                        mode: ReleasePedagogyMode::Prerequisites,
                        include_soft: false,
                        root: ExactVersionReference {
                            object_id: uuid(4),
                            version_hash: "f".repeat(64),
                        },
                        unit_order: vec![ExactVersionReference {
                            object_id: uuid(4),
                            version_hash: "f".repeat(64),
                        }],
                        edge_ids: Vec::new(),
                    },
                    replay: ReleaseReplayBinding {
                        module_path: "replay/Submission.lean".to_owned(),
                        environment_path: "replay/environment.json".to_owned(),
                        declaration_name: "Fixture.theorem".to_owned(),
                    },
                    members: Vec::new(),
                },
                manifest_hash: hash,
                files: BTreeMap::new(),
            },
            records: Vec::new(),
            evidence: Vec::new(),
            edges: Vec::new(),
            leakage_keys: BTreeSet::from([key.to_owned()]),
            component_id: String::new(),
        }
    }

    fn valid_source_manifest() -> ReleaseManifest {
        let hash = |value: char| value.to_string().repeat(64);
        let uuid = |seed| uuid::Uuid::from_u128(seed).to_string();
        let authority_id = uuid(1);
        let fidelity_id = uuid(2);
        let root_id = uuid(4);
        let fidelity_hash = hash('3');
        let mut paths = vec![
            (
                format!("artifacts/{}", hash('a')),
                ReleaseMemberKind::Artifact,
            ),
            (format!("edges/{}.json", uuid(5)), ReleaseMemberKind::Edge),
            (
                "environments/environment.json".to_owned(),
                ReleaseMemberKind::Environment,
            ),
            (
                format!("evidence/{}@{}.json", authority_id, hash('2')),
                ReleaseMemberKind::Evidence,
            ),
            (
                "exports/pedagogy-path.json".to_owned(),
                ReleaseMemberKind::Export,
            ),
            ("licenses/index.json".to_owned(), ReleaseMemberKind::License),
            (
                format!("objects/learning_unit/{root_id}@{}.json", hash('f')),
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
                format!("reports/fidelity/{fidelity_id}@{fidelity_hash}.json"),
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
        ];
        paths.sort_by(|left, right| left.0.cmp(&right.0));
        let members = paths
            .into_iter()
            .map(|(path, kind)| ReleaseMember {
                content_hash: if kind == ReleaseMemberKind::Artifact {
                    path.strip_prefix("artifacts/")
                        .expect("artifact path")
                        .to_owned()
                } else {
                    hash('0')
                },
                path,
                kind,
                byte_size: 1,
                license_expression: None,
                restriction: ArtifactRestriction::Private,
                artifact_metadata: None,
            })
            .collect();
        let manifest = ReleaseManifest {
            schema_version: crate::domain::RELEASE_MANIFEST_SCHEMA_VERSION.to_owned(),
            profile: ReleaseProfile::Private,
            publication: ReleasePublicationBinding {
                ingestion_receipt_hash: hash('1'),
                authority_evidence_id: authority_id,
                authority_evidence_hash: hash('2'),
                fidelity_evidence_id: fidelity_id,
                fidelity_evidence_hash: fidelity_hash,
                fidelity_report_artifact_hash: hash('4'),
                stage_hash: hash('5'),
                report_artifact_hash: hash('6'),
                retained_closure_artifact_hash: hash('7'),
                attestation_bundle_artifact_hash: hash('8'),
                raw_verification_hash: hash('9'),
                request_hash: hash('b'),
                policy_hash: hash('c'),
                subject: ExactVersionReference {
                    object_id: uuid(3),
                    version_hash: hash('d'),
                },
                outcome: PublicationOutcome::Proof,
                environment_hash: hash('e'),
                module_artifact_hash: hash('a'),
                declaration_name: "Fixture.theorem".to_owned(),
            },
            pedagogy: ReleasePedagogyBinding {
                mode: ReleasePedagogyMode::Prerequisites,
                include_soft: false,
                root: ExactVersionReference {
                    object_id: root_id.clone(),
                    version_hash: hash('f'),
                },
                unit_order: vec![ExactVersionReference {
                    object_id: root_id,
                    version_hash: hash('f'),
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
        manifest.validate().expect("valid source manifest");
        manifest
    }

    fn synthetic_projection() -> Projection {
        let source_manifest = valid_source_manifest();
        let source_hash = source_manifest.manifest_hash().expect("source hash");
        let plan = RlExportPlan {
            schema_version: crate::domain::RL_EXPORT_PLAN_SCHEMA_VERSION.to_owned(),
            publication_cutoff: "2026-07-21".to_owned(),
            releases: vec![RlPlanRelease {
                release_id: "fixture".to_owned(),
                expected_manifest_hash: source_hash.clone(),
                split: RlSplit::HeldOutEvaluation,
                published_on: "2026-07-22".to_owned(),
                benchmark_identity: "fixture-benchmark".to_owned(),
                leakage_labels: crate::domain::RlLeakageLabels {
                    theorem_dependency_components: vec!["fixture-dependency".to_owned()],
                    equivalent_formalizations: vec!["fixture-equivalence".to_owned()],
                    shared_sources: vec!["fixture-source".to_owned()],
                    certificate_families: vec!["fixture-certificate".to_owned()],
                    proof_variants: vec!["fixture-proof".to_owned()],
                },
            }],
        };
        let plan_hash = plan.plan_hash().expect("plan hash");
        let component_id = format!("rl_component_{}", "b".repeat(64));
        let mut task = RlTask {
            schema_version: RL_TASK_SCHEMA_VERSION.to_owned(),
            task_id: format!("rl_task_{}", "0".repeat(64)),
            family: RlTaskFamily::Formalization,
            split: RlSplit::HeldOutEvaluation,
            leakage_component_id: component_id.clone(),
            input: json!({"claim": "True."}),
            target: json!({"formal_statement": "True"}),
            evidence: vec![RlTaskEvidenceReference {
                path: "source-releases/fixture/manifest.json".to_owned(),
                content_hash: source_hash.clone(),
            }],
            trust: RlTaskTrust {
                source_release_id: "fixture".to_owned(),
                source_release_manifest_hash: source_hash.clone(),
                publication_receipt_hash: source_manifest
                    .publication
                    .ingestion_receipt_hash
                    .clone(),
                authority_evidence_ids: vec![
                    source_manifest.publication.authority_evidence_id.clone(),
                ],
                fidelity_evidence_ids: vec![
                    source_manifest.publication.fidelity_evidence_id.clone(),
                ],
                kernel_verified: true,
            },
            policy: RlTaskPolicy {
                restriction: ArtifactRestriction::Private,
                license_expressions: Vec::new(),
                license_complete: false,
            },
        };
        task.task_id = task.expected_task_id().expect("task ID");
        task.validate().expect("valid task");
        let task_id = task.task_id.clone();
        let task_families = RlTaskFamily::ALL
            .into_iter()
            .map(|family| {
                let emitted = u64::from(family == RlTaskFamily::Formalization);
                RlTaskFamilySummary {
                    family,
                    emitted_task_count: emitted,
                    skip_reason: (emitted == 0).then(|| "synthetic_evidence_absent".to_owned()),
                }
            })
            .collect();
        let report = RlLeakageReport {
            schema_version: RL_LEAKAGE_REPORT_SCHEMA_VERSION.to_owned(),
            plan_hash: plan_hash.clone(),
            components: vec![RlLeakageComponent {
                component_id: component_id.clone(),
                split: RlSplit::HeldOutEvaluation,
                release_ids: vec!["fixture".to_owned()],
                leakage_keys: vec!["declared:fixture".to_owned()],
                task_ids: vec![task_id.clone()],
            }],
            task_families,
            cross_split_overlap_count: 0,
            temporal_policy_verified: true,
        };
        report.validate().expect("valid report");
        let report_bytes = canonical_bytes(&report, "report").expect("report bytes");
        let mut files = BTreeMap::new();
        insert_file(
            &mut files,
            "plan/plan.json",
            canonical_bytes(&plan, "plan").expect("plan bytes"),
            RlExportMemberKind::Plan,
            None,
            ArtifactRestriction::Private,
        )
        .expect("plan insert");
        for (path, bytes) in schema_files() {
            insert_file(
                &mut files,
                path,
                bytes.to_vec(),
                RlExportMemberKind::Schema,
                Some(GENERATED_RELEASE_LICENSE.to_owned()),
                ArtifactRestriction::Public,
            )
            .expect("schema insert");
        }
        insert_file(
            &mut files,
            "source-releases/fixture/manifest.json",
            canonical_bytes(&source_manifest, "source manifest").expect("source bytes"),
            RlExportMemberKind::SourceReleaseManifest,
            None,
            ArtifactRestriction::Private,
        )
        .expect("source insert");
        insert_file(
            &mut files,
            &format!("tasks/held_out_evaluation/formalization/{task_id}.json"),
            canonical_bytes(&task, "task").expect("task bytes"),
            RlExportMemberKind::Task,
            None,
            ArtifactRestriction::Private,
        )
        .expect("task insert");
        insert_file(
            &mut files,
            "leakage/report.json",
            report_bytes.clone(),
            RlExportMemberKind::LeakageReport,
            None,
            ArtifactRestriction::Private,
        )
        .expect("report insert");
        let manifest = RlExportManifest {
            schema_version: RL_EXPORT_MANIFEST_SCHEMA_VERSION.to_owned(),
            plan_hash,
            publication_cutoff: plan.publication_cutoff,
            source_releases: vec![RlExportSourceBinding {
                release_id: "fixture".to_owned(),
                release_manifest_hash: source_hash,
                release_profile: ReleaseProfile::Private,
                split: RlSplit::HeldOutEvaluation,
                published_on: "2026-07-22".to_owned(),
                leakage_component_id: component_id,
            }],
            leakage_report_sha256: sha256(&report_bytes),
            task_count: 1,
            component_count: 1,
            members: files
                .iter()
                .map(|(path, file)| file.member(path.clone()))
                .collect(),
        };
        let projection = Projection { manifest, files };
        validate_projection(&projection).expect("valid synthetic projection");
        projection
    }

    fn materialize_projection(root: &Path, projection: &Projection) {
        for (path, file) in &projection.files {
            write_new_member(root, path, &file.bytes).expect("write member");
        }
        write_new_member(
            root,
            "manifest.json",
            &canonical_bytes(&projection.manifest, "manifest").expect("manifest bytes"),
        )
        .expect("write manifest");
    }

    #[test]
    fn component_assignment_rejects_cross_split_overlap() {
        let mut releases = vec![
            dummy_release(plan_release("train-a", 'a', RlSplit::Train), "shared"),
            dummy_release(
                plan_release("eval-a", 'b', RlSplit::HeldOutEvaluation),
                "shared",
            ),
        ];
        assert_eq!(
            assign_components(&mut releases)
                .expect_err("cross-split component blocked")
                .code,
            "MCL_RL_SPLIT_LEAKAGE"
        );
    }

    #[test]
    fn component_assignment_is_stable_and_component_level() {
        let mut releases = vec![
            dummy_release(
                plan_release("eval-a", 'a', RlSplit::HeldOutEvaluation),
                "shared",
            ),
            dummy_release(
                plan_release("eval-b", 'b', RlSplit::HeldOutEvaluation),
                "shared",
            ),
            dummy_release(
                plan_release("eval-c", 'c', RlSplit::HeldOutEvaluation),
                "separate",
            ),
        ];
        let first = assign_components(&mut releases).expect("components");
        assert_eq!(first.len(), 2);
        assert_eq!(releases[0].component_id, releases[1].component_id);
        assert_ne!(releases[0].component_id, releases[2].component_id);
        let observed = first
            .iter()
            .map(|component| component.component_id.clone())
            .collect::<Vec<_>>();
        let mut second_releases = vec![
            dummy_release(
                plan_release("eval-a", 'a', RlSplit::HeldOutEvaluation),
                "shared",
            ),
            dummy_release(
                plan_release("eval-b", 'b', RlSplit::HeldOutEvaluation),
                "shared",
            ),
            dummy_release(
                plan_release("eval-c", 'c', RlSplit::HeldOutEvaluation),
                "separate",
            ),
        ];
        let repeated = assign_components(&mut second_releases).expect("repeated components");
        assert_eq!(
            observed,
            repeated
                .iter()
                .map(|component| component.component_id.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn committed_schemas_compile_and_timestamp_binding_is_utc() {
        for (path, bytes) in schema_files() {
            let schema: Value = serde_json::from_slice(bytes).expect("schema JSON");
            jsonschema::options()
                .build(&schema)
                .unwrap_or_else(|error| panic!("{path} does not compile: {error}"));
        }
        assert_eq!(unix_timestamp_date(0).expect("epoch"), "1970-01-01");
        assert_eq!(
            unix_timestamp_date(253_402_300_799).expect("upper bound"),
            "9999-12-31"
        );
        assert!(unix_timestamp_date(-1).is_err());
    }

    #[test]
    fn output_and_content_addressed_paths_are_immutable() {
        let parent = tempfile::tempdir().expect("parent");
        let output = parent.path().join("already-exists");
        fs::create_dir(&output).expect("existing output");
        assert_eq!(
            resolve_new_output(&output)
                .expect_err("overwrite blocked")
                .code,
            "MCL_RL_EXPORT_OUTPUT_EXISTS"
        );
        let mut files = BTreeMap::new();
        insert_file(
            &mut files,
            "artifacts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            b"one".to_vec(),
            RlExportMemberKind::Artifact,
            None,
            ArtifactRestriction::Private,
        )
        .expect("first insert");
        assert_eq!(
            insert_file(
                &mut files,
                "artifacts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                b"two".to_vec(),
                RlExportMemberKind::Artifact,
                None,
                ArtifactRestriction::Private,
            )
            .expect_err("conflicting content blocked")
            .code,
            "MCL_RL_EXPORT_MEMBER_CONFLICT"
        );
    }

    #[test]
    fn integrity_rejects_inventory_hash_schema_and_expected_hash_substitution() {
        let projection = synthetic_projection();
        let exact = tempfile::tempdir().expect("exact export");
        materialize_projection(exact.path(), &projection);
        verify_export_integrity(exact.path()).expect("exact integrity");

        let extra = tempfile::tempdir().expect("extra export");
        materialize_projection(extra.path(), &synthetic_projection());
        fs::create_dir(extra.path().join("empty-extra")).expect("extra directory");
        assert_eq!(
            verify_export_integrity(extra.path())
                .expect_err("extra inventory blocked")
                .code,
            "MCL_RL_EXPORT_INVENTORY_MISMATCH"
        );

        let changed = tempfile::tempdir().expect("changed export");
        materialize_projection(changed.path(), &synthetic_projection());
        let task_path = fs::read_dir(
            changed
                .path()
                .join("tasks/held_out_evaluation/formalization"),
        )
        .expect("task directory")
        .next()
        .expect("task entry")
        .expect("task entry")
        .path();
        fs::write(task_path, b"{}").expect("alter task");
        assert_eq!(
            verify_export_integrity(changed.path())
                .expect_err("member hash blocked")
                .code,
            "MCL_RL_EXPORT_MEMBER_HASH_MISMATCH"
        );

        let mut substituted = synthetic_projection();
        let schema_path = "schemas/rl-task-1.schema.json";
        substituted
            .files
            .get_mut(schema_path)
            .expect("schema")
            .bytes = b"{}".to_vec();
        let member = substituted
            .files
            .get(schema_path)
            .expect("schema")
            .member(schema_path.to_owned());
        *substituted
            .manifest
            .members
            .iter_mut()
            .find(|item| item.path == schema_path)
            .expect("schema member") = member;
        let schema_root = tempfile::tempdir().expect("schema export");
        materialize_projection(schema_root.path(), &substituted);
        assert_eq!(
            verify_export_integrity(schema_root.path())
                .expect_err("schema substitution blocked")
                .code,
            "MCL_RL_EXPORT_SCHEMA_SUBSTITUTED"
        );

        let observed =
            sha256(&canonical_bytes(&projection.manifest, "manifest").expect("manifest bytes"));
        let mut substituted_plan: RlExportPlan = decode_canonical(
            &projection.files.get("plan/plan.json").expect("plan").bytes,
            "plan",
        )
        .expect("decode plan");
        substituted_plan.releases[0].benchmark_identity = "substituted-benchmark".to_owned();
        let plan_root = tempfile::tempdir().expect("plan root");
        let plan_path = plan_root.path().join("plan.json");
        fs::write(
            &plan_path,
            canonical_bytes(&substituted_plan, "substituted plan").expect("plan bytes"),
        )
        .expect("write substituted plan");
        assert_eq!(
            verify_rl_export(
                exact.path(),
                &observed,
                &plan_path,
                Path::new("unread-sources"),
            )
            .expect_err("independent plan substitution blocked")
            .code,
            "MCL_RL_BINDING_MISMATCH"
        );

        let substituted_hash = if observed == "a".repeat(64) {
            "b".repeat(64)
        } else {
            "a".repeat(64)
        };
        assert_eq!(
            verify_rl_export(
                exact.path(),
                &substituted_hash,
                Path::new("unread-plan"),
                Path::new("unread-sources"),
            )
            .expect_err("expected hash substitution blocked")
            .code,
            "MCL_RL_EXPORT_MANIFEST_HASH_MISMATCH"
        );
    }
}
