use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::canonical::canonical_json;
use crate::comparator_export::{ComparatorPackageVerificationReport, verify_package_integrity};
use crate::domain::{
    COMPARATOR_RUN_COMPARATOR_COMMIT, COMPARATOR_RUN_LANDRUN_COMMIT, COMPARATOR_RUN_LEAN_TOOLCHAIN,
    COMPARATOR_RUN_LEAN4EXPORT_COMMIT, COMPARATOR_RUN_PROJECT_NAME, ComparatorRunClassification,
    ComparatorRunFileBinding, ComparatorRunReport, MAX_ACCEPTED_COMPARATOR_STDERR_BYTES,
    MAX_ACCEPTED_COMPARATOR_STDOUT_BYTES, MAX_COMPARATOR_RUN_BINARY_BYTES,
    MAX_COMPARATOR_RUN_TEXT_BYTES,
};
use crate::error::AppError;

const REPORT_PATH: &str = "report.json";
const EXPECTED_ROOT_FILES: [&str; 15] = [
    "comparator.bin",
    "comparator.stderr",
    "comparator.stdout",
    "lake-manifest.json",
    "lakefile.toml",
    "landlock-probe.stderr",
    "landlock-probe.stdout",
    "landrun.bin",
    "lean-toolchain",
    "lean4export.bin",
    "network-probe.py",
    "package-reprojection.json",
    "report.json",
    "runner-script.sh",
    "systemd.properties",
];
const EXPECTED_PACKAGE_FILES: [&str; 5] = [
    "Challenge.lean",
    "Solution.lean",
    "config.json",
    "formalization.yaml",
    "verification.json",
];
const LAKEFILE: &str = r#"name = "mathos_comparator_pilot_a"
version = "0.1.0"

[[lean_lib]]
name = "Challenge"

[[lean_lib]]
name = "Solution"
"#;
const LANDLOCK_PROBE_STDOUT: &str = "MATHOS_LANDLOCK_STRICT_PROBE=passed\n";

#[derive(Clone, Debug)]
pub struct ComparatorRunVerificationRequest<'a> {
    pub run_dir: &'a Path,
    pub expected_report_hash: &'a str,
    pub expected_package_verification_hash: &'a str,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ComparatorRunVerificationOutcome {
    pub report_hash: String,
    pub package_verification_hash: String,
    pub source_release_manifest_hash: String,
    pub source_commit_sha: String,
    pub workflow_run_id: String,
    pub classification: ComparatorRunClassification,
    pub comparator_verified: bool,
    pub authoritative: bool,
    pub database_independent: bool,
    pub inventory_verified: bool,
    pub hashes_verified: bool,
    pub package_bindings_verified: bool,
    pub tool_bindings_verified: bool,
    pub harness_verified: bool,
    pub sandbox_verified: bool,
    pub official_success_path_verified: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LakeManifest {
    version: String,
    packages_dir: String,
    packages: Vec<Value>,
    name: String,
    lake_dir: String,
    fixed_toolchain: bool,
}

