use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::schemas::ExactVersionReference;
use crate::error::AppError;

pub const COMPARATOR_RUN_REPORT_SCHEMA_VERSION: &str = "comparator_run_report/1";
pub const COMPARATOR_RUN_LEAN_TOOLCHAIN: &str = "leanprover/lean4:v4.32.0";
pub const COMPARATOR_RUN_GO_TOOLCHAIN: &str = "go version go1.24.2 linux/amd64";
pub const COMPARATOR_RUN_REPOSITORY: &str = "Mnehmos/MathOS";
pub const COMPARATOR_RUN_REPOSITORY_ID: &str = "1305399818";
pub const COMPARATOR_RUN_WORKFLOW_PATH: &str = ".github/workflows/publication.yml";
pub const COMPARATOR_RUN_WORKFLOW_REF: &str =
    "Mnehmos/MathOS/.github/workflows/publication.yml@refs/heads/main";
pub const COMPARATOR_RUN_SOURCE_REF: &str = "refs/heads/main";
pub const COMPARATOR_RUN_JOB: &str = "comparator";
pub const COMPARATOR_RUN_COMMAND_PROFILE: &str = "official_comparator_systemd_landrun_v1";
pub const COMPARATOR_RUN_PROJECT_NAME: &str = "mathos_comparator_pilot_a";

pub const COMPARATOR_RUN_COMPARATOR_COMMIT: &str = "68a064109f01c08f47c8edc9f51d6a2bbffaa188";
pub const COMPARATOR_RUN_COMPARATOR_TREE: &str = "0bb408593d6e5f625db53b3be16e3f1cc91a7524";
pub const COMPARATOR_RUN_LEAN4EXPORT_COMMIT: &str = "af5aa64bb914c3c2c781f378088dbd38acf4f804";
pub const COMPARATOR_RUN_LEAN4EXPORT_TREE: &str = "5058a7945d24656600ca05917e3c8c174485bcf5";
pub const COMPARATOR_RUN_LANDRUN_COMMIT: &str = "5ed4a3db3a4ad930d577215c6b9abaa19df7f99f";
pub const COMPARATOR_RUN_LANDRUN_TREE: &str = "890013a5099a92792cbacd2cfff91af3f13cec9c";

