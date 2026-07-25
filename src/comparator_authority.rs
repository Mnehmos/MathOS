use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::canonical::canonical_json;
use crate::comparator_export::verify_comparator_package;
use crate::comparator_run::{ComparatorRunVerificationRequest, verify_comparator_run};
use crate::domain::{
    COMPARATOR_AUTHORITY_RUN_PATHS, COMPARATOR_RUN_REPORT_SCHEMA_VERSION,
    ComparatorAuthorityArtifactSet, ComparatorAuthorityStage, ComparatorAuthorityStageArtifact,
    ComparatorPackagePlan, ComparatorPackageVerification, ComparatorRunClassification,
    ComparatorRunReport, ReleaseManifest, committed_comparator_authority_policy,
};
use crate::error::AppError;
use crate::release::verify_release_bundle_integrity;

const MAX_PLAN_BYTES: u64 = 2 * 1_048_576;

pub(crate) struct PreparedComparatorAuthorityStage {
    pub stage: ComparatorAuthorityStage,
    pub report: ComparatorRunReport,
    pub package_verification: ComparatorPackageVerification,
    pub plan: ComparatorPackagePlan,
    pub release_manifest: ReleaseManifest,
    pub run_root: PathBuf,
    pub release_files: BTreeMap<String, Vec<u8>>,
    pub plan_bytes: Vec<u8>,
    pub attestation_bundle_bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_comparator_authority_stage(
    run_dir: &Path,
    expected_report_hash: &str,
    expected_package_verification_hash: &str,
    plan_path: &Path,
    release_dir: &Path,
    expected_release_manifest_hash: &str,
    attestation_bundle_bytes: &[u8],
) -> Result<PreparedComparatorAuthorityStage, AppError> {
    let run_root = require_real_directory(run_dir, "Comparator run bundle")?;
    let release_root = require_real_directory(release_dir, "source release")?;
    let plan_bytes = read_real_file(plan_path, MAX_PLAN_BYTES, "Comparator plan")?;
    let plan: ComparatorPackagePlan = decode_canonical(&plan_bytes, "Comparator plan")?;
    plan.validate()?;
    let plan_hash = format!("{:x}", Sha256::digest(&plan_bytes));

    let report_bytes = read_real_file(
        &run_root.join("report.json"),
        crate::domain::MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator report",
    )?;
    let report: ComparatorRunReport = decode_canonical(&report_bytes, "Comparator report")?;
    report.validate()?;
    if report.schema_version != COMPARATOR_RUN_REPORT_SCHEMA_VERSION
        || report.classification != ComparatorRunClassification::Accepted
        || !report.comparator_verified
        || report.authoritative
    {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_REPORT_REJECTED",
            "only an exact accepted non-authoritative protected report can enter the authority gate",
            "Use the accepted report emitted by the protected official Comparator job.",
        ));
    }
    let report_hash = format!("{:x}", Sha256::digest(&report_bytes));
    if report_hash != expected_report_hash {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_REPORT_HASH_MISMATCH",
            "Comparator report differs from the independently expected identity",
            "Use the exact report hash from the protected retained-artifact channel.",
        ));
    }

    let run_verification = verify_comparator_run(ComparatorRunVerificationRequest {
        run_dir: &run_root,
        expected_report_hash,
        expected_package_verification_hash,
    })?;
    if !run_verification.comparator_verified
        || run_verification.authoritative
        || !run_verification.database_independent
    {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_RUN_REJECTED",
            "Comparator run failed the reviewed database-independent verification contract",
            "Quarantine the bundle and restore the exact accepted protected run.",
        ));
    }

    let release = verify_release_bundle_integrity(&release_root)?;
    if release.manifest_hash != expected_release_manifest_hash
        || report.package.source_release_manifest_hash != release.manifest_hash
        || plan.source_release_manifest_hash != release.manifest_hash
    {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_RELEASE_MISMATCH",
            "Comparator report, plan, and frozen release identities disagree",
            "Use the exact frozen release bound by the protected Comparator package.",
        ));
    }
    let package_report = verify_comparator_package(
        &run_root.join("package"),
        expected_package_verification_hash,
        plan_path,
        &release_root,
        expected_release_manifest_hash,
    )?;
    if package_report.verification_hash != report.package.verification_hash
        || package_report.input_fingerprint != report.package.input_fingerprint
        || package_report.source_formalization != report.package.source_formalization
        || report.package.plan_hash != plan_hash
    {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_PACKAGE_MISMATCH",
            "Comparator report does not match the independently reprojected package",
            "Quarantine the closure and rebuild it from the exact plan and frozen release.",
        ));
    }

    if attestation_bundle_bytes.is_empty()
        || attestation_bundle_bytes.len() as u64
            > crate::domain::comparator_authority::MAX_COMPARATOR_ATTESTATION_BUNDLE_BYTES
    {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_BUNDLE_INVALID",
            "Comparator attestation bundle is empty or exceeds its staging bound",
            "Use one bounded Sigstore JSON bundle for the exact report.",
        ));
    }
    let bundle_value: Value =
        serde_json::from_slice(attestation_bundle_bytes).map_err(|error| {
            authority_error(
                "MCL_COMPARATOR_AUTHORITY_BUNDLE_INVALID",
                format!("Comparator attestation bundle is not valid JSON: {error}"),
                "Use the exact Sigstore JSON bundle emitted by the protected workflow.",
            )
        })?;
    if !bundle_value.is_object() {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_BUNDLE_INVALID",
            "Comparator attestation bundle must be one JSON object",
            "Use the exact Sigstore JSON bundle emitted by the protected workflow.",
        ));
    }

    let mut artifacts = Vec::new();
    for path in COMPARATOR_AUTHORITY_RUN_PATHS {
        let full_path = safe_member_path(&run_root, path)?;
        let (content_hash, byte_size) = hash_real_file(
            &full_path,
            crate::domain::MAX_COMPARATOR_RUN_BINARY_BYTES,
            path,
        )?;
        artifacts.push(ComparatorAuthorityStageArtifact {
            artifact_set: ComparatorAuthorityArtifactSet::RunBundle,
            path: path.to_owned(),
            content_hash,
            byte_size,
        });
    }

    let manifest_bytes =
        canonical_json(&serde_json::to_value(&release.manifest).map_err(|error| {
            authority_error(
                "MCL_COMPARATOR_AUTHORITY_RELEASE_MISMATCH",
                error.to_string(),
                "Report this deterministic release-manifest serialization defect.",
            )
        })?)?;
    let on_disk_manifest = read_real_file(
        &release_root.join("manifest.json"),
        4 * 1_048_576,
        "release manifest",
    )?;
    if manifest_bytes != on_disk_manifest {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_RELEASE_MISMATCH",
            "frozen release manifest is not canonical",
            "Restore the exact canonical frozen release.",
        ));
    }
    let mut release_files = release.files;
    release_files.insert("manifest.json".to_owned(), manifest_bytes);
    for (path, bytes) in &release_files {
        artifacts.push(ComparatorAuthorityStageArtifact {
            artifact_set: ComparatorAuthorityArtifactSet::SourceRelease,
            path: path.clone(),
            content_hash: format!("{:x}", Sha256::digest(bytes)),
            byte_size: bytes.len() as u64,
        });
    }

    let policy = committed_comparator_authority_policy()?;
    let policy_hash = policy.policy_hash()?;
    let policy_bytes = canonical_json(&serde_json::to_value(&policy).map_err(|error| {
        authority_error(
            "MCL_COMPARATOR_AUTHORITY_POLICY_INVALID",
            error.to_string(),
            "Report this deterministic Comparator policy serialization defect.",
        )
    })?)?;
    let attestation_bundle_artifact_hash =
        format!("{:x}", Sha256::digest(attestation_bundle_bytes));
    let stage = ComparatorAuthorityStage {
        schema_version: crate::domain::COMPARATOR_AUTHORITY_STAGE_SCHEMA_VERSION.to_owned(),
        report_artifact_hash: report_hash,
        package_verification_hash: package_report.verification_hash,
        package_input_fingerprint: package_report.input_fingerprint,
        plan_artifact_hash: plan_hash,
        plan_byte_size: plan_bytes.len() as u64,
        source_release_manifest_hash: release.manifest_hash,
        attestation_bundle_artifact_hash,
        attestation_bundle_byte_size: attestation_bundle_bytes.len() as u64,
        policy_hash,
        source_formalization: report.package.source_formalization.clone(),
        source_commit_sha: report.workflow.source_commit_sha.clone(),
        workflow_run_id: report.workflow.run_id.clone(),
        workflow_run_attempt: report.workflow.run_attempt,
        artifacts,
        authoritative: false,
    };
    stage.validate()?;
    Ok(PreparedComparatorAuthorityStage {
        stage,
        report,
        package_verification: serde_json::from_slice(&read_real_file(
            &run_root.join("package/verification.json"),
            crate::domain::MAX_COMPARATOR_RUN_TEXT_BYTES,
            "Comparator package verification",
        )?)
        .map_err(|error| {
            authority_error(
                "MCL_COMPARATOR_AUTHORITY_PACKAGE_MISMATCH",
                format!("Comparator package verification is invalid: {error}"),
                "Restore the exact canonical Comparator package.",
            )
        })?,
        plan,
        release_manifest: release.manifest,
        run_root,
        release_files,
        plan_bytes,
        attestation_bundle_bytes: attestation_bundle_bytes.to_vec(),
        policy_bytes,
    })
}