pub fn verify_comparator_run(
    request: ComparatorRunVerificationRequest<'_>,
) -> Result<ComparatorRunVerificationOutcome, AppError> {
    require_hash(
        request.expected_report_hash,
        "expected Comparator run report",
    )?;
    require_hash(
        request.expected_package_verification_hash,
        "expected Comparator package verification",
    )?;
    let root = require_real_directory(request.run_dir)?;
    verify_inventory(&root)?;

    let report_bytes = read_real_file(
        &root.join(REPORT_PATH),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator run report",
    )?;
    let report_hash = sha256(&report_bytes);
    if report_hash != request.expected_report_hash {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_REPORT_HASH_MISMATCH",
            format!(
                "Comparator run report hash {report_hash} differs from trusted expected hash {}",
                request.expected_report_hash
            ),
        ));
    }
    let report: ComparatorRunReport = serde_json::from_slice(&report_bytes).map_err(|error| {
        run_error(
            "MCL_COMPARATOR_RUN_REPORT_INVALID",
            format!("Comparator run report is not closed valid JSON: {error}"),
        )
    })?;
    report.validate()?;
    let report_value = serde_json::to_value(&report).map_err(|error| {
        run_error(
            "MCL_COMPARATOR_RUN_REPORT_INVALID",
            format!("Comparator run report cannot be serialized: {error}"),
        )
    })?;
    if canonical_json(&report_value)? != report_bytes {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_REPORT_NONCANONICAL",
            "Comparator run report is not exact RFC 8785 canonical JSON",
        ));
    }

    verify_report_bindings(&root, &report)?;
    let package = verify_package_integrity(&root.join("package"))?;
    if package.verification_hash != request.expected_package_verification_hash
        || report.package.verification_hash != request.expected_package_verification_hash
    {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_PACKAGE_HASH_MISMATCH",
            "Comparator run report or retained package differs from the trusted package identity",
        ));
    }
    verify_package_binding(&report, &package.verification)?;
    verify_reprojection(&root, &report)?;
    verify_harness(&root)?;
    verify_runner_assets(&root)?;
    verify_landlock_probe(&root)?;
    verify_systemd_properties(&root)?;

    let stdout = read_real_file(
        &root.join("comparator.stdout"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator stdout",
    )?;
    let stderr = read_real_file(
        &root.join("comparator.stderr"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator stderr",
    )?;
    let success_path = verify_official_stdout(&stdout, &report)?;
    let output_bounds = stdout.len() as u64 <= MAX_ACCEPTED_COMPARATOR_STDOUT_BYTES
        && stderr.len() as u64 <= MAX_ACCEPTED_COMPARATOR_STDERR_BYTES;
    let accepted = report.execution.exit_code == 0
        && !report.execution.timed_out
        && stderr.is_empty()
        && output_bounds
        && success_path
        && report.sandbox.tcp_network_denied
        && report.sandbox.unix_socket_denied
        && report.sandbox.network_isolated
        && report.predicates.all();
    let observed_classification = if report.execution.timed_out {
        ComparatorRunClassification::Failed
    } else if accepted {
        ComparatorRunClassification::Accepted
    } else {
        ComparatorRunClassification::Rejected
    };
    if report.classification != observed_classification || report.comparator_verified != accepted {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_CLASSIFICATION_MISMATCH",
            "Comparator run classification does not match the independently verified execution",
        ));
    }

    Ok(ComparatorRunVerificationOutcome {
        report_hash,
        package_verification_hash: package.verification_hash,
        source_release_manifest_hash: report.package.source_release_manifest_hash,
        source_commit_sha: report.workflow.source_commit_sha,
        workflow_run_id: report.workflow.run_id,
        classification: report.classification,
        comparator_verified: accepted,
        authoritative: false,
        database_independent: true,
        inventory_verified: true,
        hashes_verified: true,
        package_bindings_verified: true,
        tool_bindings_verified: true,
        harness_verified: true,
        sandbox_verified: true,
        official_success_path_verified: success_path,
    })
}

fn verify_report_bindings(root: &Path, report: &ComparatorRunReport) -> Result<(), AppError> {
    for binding in &report.package.members {
        verify_file_binding(root, binding, MAX_COMPARATOR_RUN_TEXT_BYTES)?;
    }
    for tool in &report.tools {
        verify_file_binding(root, &tool.binary, MAX_COMPARATOR_RUN_BINARY_BYTES)?;
    }
    for binding in &report.harness.files {
        verify_file_binding(root, binding, MAX_COMPARATOR_RUN_TEXT_BYTES)?;
    }
    for binding in [
        &report.execution.stdout,
        &report.execution.stderr,
        &report.execution.systemd_properties,
        &report.execution.landlock_probe_stdout,
        &report.execution.landlock_probe_stderr,
        &report.execution.package_reprojection,
        &report.execution.runner_script,
        &report.execution.network_probe,
    ] {
        verify_file_binding(root, binding, MAX_COMPARATOR_RUN_TEXT_BYTES)?;
    }
    Ok(())
}

fn verify_package_binding(
    report: &ComparatorRunReport,
    package: &crate::domain::ComparatorPackageVerification,
) -> Result<(), AppError> {
    if report.package.input_fingerprint != package.input_fingerprint
        || report.package.plan_hash != package.plan_hash
        || report.package.source_release_manifest_hash != package.source_release_manifest_hash
        || report.package.source_formalization != package.source_formalization
        || report.package.declaration_name != package.declaration_name
        || report.package.lean_toolchain != package.lean_toolchain
        || package.tool_pins.comparator_commit != COMPARATOR_RUN_COMPARATOR_COMMIT
        || package.tool_pins.lean4export_commit != COMPARATOR_RUN_LEAN4EXPORT_COMMIT
        || package.tool_pins.landrun_commit != COMPARATOR_RUN_LANDRUN_COMMIT
    {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_PACKAGE_BINDING_MISMATCH",
            "Comparator run report does not bind the exact ready package and fixed tool pins",
        ));
    }
    Ok(())
}