pub const MAX_COMPARATOR_RUN_TEXT_BYTES: u64 = 4 * 1_048_576;
pub const MAX_COMPARATOR_RUN_BINARY_BYTES: u64 = 512 * 1_048_576;
pub const MAX_ACCEPTED_COMPARATOR_STDOUT_BYTES: u64 = 256 * 1_024;
pub const MAX_ACCEPTED_COMPARATOR_STDERR_BYTES: u64 = 64 * 1_024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparatorRunClassification {
    Accepted,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunFileBinding {
    pub path: String,
    pub content_hash: String,
    pub byte_size: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunPackageBinding {
    pub verification_hash: String,
    pub input_fingerprint: String,
    pub plan_hash: String,
    pub source_release_manifest_hash: String,
    pub source_formalization: ExactVersionReference,
    pub declaration_name: String,
    pub lean_toolchain: String,
    pub members: Vec<ComparatorRunFileBinding>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunWorkflowBinding {
    pub repository: String,
    pub repository_id: String,
    pub workflow_path: String,
    pub workflow_ref: String,
    pub source_ref: String,
    pub source_commit_sha: String,
    pub source_tree_sha: String,
    pub run_id: String,
    pub run_attempt: u32,
    pub job: String,
    pub protected_ref: bool,
    pub github_hosted: bool,
    pub runner_os: String,
    pub runner_arch: String,
    pub runner_image: String,
    pub kernel_release: String,
    pub systemd_version: String,
    pub runner_uid: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunToolBinding {
    pub name: String,
    pub repository: String,
    pub commit: String,
    pub source_tree: String,
    pub build_toolchain: String,
    pub binary: ComparatorRunFileBinding,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunHarnessBinding {
    pub project_name: String,
    pub challenge_module: String,
    pub solution_module: String,
    pub files: Vec<ComparatorRunFileBinding>,
    pub manifest_created_before_solution_copy: bool,
    pub source_file_count: u32,
    pub no_lake_directory_before_run: bool,
    pub no_olean_before_run: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunSandboxBinding {
    pub real_landrun: bool,
    pub fake_landrun: bool,
    pub landlock_abi: u32,
    pub strict_probe_without_best_effort: bool,
    pub comparator_best_effort_after_strict_probe: bool,
    pub systemd_user_manager: bool,
    pub restrict_address_families: String,
    pub no_new_privileges: bool,
    pub non_root: bool,
    pub tcp_network_denied: bool,
    pub unix_socket_denied: bool,
    pub network_isolated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunExecutionBinding {
    pub command_profile: String,
    pub exit_code: i32,
    pub timed_out: bool,
    pub stdout: ComparatorRunFileBinding,
    pub stderr: ComparatorRunFileBinding,
    pub systemd_properties: ComparatorRunFileBinding,
    pub landlock_probe_stdout: ComparatorRunFileBinding,
    pub landlock_probe_stderr: ComparatorRunFileBinding,
    pub package_reprojection: ComparatorRunFileBinding,
    pub runner_script: ComparatorRunFileBinding,
    pub network_probe: ComparatorRunFileBinding,
    pub success_markers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunPredicates {
    pub package_reprojected: bool,
    pub tool_sources_and_binaries_verified: bool,
    pub fresh_harness_verified: bool,
    pub landlock_strict_probe_passed: bool,
    pub systemd_controls_verified: bool,
    pub network_isolation_verified: bool,
    pub non_root_verified: bool,
    pub output_bounds_verified: bool,
    pub unexpected_stderr_absent: bool,
    pub success_markers_ordered_unique: bool,
    pub statement_match_verified: bool,
    pub axioms_verified: bool,
    pub lean_kernel_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparatorRunReport {
    pub schema_version: String,
    pub classification: ComparatorRunClassification,
    pub comparator_verified: bool,
    pub authoritative: bool,
    pub attestation_required: bool,
    pub package: ComparatorRunPackageBinding,
    pub workflow: ComparatorRunWorkflowBinding,
    pub tools: Vec<ComparatorRunToolBinding>,
    pub harness: ComparatorRunHarnessBinding,
    pub sandbox: ComparatorRunSandboxBinding,
    pub execution: ComparatorRunExecutionBinding,
    pub predicates: ComparatorRunPredicates,
}

impl ComparatorRunFileBinding {
    pub fn validate(&self, expected_path: &str, maximum: u64) -> Result<(), AppError> {
        if self.path != expected_path || self.byte_size > maximum || !is_hash(&self.content_hash) {
            return Err(run_error(format!(
                "Comparator run binding for `{expected_path}` has the wrong path, hash, or size"
            )));
        }
        Ok(())
    }
}

impl ComparatorRunPredicates {
    pub fn all(&self) -> bool {
        self.package_reprojected
            && self.tool_sources_and_binaries_verified
            && self.fresh_harness_verified
            && self.landlock_strict_probe_passed
            && self.systemd_controls_verified
            && self.network_isolation_verified
            && self.non_root_verified
            && self.output_bounds_verified
            && self.unexpected_stderr_absent
            && self.success_markers_ordered_unique
            && self.statement_match_verified
            && self.axioms_verified
            && self.lean_kernel_verified
    }
}

impl ComparatorRunReport {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != COMPARATOR_RUN_REPORT_SCHEMA_VERSION
            || self.authoritative
            || !self.attestation_required
            || self.comparator_verified
                != (self.classification == ComparatorRunClassification::Accepted)
            || self.workflow.repository != COMPARATOR_RUN_REPOSITORY
            || self.workflow.repository_id != COMPARATOR_RUN_REPOSITORY_ID
            || self.workflow.workflow_path != COMPARATOR_RUN_WORKFLOW_PATH
            || self.workflow.workflow_ref != COMPARATOR_RUN_WORKFLOW_REF
            || self.workflow.source_ref != COMPARATOR_RUN_SOURCE_REF
            || self.workflow.job != COMPARATOR_RUN_JOB
            || !self.workflow.protected_ref
            || !self.workflow.github_hosted
            || self.workflow.runner_os != "Linux"
            || self.workflow.runner_arch != "X64"
            || self.workflow.runner_uid == 0
            || self.workflow.run_attempt == 0
            || !decimal(&self.workflow.run_id)
            || !is_commit(&self.workflow.source_commit_sha)
            || !is_commit(&self.workflow.source_tree_sha)
            || !bounded(&self.workflow.runner_image, 256)
            || !bounded(&self.workflow.kernel_release, 256)
            || !bounded(&self.workflow.systemd_version, 256)
            || (self.classification == ComparatorRunClassification::Accepted
                && (self.execution.exit_code != 0
                    || self.execution.timed_out
                    || self.execution.stderr.byte_size != 0
                    || !self.sandbox.network_isolated
                    || !self.predicates.all()))
        {
            return Err(run_error(
                "Comparator run report violates the protected GitHub workflow or non-authority boundary",
            ));
        }

        self.validate_package()?;
        self.validate_tools()?;
        self.validate_harness()?;
        self.validate_sandbox()?;
        self.validate_execution()?;
        Ok(())
    }

    fn validate_package(&self) -> Result<(), AppError> {
        if !is_hash(&self.package.verification_hash)
            || !is_hash(&self.package.input_fingerprint)
            || !is_hash(&self.package.plan_hash)
            || !is_hash(&self.package.source_release_manifest_hash)
            || uuid::Uuid::parse_str(&self.package.source_formalization.object_id).is_err()
            || !is_hash(&self.package.source_formalization.version_hash)
            || !lean_name(&self.package.declaration_name)
            || self.package.lean_toolchain != COMPARATOR_RUN_LEAN_TOOLCHAIN
        {
            return Err(run_error("Comparator run package identity is invalid"));
        }
        validate_exact_bindings(
            &self.package.members,
            &[
                "package/Challenge.lean",
                "package/Solution.lean",
                "package/config.json",
                "package/formalization.yaml",
                "package/verification.json",
            ],
            MAX_COMPARATOR_RUN_TEXT_BYTES,
        )
    }

    fn validate_tools(&self) -> Result<(), AppError> {
        let expected = [
            (
                "comparator",
                super::COMPARATOR_REPOSITORY,
                COMPARATOR_RUN_COMPARATOR_COMMIT,
                COMPARATOR_RUN_COMPARATOR_TREE,
                "comparator.bin",
            ),
            (
                "lean4export",
                super::LEAN4EXPORT_REPOSITORY,
                COMPARATOR_RUN_LEAN4EXPORT_COMMIT,
                COMPARATOR_RUN_LEAN4EXPORT_TREE,
                "lean4export.bin",
            ),
            (
                "landrun",
                super::LANDRUN_REPOSITORY,
                COMPARATOR_RUN_LANDRUN_COMMIT,
                COMPARATOR_RUN_LANDRUN_TREE,
                "landrun.bin",
            ),
        ];
        if self.tools.len() != expected.len() {
            return Err(run_error("Comparator run tool inventory is not exact"));
        }
        for (tool, (name, repository, commit, tree, binary)) in self.tools.iter().zip(expected) {
            if tool.name != name
                || tool.repository != repository
                || tool.commit != commit
                || tool.source_tree != tree
                || tool.binary.byte_size == 0
                || (name != "landrun" && tool.build_toolchain != COMPARATOR_RUN_LEAN_TOOLCHAIN)
                || (name == "landrun" && tool.build_toolchain != COMPARATOR_RUN_GO_TOOLCHAIN)
            {
                return Err(run_error(format!(
                    "Comparator run tool `{name}` does not use the fixed source identity"
                )));
            }
            tool.binary
                .validate(binary, MAX_COMPARATOR_RUN_BINARY_BYTES)?;
        }
        Ok(())
    }

    fn validate_harness(&self) -> Result<(), AppError> {
        if self.harness.project_name != COMPARATOR_RUN_PROJECT_NAME
            || self.harness.challenge_module != "Challenge"
            || self.harness.solution_module != "Solution"
            || !self.harness.manifest_created_before_solution_copy
            || self.harness.source_file_count != 3
            || !self.harness.no_lake_directory_before_run
            || !self.harness.no_olean_before_run
        {
            return Err(run_error(
                "Comparator run harness is not the pristine fixed project",
            ));
        }
        validate_exact_bindings(
            &self.harness.files,
            &["lake-manifest.json", "lakefile.toml", "lean-toolchain"],
            MAX_COMPARATOR_RUN_TEXT_BYTES,
        )
    }

    fn validate_sandbox(&self) -> Result<(), AppError> {
        if !self.sandbox.real_landrun
            || self.sandbox.fake_landrun
            || self.sandbox.landlock_abi != 5
            || !self.sandbox.strict_probe_without_best_effort
            || !self.sandbox.comparator_best_effort_after_strict_probe
            || !self.sandbox.systemd_user_manager
            || self.sandbox.restrict_address_families != "~AF_UNIX"
            || !self.sandbox.no_new_privileges
            || !self.sandbox.non_root
            || self.sandbox.network_isolated
                != (self.sandbox.tcp_network_denied && self.sandbox.unix_socket_denied)
        {
            return Err(run_error(
                "Comparator run sandbox predicates are not fail-closed",
            ));
        }
        Ok(())
    }

    fn validate_execution(&self) -> Result<(), AppError> {
        if self.execution.command_profile != COMPARATOR_RUN_COMMAND_PROFILE
            || !(0..=255).contains(&self.execution.exit_code)
            || self.execution.success_markers.len() > 16
            || self
                .execution
                .success_markers
                .iter()
                .any(|value| !bounded(value, 8_192) || value.contains('\n') || value.contains('\r'))
        {
            return Err(run_error(
                "Comparator execution metadata is invalid or unbounded",
            ));
        }
        self.execution
            .stdout
            .validate("comparator.stdout", MAX_COMPARATOR_RUN_TEXT_BYTES)?;
        self.execution
            .stderr
            .validate("comparator.stderr", MAX_COMPARATOR_RUN_TEXT_BYTES)?;
        self.execution
            .systemd_properties
            .validate("systemd.properties", MAX_COMPARATOR_RUN_TEXT_BYTES)?;
        self.execution
            .landlock_probe_stdout
            .validate("landlock-probe.stdout", MAX_COMPARATOR_RUN_TEXT_BYTES)?;
        self.execution
            .landlock_probe_stderr
            .validate("landlock-probe.stderr", MAX_COMPARATOR_RUN_TEXT_BYTES)?;
        self.execution
            .package_reprojection
            .validate("package-reprojection.json", MAX_COMPARATOR_RUN_TEXT_BYTES)?;
        self.execution
            .runner_script
            .validate("runner-script.sh", MAX_COMPARATOR_RUN_TEXT_BYTES)?;
        self.execution
            .network_probe
            .validate("network-probe.py", MAX_COMPARATOR_RUN_TEXT_BYTES)
    }
}

fn validate_exact_bindings(
    bindings: &[ComparatorRunFileBinding],
    expected: &[&str],
    maximum: u64,
) -> Result<(), AppError> {
    if bindings.len() != expected.len() {
        return Err(run_error("Comparator run file inventory is not exact"));
    }
    for (binding, expected_path) in bindings.iter().zip(expected) {
        binding.validate(expected_path, maximum)?;
    }
    let unique = bindings
        .iter()
        .map(|binding| binding.path.as_str())
        .collect::<BTreeSet<_>>();
    if unique.len() != bindings.len() {
        return Err(run_error(
            "Comparator run file inventory contains duplicates",
        ));
    }
    Ok(())
}

fn bounded(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.contains('\0')
}

fn decimal(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn lean_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\''))
        })
}

fn run_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_COMPARATOR_RUN_REPORT_INVALID",
        message,
        false,
        "Restore the exact canonical protected Comparator run bundle and attested report.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_requires_verification_and_non_authority_is_structural() {
        let mut report = fixture();
        report.validate().expect("closed accepted report");

        report.authoritative = true;
        assert_eq!(
            report.validate().expect_err("authority rejected").code,
            "MCL_COMPARATOR_RUN_REPORT_INVALID"
        );

        report.authoritative = false;
        report.comparator_verified = false;
        assert_eq!(
            report
                .validate()
                .expect_err("accepted without Comparator verification rejected")
                .code,
            "MCL_COMPARATOR_RUN_REPORT_INVALID"
        );
    }

    fn binding(path: &str) -> ComparatorRunFileBinding {
        ComparatorRunFileBinding {
            path: path.to_owned(),
            content_hash: "a".repeat(64),
            byte_size: 1,
        }
    }

    fn fixture() -> ComparatorRunReport {
        ComparatorRunReport {
            schema_version: COMPARATOR_RUN_REPORT_SCHEMA_VERSION.to_owned(),
            classification: ComparatorRunClassification::Accepted,
            comparator_verified: true,
            authoritative: false,
            attestation_required: true,
            package: ComparatorRunPackageBinding {
                verification_hash: "1".repeat(64),
                input_fingerprint: "2".repeat(64),
                plan_hash: "3".repeat(64),
                source_release_manifest_hash: "4".repeat(64),
                source_formalization: ExactVersionReference {
                    object_id: "00000000-0000-4000-8000-000000000001".to_owned(),
                    version_hash: "5".repeat(64),
                },
                declaration_name: "Fixture.theorem'".to_owned(),
                lean_toolchain: COMPARATOR_RUN_LEAN_TOOLCHAIN.to_owned(),
                members: [
                    "package/Challenge.lean",
                    "package/Solution.lean",
                    "package/config.json",
                    "package/formalization.yaml",
                    "package/verification.json",
                ]
                .map(binding)
                .to_vec(),
            },
            workflow: ComparatorRunWorkflowBinding {
                repository: COMPARATOR_RUN_REPOSITORY.to_owned(),
                repository_id: COMPARATOR_RUN_REPOSITORY_ID.to_owned(),
                workflow_path: COMPARATOR_RUN_WORKFLOW_PATH.to_owned(),
                workflow_ref: COMPARATOR_RUN_WORKFLOW_REF.to_owned(),
                source_ref: COMPARATOR_RUN_SOURCE_REF.to_owned(),
                source_commit_sha: "6".repeat(40),
                source_tree_sha: "7".repeat(40),
                run_id: "123".to_owned(),
                run_attempt: 1,
                job: COMPARATOR_RUN_JOB.to_owned(),
                protected_ref: true,
                github_hosted: true,
                runner_os: "Linux".to_owned(),
                runner_arch: "X64".to_owned(),
                runner_image: "ubuntu24".to_owned(),
                kernel_release: "6.11.0".to_owned(),
                systemd_version: "255".to_owned(),
                runner_uid: 1001,
            },
            tools: vec![
                ComparatorRunToolBinding {
                    name: "comparator".to_owned(),
                    repository: super::super::COMPARATOR_REPOSITORY.to_owned(),
                    commit: COMPARATOR_RUN_COMPARATOR_COMMIT.to_owned(),
                    source_tree: COMPARATOR_RUN_COMPARATOR_TREE.to_owned(),
                    build_toolchain: COMPARATOR_RUN_LEAN_TOOLCHAIN.to_owned(),
                    binary: binding("comparator.bin"),
                },
                ComparatorRunToolBinding {
                    name: "lean4export".to_owned(),
                    repository: super::super::LEAN4EXPORT_REPOSITORY.to_owned(),
                    commit: COMPARATOR_RUN_LEAN4EXPORT_COMMIT.to_owned(),
                    source_tree: COMPARATOR_RUN_LEAN4EXPORT_TREE.to_owned(),
                    build_toolchain: COMPARATOR_RUN_LEAN_TOOLCHAIN.to_owned(),
                    binary: binding("lean4export.bin"),
                },
                ComparatorRunToolBinding {
                    name: "landrun".to_owned(),
                    repository: super::super::LANDRUN_REPOSITORY.to_owned(),
                    commit: COMPARATOR_RUN_LANDRUN_COMMIT.to_owned(),
                    source_tree: COMPARATOR_RUN_LANDRUN_TREE.to_owned(),
                    build_toolchain: COMPARATOR_RUN_GO_TOOLCHAIN.to_owned(),
                    binary: binding("landrun.bin"),
                },
            ],
            harness: ComparatorRunHarnessBinding {
                project_name: COMPARATOR_RUN_PROJECT_NAME.to_owned(),
                challenge_module: "Challenge".to_owned(),
                solution_module: "Solution".to_owned(),
                files: ["lake-manifest.json", "lakefile.toml", "lean-toolchain"]
                    .map(binding)
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
                command_profile: COMPARATOR_RUN_COMMAND_PROFILE.to_owned(),
                exit_code: 0,
                timed_out: false,
                stdout: binding("comparator.stdout"),
                stderr: ComparatorRunFileBinding {
                    byte_size: 0,
                    ..binding("comparator.stderr")
                },
                systemd_properties: binding("systemd.properties"),
                landlock_probe_stdout: binding("landlock-probe.stdout"),
                landlock_probe_stderr: binding("landlock-probe.stderr"),
                package_reprojection: binding("package-reprojection.json"),
                runner_script: binding("runner-script.sh"),
                network_probe: binding("network-probe.py"),
                success_markers: vec!["Building Challenge".to_owned()],
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
        }
    }
}