pub(crate) fn read_prepared_run_member(
    prepared: &PreparedComparatorAuthorityStage,
    path: &str,
) -> Result<Vec<u8>, AppError> {
    let entry = prepared
        .stage
        .artifacts
        .iter()
        .find(|entry| {
            entry.artifact_set == ComparatorAuthorityArtifactSet::RunBundle && entry.path == path
        })
        .ok_or_else(|| {
            authority_error(
                "MCL_COMPARATOR_STAGE_INVALID",
                format!("prepared Comparator stage has no run member `{path}`"),
                "Rebuild the exact Comparator authority stage.",
            )
        })?;
    let bytes = read_real_file(
        &safe_member_path(&prepared.run_root, path)?,
        entry.byte_size,
        path,
    )?;
    if bytes.len() as u64 != entry.byte_size
        || format!("{:x}", Sha256::digest(&bytes)) != entry.content_hash
    {
        return Err(authority_error(
            "MCL_COMPARATOR_STAGE_INPUT_CHANGED",
            format!("Comparator run member `{path}` changed after verification"),
            "Retry staging from an unchanged protected artifact directory.",
        ));
    }
    Ok(bytes)
}

fn decode_canonical<T: DeserializeOwned>(bytes: &[u8], label: &str) -> Result<T, AppError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        authority_error(
            "MCL_COMPARATOR_AUTHORITY_DOCUMENT_INVALID",
            format!("{label} is not valid JSON: {error}"),
            "Restore the exact canonical protected document.",
        )
    })?;
    let canonical = canonical_json(&value)?;
    if canonical != bytes {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_DOCUMENT_INVALID",
            format!("{label} is not canonical JSON"),
            "Restore the exact canonical protected document.",
        ));
    }
    serde_json::from_value(value).map_err(|error| {
        authority_error(
            "MCL_COMPARATOR_AUTHORITY_DOCUMENT_INVALID",
            format!("{label} violates its closed contract: {error}"),
            "Restore the exact canonical protected document.",
        )
    })
}