fn verify_reprojection(root: &Path, report: &ComparatorRunReport) -> Result<(), AppError> {
    let bytes = read_real_file(
        &root.join("package-reprojection.json"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator package reprojection receipt",
    )?;
    let receipt: ComparatorPackageVerificationReport =
        serde_json::from_slice(&bytes).map_err(|error| {
            run_error(
                "MCL_COMPARATOR_RUN_REPROJECTION_INVALID",
                format!("Comparator package reprojection receipt is invalid: {error}"),
            )
        })?;
    if receipt.verification_hash != report.package.verification_hash
        || receipt.input_fingerprint != report.package.input_fingerprint
        || receipt.source_release_manifest_hash != report.package.source_release_manifest_hash
        || receipt.source_formalization != report.package.source_formalization
        || receipt.comparator_verified
        || receipt.authoritative
        || receipt.member_count != 5
        || !receipt.database_independent
        || !receipt.inventory_verified
        || !receipt.hashes_verified
        || !receipt.bindings_verified
        || !receipt.deterministic_reprojection
    {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_REPROJECTION_INVALID",
            "Comparator package reprojection receipt does not prove the exact ready package",
        ));
    }
    Ok(())
}

fn verify_harness(root: &Path) -> Result<(), AppError> {
    let toolchain = read_real_file(
        &root.join("lean-toolchain"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator harness toolchain",
    )?;
    if toolchain != format!("{COMPARATOR_RUN_LEAN_TOOLCHAIN}\n").as_bytes() {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_HARNESS_INVALID",
            "Comparator harness uses the wrong Lean toolchain bytes",
        ));
    }
    let lakefile = read_real_file(
        &root.join("lakefile.toml"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator harness lakefile",
    )?;
    if lakefile != LAKEFILE.as_bytes() {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_HARNESS_INVALID",
            "Comparator harness lakefile differs from the fixed project",
        ));
    }
    let manifest_bytes = read_real_file(
        &root.join("lake-manifest.json"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator harness Lake manifest",
    )?;
    let manifest: LakeManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        run_error(
            "MCL_COMPARATOR_RUN_HARNESS_INVALID",
            format!("Comparator harness Lake manifest is invalid: {error}"),
        )
    })?;
    if manifest.version != "1.2.0"
        || manifest.packages_dir != ".lake/packages"
        || !manifest.packages.is_empty()
        || manifest.name != COMPARATOR_RUN_PROJECT_NAME
        || manifest.lake_dir != ".lake"
        || manifest.fixed_toolchain
    {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_HARNESS_INVALID",
            "Comparator harness Lake manifest has dependencies or unexpected state",
        ));
    }
    Ok(())
}

fn verify_runner_assets(root: &Path) -> Result<(), AppError> {
    let runner_script = read_real_file(
        &root.join("runner-script.sh"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "protected Comparator runner script",
    )?;
    let network_probe = read_real_file(
        &root.join("network-probe.py"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "protected Comparator network probe",
    )?;
    if runner_script != include_bytes!("../scripts/comparator-protected-run.sh")
        || network_probe != include_bytes!("../scripts/comparator-network-probe.py")
    {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_INVOCATION_MISMATCH",
            "retained Comparator runner or network probe differs from the reviewed verifier build",
        ));
    }
    Ok(())
}

fn verify_landlock_probe(root: &Path) -> Result<(), AppError> {
    let stdout = read_real_file(
        &root.join("landlock-probe.stdout"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Landlock strict-probe stdout",
    )?;
    let stderr = read_real_file(
        &root.join("landlock-probe.stderr"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Landlock strict-probe stderr",
    )?;
    let stderr = std::str::from_utf8(&stderr).map_err(|_| {
        run_error(
            "MCL_COMPARATOR_RUN_SANDBOX_INVALID",
            "Landlock strict-probe stderr is not UTF-8",
        )
    })?;
    if stdout != LANDLOCK_PROBE_STDOUT.as_bytes()
        || !stderr.contains("BestEffort:false")
        || stderr.contains("BestEffort:true")
        || !stderr.contains("Landlock restrictions applied successfully")
    {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_SANDBOX_INVALID",
            "Landlock strict V5 probe did not prove fail-closed filesystem and TCP controls",
        ));
    }
    Ok(())
}

fn verify_systemd_properties(root: &Path) -> Result<(), AppError> {
    let bytes = read_real_file(
        &root.join("systemd.properties"),
        MAX_COMPARATOR_RUN_TEXT_BYTES,
        "Comparator systemd properties",
    )?;
    let value = std::str::from_utf8(&bytes).map_err(|_| {
        run_error(
            "MCL_COMPARATOR_RUN_SANDBOX_INVALID",
            "Comparator systemd properties are not UTF-8",
        )
    })?;
    if value.contains('\r') {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_SANDBOX_INVALID",
            "Comparator systemd properties do not use LF line endings",
        ));
    }
    let lines = value.lines().collect::<BTreeSet<_>>();
    let expected = [
        "NoNewPrivileges=yes",
        "RestrictAddressFamilies=~AF_UNIX",
        "User=",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if lines != expected || value.lines().count() != 3 {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_SANDBOX_INVALID",
            "live systemd properties do not prove the reviewed user-service restrictions",
        ));
    }
    Ok(())
}

fn verify_official_stdout(stdout: &[u8], report: &ComparatorRunReport) -> Result<bool, AppError> {
    let value = std::str::from_utf8(stdout).map_err(|_| {
        run_error(
            "MCL_COMPARATOR_RUN_OUTPUT_INVALID",
            "Comparator stdout is not UTF-8",
        )
    })?;
    if value.contains('\0') || value.contains('\r') {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_OUTPUT_INVALID",
            "Comparator stdout contains NUL or non-LF line endings",
        ));
    }
    let mut positions = [None; 7];
    let mut markers = Vec::new();
    let mut challenge_export = None;
    let mut solution_export = None;
    let mut observed_uid = None;
    let mut uid_lines = 0;
    let mut tcp_denial_lines = 0;
    let mut unix_denial_lines = 0;
    for (line_number, line) in value.lines().enumerate() {
        if let Some(raw_uid) = line.strip_prefix("MATHOS_COMPARATOR_UID=") {
            uid_lines += 1;
            observed_uid = raw_uid.parse::<u32>().ok();
        }
        if line == "MATHOS_LANDRUN_TCP_DENIED=passed" {
            tcp_denial_lines += 1;
        }
        if line == "MATHOS_SYSTEMD_AF_UNIX_DENIED=passed" {
            unix_denial_lines += 1;
        }
        let marker = if line == "Building Challenge" {
            Some(0)
        } else if line.starts_with("Exporting #[") && line.ends_with("] from Challenge") {
            challenge_export = line
                .strip_prefix("Exporting ")
                .and_then(|line| line.strip_suffix(" from Challenge"));
            Some(1)
        } else if line == "Building Solution" {
            Some(2)
        } else if line.starts_with("Exporting #[") && line.ends_with("] from Solution") {
            solution_export = line
                .strip_prefix("Exporting ")
                .and_then(|line| line.strip_suffix(" from Solution"));
            Some(3)
        } else if line == "Running Lean default kernel on solution." {
            Some(4)
        } else if line == "Lean default kernel accepts the solution" {
            Some(5)
        } else if line == "Your solution is okay!" {
            Some(6)
        } else {
            None
        };
        if let Some(index) = marker {
            if positions[index].is_some() {
                return Ok(false);
            }
            positions[index] = Some(line_number);
            markers.push(line.to_owned());
        }
    }
    let ordered = positions
        .iter()
        .copied()
        .collect::<Option<Vec<_>>>()
        .is_some_and(|positions| positions.windows(2).all(|pair| pair[0] < pair[1]));
    Ok(ordered
        && markers == report.execution.success_markers
        && challenge_export.is_some()
        && challenge_export == solution_export
        && uid_lines == 1
        && tcp_denial_lines == 1
        && unix_denial_lines == 1
        && observed_uid == Some(report.workflow.runner_uid))
}

fn verify_file_binding(
    root: &Path,
    binding: &ComparatorRunFileBinding,
    maximum: u64,
) -> Result<(), AppError> {
    let path = root.join(&binding.path);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| AppError::io("inspect Comparator run member", error))?;
    if is_link_or_reparse(&metadata)
        || !metadata.is_file()
        || metadata.len() > maximum
        || metadata.len() != binding.byte_size
    {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_MEMBER_MISMATCH",
            format!(
                "Comparator run member `{}` has the wrong type or size",
                binding.path
            ),
        ));
    }
    if sha256_file(&path)? != binding.content_hash {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_MEMBER_MISMATCH",
            format!(
                "Comparator run member `{}` has the wrong hash",
                binding.path
            ),
        ));
    }
    Ok(())
}

fn verify_inventory(root: &Path) -> Result<(), AppError> {
    let mut root_files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for entry in
        fs::read_dir(root).map_err(|error| AppError::io("read Comparator run bundle", error))?
    {
        let entry = entry.map_err(|error| AppError::io("read Comparator run entry", error))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| AppError::io("inspect Comparator run entry", error))?;
        if is_link_or_reparse(&metadata) {
            return Err(run_error(
                "MCL_COMPARATOR_RUN_INVENTORY_MISMATCH",
                "Comparator run bundle contains a link or reparse point",
            ));
        }
        let name = entry.file_name().into_string().map_err(|_| {
            run_error(
                "MCL_COMPARATOR_RUN_INVENTORY_MISMATCH",
                "Comparator run bundle contains a non-UTF-8 name",
            )
        })?;
        if metadata.is_file() {
            root_files.insert(name);
        } else if metadata.is_dir() {
            directories.insert(name);
        } else {
            return Err(run_error(
                "MCL_COMPARATOR_RUN_INVENTORY_MISMATCH",
                "Comparator run bundle contains a non-file entry",
            ));
        }
    }
    let expected_root = EXPECTED_ROOT_FILES.map(str::to_owned).into_iter().collect();
    if root_files != expected_root || directories != BTreeSet::from(["package".to_owned()]) {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_INVENTORY_MISMATCH",
            "Comparator run bundle root differs from the exact evidence inventory",
        ));
    }
    let package_root = root.join("package");
    let mut package_files = BTreeSet::new();
    for entry in fs::read_dir(&package_root)
        .map_err(|error| AppError::io("read retained Comparator package", error))?
    {
        let entry = entry.map_err(|error| AppError::io("read retained package entry", error))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| AppError::io("inspect retained package entry", error))?;
        if is_link_or_reparse(&metadata) || !metadata.is_file() {
            return Err(run_error(
                "MCL_COMPARATOR_RUN_INVENTORY_MISMATCH",
                "retained Comparator package contains an unsafe entry",
            ));
        }
        package_files.insert(entry.file_name().into_string().map_err(|_| {
            run_error(
                "MCL_COMPARATOR_RUN_INVENTORY_MISMATCH",
                "retained Comparator package contains a non-UTF-8 name",
            )
        })?);
    }
    let expected_package = EXPECTED_PACKAGE_FILES
        .map(str::to_owned)
        .into_iter()
        .collect();
    if package_files != expected_package {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_INVENTORY_MISMATCH",
            "retained Comparator package differs from the exact five-file inventory",
        ));
    }
    Ok(())
}