fn require_real_directory(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io(&format!("inspect {label}"), error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_PATH_UNSAFE",
            format!("{label} must be a real directory"),
            "Use a copied artifact directory without links or reparse points.",
        ));
    }
    path.canonicalize()
        .map_err(|error| AppError::io(&format!("canonicalize {label}"), error))
}

fn safe_member_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_PATH_UNSAFE",
            format!("unsafe staged member path `{relative}`"),
            "Use only normalized relative paths from the closed manifests.",
        ));
    }
    let path = root.join(relative_path);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| AppError::io(&format!("inspect staged member `{relative}`"), error))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_PATH_UNSAFE",
            format!("staged member `{relative}` is not a real file"),
            "Use the exact artifact tree without links or special files.",
        ));
    }
    let canonical = path.canonicalize().map_err(|error| {
        AppError::io(&format!("canonicalize staged member `{relative}`"), error)
    })?;
    if !canonical.starts_with(root) {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_PATH_UNSAFE",
            format!("staged member `{relative}` escapes its root"),
            "Use the exact artifact tree without path redirection.",
        ));
    }
    Ok(canonical)
}

fn read_real_file(path: &Path, maximum: u64, label: &str) -> Result<Vec<u8>, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io(&format!("inspect {label}"), error))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > maximum {
        return Err(authority_error(
            "MCL_COMPARATOR_AUTHORITY_PATH_UNSAFE",
            format!("{label} is not a bounded real file"),
            "Use the exact bounded protected artifact without links.",
        ));
    }
    fs::read(path).map_err(|error| AppError::io(&format!("read {label}"), error))
}

fn hash_real_file(path: &Path, maximum: u64, label: &str) -> Result<(String, u64), AppError> {
    let bytes = read_real_file(path, maximum, label)?;
    Ok((format!("{:x}", Sha256::digest(&bytes)), bytes.len() as u64))
}

fn authority_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: &'static str,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}