fn require_real_directory(path: &Path) -> Result<PathBuf, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io("inspect Comparator run directory", error))?;
    if is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_PATH_UNSAFE",
            "Comparator run root is not a real directory",
        ));
    }
    path.canonicalize()
        .map_err(|error| AppError::io("canonicalize Comparator run directory", error))
}

fn read_real_file(path: &Path, maximum: u64, label: &str) -> Result<Vec<u8>, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io("inspect Comparator run file", error))?;
    if is_link_or_reparse(&metadata) || !metadata.is_file() || metadata.len() > maximum {
        return Err(run_error(
            "MCL_COMPARATOR_RUN_PATH_UNSAFE",
            format!("{label} is not a bounded regular file"),
        ));
    }
    fs::read(path).map_err(|error| AppError::io("read Comparator run file", error))
}

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let file =
        File::open(path).map_err(|error| AppError::io("open Comparator run member", error))?;
    let mut reader = BufReader::new(file);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| AppError::io("hash Comparator run member", error))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
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
        return Err(run_error(
            "MCL_COMPARATOR_RUN_EXPECTED_HASH_INVALID",
            format!("{label} is not a lowercase SHA-256 identity"),
        ));
    }
    Ok(())
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn run_error(code: &'static str, message: impl Into<String>) -> AppError {
    AppError::new(
        code,
        message,
        false,
        "Quarantine the run bundle and restore the exact protected, attested Comparator evidence.",
    )
}

#[cfg(test)]
mod tests {
    use jsonschema::validator_for;
    use tempfile::TempDir;

    use super::*;
    use crate::domain::schemas::ExactVersionReference;
    use crate::domain::{
        COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION, COMPARATOR_REPOSITORY,
        ComparatorFileBinding, ComparatorPackageStatus, ComparatorPackageVerification,
        ComparatorRunExecutionBinding, ComparatorRunHarnessBinding, ComparatorRunPackageBinding,
        ComparatorRunPredicates, ComparatorRunSandboxBinding, ComparatorRunToolBinding,
        ComparatorRunWorkflowBinding, ComparatorSourceMemberBinding, ComparatorToolPins,
        LANDRUN_REPOSITORY, LEAN4EXPORT_REPOSITORY,
    };

    #[test]
    fn committed_schema_is_valid_and_closed() {
        let schema: Value = serde_json::from_str(include_str!(
            "../schemas/release/comparator-run-report-1.schema.json"
        ))
        .expect("Comparator run schema JSON");
        validator_for(&schema).expect("valid Comparator run JSON Schema");
        assert_eq!(schema["additionalProperties"], Value::Bool(false));
        assert_eq!(
            schema["properties"]["authoritative"]["const"],
            Value::Bool(false)
        );
    }

    #[test]
    fn accepted_bundle_verifies_offline_and_binary_substitution_fails() {
        let (root, report_hash, package_hash) = accepted_bundle();
        let outcome = verify_comparator_run(ComparatorRunVerificationRequest {
            run_dir: root.path(),
            expected_report_hash: &report_hash,
            expected_package_verification_hash: &package_hash,
        })
        .expect("accepted protected Comparator fixture verifies");
        assert!(outcome.comparator_verified);
        assert!(!outcome.authoritative);
        assert!(outcome.database_independent);

        fs::write(root.path().join("comparator.bin"), b"substituted")
            .expect("substitute Comparator binary");
        assert_eq!(
            verify_comparator_run(ComparatorRunVerificationRequest {
                run_dir: root.path(),
                expected_report_hash: &report_hash,
                expected_package_verification_hash: &package_hash,
            })
            .expect_err("binary substitution rejected")
            .code,
            "MCL_COMPARATOR_RUN_MEMBER_MISMATCH"
        );
    }

    fn accepted_bundle() -> (TempDir, String, String) {
        let root = TempDir::new().expect("Comparator run fixture root");
        fs::create_dir(root.path().join("package")).expect("package directory");
        let challenge = b"theorem Fixture.theorem : True := by\n  sorry\n";
        let solution = b"theorem Fixture.theorem : True := by\n  trivial\n";
        let config = canonical_json(&serde_json::json!({
            "challenge_module": "Challenge",
            "solution_module": "Solution",
            "theorem_names": ["Fixture.theorem"],
            "permitted_axioms": [],
            "enable_nanoda": false
        }))
        .expect("canonical config");
        let formalization = b"schema_version: comparator_formalization/1\n";
        write(root.path(), "package/Challenge.lean", challenge);
        write(root.path(), "package/Solution.lean", solution);
        write(root.path(), "package/config.json", &config);
        write(root.path(), "package/formalization.yaml", formalization);
        let source_formalization = ExactVersionReference {
            object_id: "00000000-0000-4000-8000-000000000001".to_owned(),
            version_hash: "5".repeat(64),
        };
        let file_binding = |path: &str, bytes: &[u8]| ComparatorFileBinding {
            path: path.to_owned(),
            content_hash: sha256(bytes),
            byte_size: bytes.len() as u64,
        };
        let source_binding = ComparatorSourceMemberBinding {
            path: "replay/Submission.lean".to_owned(),
            content_hash: "8".repeat(64),
            byte_size: 1,
        };
        let package_verification = ComparatorPackageVerification {
            schema_version: COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION.to_owned(),
            status: ComparatorPackageStatus::Ready,
            comparator_verified: false,
            authoritative: false,
            source_release_manifest_hash: "4".repeat(64),
            source_formalization: source_formalization.clone(),
            source_formalization_member: source_binding.clone(),
            theorem_source: source_binding.clone(),
            dependency_manifest: ComparatorSourceMemberBinding {
                path: "replay/environment.json".to_owned(),
                ..source_binding
            },
            challenge: file_binding("Challenge.lean", challenge),
            solution: file_binding("Solution.lean", solution),
            config: file_binding("config.json", &config),
            formalization: file_binding("formalization.yaml", formalization),
            declaration_name: "Fixture.theorem".to_owned(),
            lean_toolchain: COMPARATOR_RUN_LEAN_TOOLCHAIN.to_owned(),
            tool_pins: ComparatorToolPins {
                comparator_repository: COMPARATOR_REPOSITORY.to_owned(),
                comparator_commit: COMPARATOR_RUN_COMPARATOR_COMMIT.to_owned(),
                lean4export_repository: LEAN4EXPORT_REPOSITORY.to_owned(),
                lean4export_commit: COMPARATOR_RUN_LEAN4EXPORT_COMMIT.to_owned(),
                landrun_repository: LANDRUN_REPOSITORY.to_owned(),
                landrun_commit: COMPARATOR_RUN_LANDRUN_COMMIT.to_owned(),
            },
            plan_hash: "3".repeat(64),
            input_fingerprint: "2".repeat(64),
        };
        let verification_bytes = canonical_json(
            &serde_json::to_value(&package_verification).expect("package verification value"),
        )
        .expect("canonical package verification");
        write(
            root.path(),
            "package/verification.json",
            &verification_bytes,
        );
        let package_hash = sha256(&verification_bytes);

        let stdout = concat!(
            "MATHOS_SYSTEMD_AF_UNIX_DENIED=passed\n",
            "MATHOS_LANDRUN_TCP_DENIED=passed\n",
            "MATHOS_COMPARATOR_UID=1001\n",
            "Building Challenge\n",
            "Build completed successfully (3 jobs).\n",
            "Exporting #[Fixture.theorem] from Challenge\n",
            "Building Solution\n",
            "Build completed successfully (3 jobs).\n",
            "Exporting #[Fixture.theorem] from Solution\n",
            "Running Lean default kernel on solution.\n",
            "Lean default kernel accepts the solution\n",
            "Your solution is okay!\n"
        );
        let markers = vec![
            "Building Challenge".to_owned(),
            "Exporting #[Fixture.theorem] from Challenge".to_owned(),
            "Building Solution".to_owned(),
            "Exporting #[Fixture.theorem] from Solution".to_owned(),
            "Running Lean default kernel on solution.".to_owned(),
            "Lean default kernel accepts the solution".to_owned(),
            "Your solution is okay!".to_owned(),
        ];
        write(root.path(), "comparator.stdout", stdout.as_bytes());
        write(root.path(), "comparator.stderr", b"");
        write(
            root.path(),
            "systemd.properties",
            b"User=\nNoNewPrivileges=yes\nRestrictAddressFamilies=~AF_UNIX\n",
        );
        write(
            root.path(),
            "landlock-probe.stdout",
            LANDLOCK_PROBE_STDOUT.as_bytes(),
        );
        write(
            root.path(),
            "landlock-probe.stderr",
            b"[landrun] Sandbox config: {BestEffort:false}\n[landrun] Landlock restrictions applied successfully\n",
        );
        write(root.path(), "comparator.bin", b"comparator-binary");
        write(root.path(), "lean4export.bin", b"lean4export-binary");
        write(root.path(), "landrun.bin", b"landrun-binary");
        write(
            root.path(),
            "runner-script.sh",
            include_bytes!("../scripts/comparator-protected-run.sh"),
        );
        write(
            root.path(),
            "network-probe.py",
            include_bytes!("../scripts/comparator-network-probe.py"),
        );
        write(root.path(), "lakefile.toml", LAKEFILE.as_bytes());
        write(
            root.path(),
            "lean-toolchain",
            format!("{COMPARATOR_RUN_LEAN_TOOLCHAIN}\n").as_bytes(),
        );
        write(
            root.path(),
            "lake-manifest.json",
            br#"{"version":"1.2.0","packagesDir":".lake/packages","packages":[],"name":"mathos_comparator_pilot_a","lakeDir":".lake","fixedToolchain":false}"#,
        );

        let total_member_bytes = [
            challenge.len(),
            solution.len(),
            config.len(),
            formalization.len(),
        ]
        .into_iter()
        .sum::<usize>()
            + verification_bytes.len();
        let reprojection = ComparatorPackageVerificationReport {
            verification_hash: package_hash.clone(),
            input_fingerprint: package_verification.input_fingerprint.clone(),
            source_release_manifest_hash: package_verification.source_release_manifest_hash.clone(),
            source_formalization: source_formalization.clone(),
            status: ComparatorPackageStatus::Ready,
            comparator_verified: false,
            authoritative: false,
            member_count: 5,
            total_member_bytes: total_member_bytes as u64,
            database_independent: true,
            inventory_verified: true,
            hashes_verified: true,
            bindings_verified: true,
            deterministic_reprojection: true,
        };
        write(
            root.path(),
            "package-reprojection.json",
            &serde_json::to_vec_pretty(&reprojection).expect("reprojection JSON"),
        );

        let report = ComparatorRunReport {
            schema_version: crate::domain::COMPARATOR_RUN_REPORT_SCHEMA_VERSION.to_owned(),
            classification: ComparatorRunClassification::Accepted,
            comparator_verified: true,
            authoritative: false,
            attestation_required: true,
            package: ComparatorRunPackageBinding {
                verification_hash: package_hash.clone(),
                input_fingerprint: package_verification.input_fingerprint.clone(),
                plan_hash: package_verification.plan_hash.clone(),
                source_release_manifest_hash: package_verification
                    .source_release_manifest_hash
                    .clone(),
                source_formalization,
                declaration_name: package_verification.declaration_name,
                lean_toolchain: COMPARATOR_RUN_LEAN_TOOLCHAIN.to_owned(),
                members: [
                    "package/Challenge.lean",
                    "package/Solution.lean",
                    "package/config.json",
                    "package/formalization.yaml",
                    "package/verification.json",
                ]
                .map(|path| binding(root.path(), path))
                .to_vec(),
            },
            workflow: ComparatorRunWorkflowBinding {
                repository: crate::domain::COMPARATOR_RUN_REPOSITORY.to_owned(),
                repository_id: crate::domain::COMPARATOR_RUN_REPOSITORY_ID.to_owned(),
                workflow_path: crate::domain::COMPARATOR_RUN_WORKFLOW_PATH.to_owned(),
                workflow_ref: crate::domain::COMPARATOR_RUN_WORKFLOW_REF.to_owned(),
                source_ref: crate::domain::COMPARATOR_RUN_SOURCE_REF.to_owned(),
                source_commit_sha: "6".repeat(40),
                source_tree_sha: "7".repeat(40),
                run_id: "123456".to_owned(),
                run_attempt: 1,
                job: crate::domain::COMPARATOR_RUN_JOB.to_owned(),
                protected_ref: true,
                github_hosted: true,
                runner_os: "Linux".to_owned(),
                runner_arch: "X64".to_owned(),
                runner_image: "ubuntu24".to_owned(),
                kernel_release: "6.11.0".to_owned(),
                systemd_version: "systemd 255".to_owned(),
                runner_uid: 1001,
            },
            tools: vec![
                ComparatorRunToolBinding {
                    name: "comparator".to_owned(),
                    repository: COMPARATOR_REPOSITORY.to_owned(),
                    commit: COMPARATOR_RUN_COMPARATOR_COMMIT.to_owned(),
                    source_tree: crate::domain::COMPARATOR_RUN_COMPARATOR_TREE.to_owned(),
                    build_toolchain: COMPARATOR_RUN_LEAN_TOOLCHAIN.to_owned(),
                    binary: binding(root.path(), "comparator.bin"),
                },
                ComparatorRunToolBinding {
                    name: "lean4export".to_owned(),
                    repository: LEAN4EXPORT_REPOSITORY.to_owned(),
                    commit: COMPARATOR_RUN_LEAN4EXPORT_COMMIT.to_owned(),
                    source_tree: crate::domain::COMPARATOR_RUN_LEAN4EXPORT_TREE.to_owned(),
                    build_toolchain: COMPARATOR_RUN_LEAN_TOOLCHAIN.to_owned(),
                    binary: binding(root.path(), "lean4export.bin"),
                },
                ComparatorRunToolBinding {
                    name: "landrun".to_owned(),
                    repository: LANDRUN_REPOSITORY.to_owned(),
                    commit: COMPARATOR_RUN_LANDRUN_COMMIT.to_owned(),
                    source_tree: crate::domain::COMPARATOR_RUN_LANDRUN_TREE.to_owned(),
                    build_toolchain: crate::domain::COMPARATOR_RUN_GO_TOOLCHAIN.to_owned(),
                    binary: binding(root.path(), "landrun.bin"),
                },
            ],
            harness: ComparatorRunHarnessBinding {
                project_name: COMPARATOR_RUN_PROJECT_NAME.to_owned(),
                challenge_module: "Challenge".to_owned(),
                solution_module: "Solution".to_owned(),
                files: ["lake-manifest.json", "lakefile.toml", "lean-toolchain"]
                    .map(|path| binding(root.path(), path))
                    .to_vec(),
                manifest_created_before_solution_copy: true,
                source_file_count: 3,
                no_lake_directory_before_run: true,
                no_olean_before_run: true,
            },
            sandbox: ComparatorRunSandboxBinding {
                real_landrun: true,
                fake_landrun: false,
                landlock_abi: 5,
                strict_probe_without_best_effort: true,
                comparator_best_effort_after_strict_probe: true,
                systemd_user_manager: true,
                restrict_address_families: "~AF_UNIX".to_owned(),
                no_new_privileges: true,
                non_root: true,
                tcp_network_denied: true,
                unix_socket_denied: true,
                network_isolated: true,
            },
            execution: ComparatorRunExecutionBinding {
                command_profile: crate::domain::COMPARATOR_RUN_COMMAND_PROFILE.to_owned(),
                exit_code: 0,
                timed_out: false,
                stdout: binding(root.path(), "comparator.stdout"),
                stderr: binding(root.path(), "comparator.stderr"),
                systemd_properties: binding(root.path(), "systemd.properties"),
                landlock_probe_stdout: binding(root.path(), "landlock-probe.stdout"),
                landlock_probe_stderr: binding(root.path(), "landlock-probe.stderr"),
                package_reprojection: binding(root.path(), "package-reprojection.json"),
                runner_script: binding(root.path(), "runner-script.sh"),
                network_probe: binding(root.path(), "network-probe.py"),
                success_markers: markers,
            },
            predicates: ComparatorRunPredicates {
                package_reprojected: true,
                tool_sources_and_binaries_verified: true,
                fresh_harness_verified: true,
                landlock_strict_probe_passed: true,
                systemd_controls_verified: true,
                network_isolation_verified: true,
                non_root_verified: true,
                output_bounds_verified: true,
                unexpected_stderr_absent: true,
                success_markers_ordered_unique: true,
                statement_match_verified: true,
                axioms_verified: true,
                lean_kernel_verified: true,
            },
        };
        let report_value = serde_json::to_value(&report).expect("report value");
        let schema: Value = serde_json::from_str(include_str!(
            "../schemas/release/comparator-run-report-1.schema.json"
        ))
        .expect("Comparator run schema");
        assert!(
            validator_for(&schema)
                .expect("schema")
                .is_valid(&report_value)
        );
        let report_bytes = canonical_json(&report_value).expect("canonical report");
        write(root.path(), "report.json", &report_bytes);
        let report_hash = sha256(&report_bytes);
        (root, report_hash, package_hash)
    }

    fn write(root: &Path, relative: &str, bytes: &[u8]) {
        fs::write(root.join(relative), bytes).expect("write Comparator run fixture member");
    }

    fn binding(root: &Path, relative: &str) -> ComparatorRunFileBinding {
        let bytes = fs::read(root.join(relative)).expect("read fixture binding member");
        ComparatorRunFileBinding {
            path: relative.to_owned(),
            content_hash: sha256(&bytes),
            byte_size: bytes.len() as u64,
        }
    }
}
