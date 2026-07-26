use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::domain::{EnvironmentManifest, EnvironmentPlatform, TrustProfile, VerifierExecutable};
use crate::error::AppError;

mod project;

pub use project::{MaterializedLeanProject, materialize_lean_project};

const MATHLIB_CACHE_PREPARATION_COMMANDS: [[&str; 3]; 2] =
    [["exe", "cache", "get-"], ["exe", "cache", "unpack"]];
const MATHLIB_CACHE_DIRECTORY: &str = ".mathlib-cache";

pub const FORBIDDEN_SOURCE_TOKENS: &[&str] = &[
    "admit",
    "axiom",
    "builtin_initialize",
    "constant",
    "elab",
    "eval",
    "extern",
    "implemented_by",
    "include_bytes",
    "include_str",
    "initialize",
    "macro",
    "native_decide",
    "reduce",
    "run_cmd",
    "run_tac",
    "sorry",
    "sorryAx",
    "syntax",
    "unsafe",
];

#[derive(Debug)]
pub struct LeanProcessResult {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration_milliseconds: u64,
    pub timed_out: bool,
    pub output_limit_exceeded: bool,
    pub observed_toolchain_version: String,
    pub memory_limit_enforced: bool,
    pub network_isolation_enforced: bool,
}

pub fn scan_forbidden_source_token(bytes: &[u8]) -> Result<Option<String>, AppError> {
    let source = std::str::from_utf8(bytes).map_err(|error| {
        AppError::new(
            "MCL_VERIFIER_SOURCE_INVALID",
            format!("Lean source is not UTF-8: {error}"),
            false,
            "Ingest a valid UTF-8 Lean source artifact.",
        )
    })?;
    let identifiers = lean_identifiers_without_comments_or_strings(source);
    Ok(identifiers
        .into_iter()
        .find(|identifier| FORBIDDEN_SOURCE_TOKENS.contains(&identifier.as_str())))
}

pub fn parse_axiom_dependencies(
    declaration_name: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Vec<String>, AppError> {
    if declaration_name.is_empty()
        || declaration_name.len() > 256
        || !is_audit_name(declaration_name)
    {
        return Err(audit_output_error(
            "audit declaration name is not a bounded Lean name",
        ));
    }
    let stdout = std::str::from_utf8(stdout)
        .map_err(|error| audit_output_error(format!("audit stdout is not UTF-8: {error}")))?;
    let stderr = std::str::from_utf8(stderr)
        .map_err(|error| audit_output_error(format!("audit stderr is not UTF-8: {error}")))?;
    let output = format!("{stdout}\n{stderr}");
    let no_axioms = format!("'{declaration_name}' does not depend on any axioms");
    let list_prefix = format!("'{declaration_name}' depends on axioms: [");
    let no_axiom_count = output.matches(&no_axioms).count();
    let list_count = output.matches(&list_prefix).count();
    if no_axiom_count > 0 && list_count == 0 && no_axiom_count <= 16 {
        return Ok(Vec::new());
    }
    if no_axiom_count != 0 || !(1..=16).contains(&list_count) {
        return Err(audit_output_error(
            "audit output did not contain a bounded consistent declaration-specific axiom result",
        ));
    }
    let mut remaining = output.as_str();
    let mut observed = None;
    for _ in 0..list_count {
        let marker = remaining
            .find(&list_prefix)
            .ok_or_else(|| audit_output_error("audit axiom marker count changed while parsing"))?;
        let tail = &remaining[marker + list_prefix.len()..];
        let end = tail.find(']').ok_or_else(|| {
            audit_output_error("audit axiom list did not contain a closing bracket")
        })?;
        let raw = &tail[..end];
        if raw.trim().is_empty() {
            return Err(audit_output_error(
                "audit axiom list was empty instead of using the no-axioms result",
            ));
        }
        let mut axioms = raw
            .split(',')
            .map(str::trim)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if axioms.len() > 256 || axioms.iter().any(|name| !is_audit_name(name)) {
            return Err(audit_output_error(
                "audit axiom list was malformed or excessive",
            ));
        }
        let original_len = axioms.len();
        axioms.sort();
        axioms.dedup();
        if axioms.len() != original_len {
            return Err(audit_output_error("audit axiom list contained duplicates"));
        }
        if observed.as_ref().is_some_and(|prior| prior != &axioms) {
            return Err(audit_output_error("repeated audit axiom results disagree"));
        }
        observed = Some(axioms);
        remaining = &tail[end + 1..];
    }
    observed.ok_or_else(|| audit_output_error("audit axiom result was absent"))
}

fn is_audit_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\''))
        })
}

fn audit_output_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_AUDIT_OUTPUT_INVALID",
        message,
        false,
        "Quarantine the audit output and rerun the exact audit job with the pinned Lean toolchain.",
    )
}

pub fn execute_lean(
    lean_command: &str,
    lake_command: &str,
    workspace: &Path,
    module_file_name: &str,
    environment: &EnvironmentManifest,
) -> Result<LeanProcessResult, AppError> {
    execute_lean_with_profile(
        lean_command,
        lake_command,
        workspace,
        module_file_name,
        None,
        environment,
    )
}

pub fn execute_lake_project(
    lean_command: &str,
    lake_command: &str,
    workspace: &Path,
    driver_file_name: &str,
    project: &crate::domain::LeanProjectBinding,
    environment: &EnvironmentManifest,
) -> Result<LeanProcessResult, AppError> {
    project.validate()?;
    execute_lean_with_profile(
        lean_command,
        lake_command,
        workspace,
        driver_file_name,
        Some(&project.module_path),
        environment,
    )
}

/// Replays an already-authoritative publication artifact without promoting new authority.
/// Publication-profile replays retain the same verifier-controlled isolation as the
/// publication candidate.
pub fn execute_release_lean(
    lean_command: &str,
    lake_command: &str,
    workspace: &Path,
    module_file_name: &str,
    environment: &EnvironmentManifest,
) -> Result<LeanProcessResult, AppError> {
    execute_lean_with_profile(
        lean_command,
        lake_command,
        workspace,
        module_file_name,
        None,
        environment,
    )
}

pub fn execute_release_lake_project(
    lean_command: &str,
    lake_command: &str,
    workspace: &Path,
    driver_file_name: &str,
    project: &crate::domain::LeanProjectBinding,
    environment: &EnvironmentManifest,
) -> Result<LeanProcessResult, AppError> {
    project.validate()?;
    execute_lean_with_profile(
        lean_command,
        lake_command,
        workspace,
        driver_file_name,
        Some(&project.module_path),
        environment,
    )
}

fn execute_lean_with_profile(
    lean_command: &str,
    lake_command: &str,
    workspace: &Path,
    module_file_name: &str,
    project_build_target: Option<&str>,
    environment: &EnvironmentManifest,
) -> Result<LeanProcessResult, AppError> {
    environment.validate()?;
    validate_platform(environment.platform)?;
    let lean_num_threads = lean_num_threads_configuration(environment.resource_limits.concurrency);
    if !matches!(lean_command, "lean" | "lean.exe") {
        return Err(AppError::new(
            "MCL_VERIFIER_COMMAND_REJECTED",
            format!("verifier executable `{lean_command}` is not allowlisted"),
            false,
            "Configure only the platform Lean executable name.",
        ));
    }
    let version_capture = run_bounded_profiled_process(
        lean_command,
        &["--version"],
        workspace,
        Duration::from_secs(environment.resource_limits.timeout_seconds.min(30)),
        4_096,
        environment,
        &lean_num_threads,
    )?;
    let observed_toolchain_version =
        String::from_utf8(version_capture.stdout.clone()).map_err(|error| {
            AppError::new(
                "MCL_VERIFIER_VERSION_INVALID",
                format!("Lean version output is not UTF-8: {error}"),
                false,
                "Install the exact pinned Lean toolchain and retry.",
            )
        })?;
    let expected = environment
        .lean_toolchain
        .strip_prefix("leanprover/lean4:v")
        .expect("validated Lean toolchain");
    let version_diagnostic = format!(
        "{}{}",
        observed_toolchain_version.trim(),
        String::from_utf8_lossy(&version_capture.stderr).trim()
    );
    if version_capture.timed_out
        || version_capture.output_limit_exceeded
        || version_capture.exit_code != Some(0)
        || !observed_toolchain_version.contains(&format!("version {expected},"))
    {
        return Err(AppError::new(
            "MCL_VERIFIER_VERSION_MISMATCH",
            format!(
                "observed Lean version does not match pinned release {expected}: {}",
                version_diagnostic
            ),
            false,
            "Activate the exact registered Lean toolchain before running the worker.",
        ));
    }

    let (command, arguments): (&str, Vec<&str>) = match (
        environment.verifier_command.executable,
        project_build_target,
    ) {
        (VerifierExecutable::Lean, None) => (lean_command, vec![module_file_name]),
        (VerifierExecutable::Lake, Some(build_target)) => {
            if !matches!(lake_command, "lake" | "lake.exe") {
                return Err(AppError::new(
                    "MCL_VERIFIER_COMMAND_REJECTED",
                    format!("verifier executable `{lake_command}` is not allowlisted"),
                    false,
                    "Configure only the platform Lake executable name.",
                ));
            }
            let started = Instant::now();
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let total_timeout = Duration::from_secs(environment.resource_limits.timeout_seconds);
            if environment.dependency_preparation
                == Some(crate::domain::DependencyPreparation::MathlibCacheGet)
            {
                // Keep transfer and extraction as separate fixed commands. Mathlib's pipelined
                // `get` mode can race downloaded cache files on Windows, while `get-` followed by
                // `unpack` preserves the same pinned cache checks without caller-controlled input.
                for arguments in MATHLIB_CACHE_PREPARATION_COMMANDS {
                    let elapsed = started.elapsed();
                    let Some(preparation_timeout) = total_timeout.checked_sub(elapsed) else {
                        return Ok(LeanProcessResult {
                            exit_code: None,
                            stdout,
                            stderr,
                            duration_milliseconds: elapsed
                                .as_millis()
                                .try_into()
                                .unwrap_or(u64::MAX),
                            timed_out: true,
                            output_limit_exceeded: false,
                            observed_toolchain_version: observed_toolchain_version
                                .trim()
                                .to_owned(),
                            memory_limit_enforced: false,
                            network_isolation_enforced: false,
                        });
                    };
                    let consumed = stdout
                        .len()
                        .saturating_add(stderr.len())
                        .try_into()
                        .unwrap_or(u64::MAX);
                    let Some(preparation_output) = environment
                        .resource_limits
                        .max_output_bytes
                        .checked_sub(consumed)
                        .filter(|remaining| *remaining > 0)
                    else {
                        return Ok(LeanProcessResult {
                            exit_code: None,
                            stdout,
                            stderr,
                            duration_milliseconds: elapsed
                                .as_millis()
                                .try_into()
                                .unwrap_or(u64::MAX),
                            timed_out: false,
                            output_limit_exceeded: true,
                            observed_toolchain_version: observed_toolchain_version
                                .trim()
                                .to_owned(),
                            memory_limit_enforced: false,
                            network_isolation_enforced: false,
                        });
                    };
                    let preparation = run_bounded_cache_process(
                        lake_command,
                        &arguments,
                        workspace,
                        preparation_timeout,
                        preparation_output,
                        environment,
                        &lean_num_threads,
                    )?;
                    append_process_output(&mut stdout, &preparation.stdout);
                    append_process_output(&mut stderr, &preparation.stderr);
                    if preparation.timed_out
                        || preparation.output_limit_exceeded
                        || preparation.exit_code != Some(0)
                    {
                        return Ok(LeanProcessResult {
                            exit_code: preparation.exit_code,
                            stdout,
                            stderr,
                            duration_milliseconds: started
                                .elapsed()
                                .as_millis()
                                .try_into()
                                .unwrap_or(u64::MAX),
                            timed_out: preparation.timed_out,
                            output_limit_exceeded: preparation.output_limit_exceeded,
                            observed_toolchain_version: observed_toolchain_version
                                .trim()
                                .to_owned(),
                            memory_limit_enforced: false,
                            network_isolation_enforced: false,
                        });
                    }
                }
            }
            let Some(build_timeout) = total_timeout.checked_sub(started.elapsed()) else {
                return Ok(LeanProcessResult {
                    exit_code: None,
                    stdout,
                    stderr,
                    duration_milliseconds: started
                        .elapsed()
                        .as_millis()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    timed_out: true,
                    output_limit_exceeded: false,
                    observed_toolchain_version: observed_toolchain_version.trim().to_owned(),
                    memory_limit_enforced: false,
                    network_isolation_enforced: false,
                });
            };
            let consumed = stdout
                .len()
                .saturating_add(stderr.len())
                .try_into()
                .unwrap_or(u64::MAX);
            let Some(build_output) = environment
                .resource_limits
                .max_output_bytes
                .checked_sub(consumed)
                .filter(|remaining| *remaining > 0)
            else {
                return Ok(LeanProcessResult {
                    exit_code: None,
                    stdout,
                    stderr,
                    duration_milliseconds: started
                        .elapsed()
                        .as_millis()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    timed_out: false,
                    output_limit_exceeded: true,
                    observed_toolchain_version: observed_toolchain_version.trim().to_owned(),
                    memory_limit_enforced: false,
                    network_isolation_enforced: false,
                });
            };
            let olean_target = lake_olean_target(build_target);
            let build = run_bounded_profiled_process(
                lake_command,
                &["build", olean_target.as_str()],
                workspace,
                build_timeout,
                build_output,
                environment,
                &lean_num_threads,
            )?;
            append_process_output(&mut stdout, &build.stdout);
            append_process_output(&mut stderr, &build.stderr);
            if build.timed_out || build.output_limit_exceeded || build.exit_code != Some(0) {
                return Ok(LeanProcessResult {
                    exit_code: build.exit_code,
                    stdout,
                    stderr,
                    duration_milliseconds: started
                        .elapsed()
                        .as_millis()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    timed_out: build.timed_out,
                    output_limit_exceeded: build.output_limit_exceeded,
                    observed_toolchain_version: observed_toolchain_version.trim().to_owned(),
                    memory_limit_enforced: build.memory_limit_enforced,
                    network_isolation_enforced: build.network_isolation_enforced,
                });
            }
            let elapsed = started.elapsed();
            let Some(remaining_timeout) = total_timeout.checked_sub(elapsed) else {
                return Ok(LeanProcessResult {
                    exit_code: None,
                    stdout,
                    stderr,
                    duration_milliseconds: elapsed.as_millis().try_into().unwrap_or(u64::MAX),
                    timed_out: true,
                    output_limit_exceeded: false,
                    observed_toolchain_version: observed_toolchain_version.trim().to_owned(),
                    memory_limit_enforced: build.memory_limit_enforced,
                    network_isolation_enforced: build.network_isolation_enforced,
                });
            };
            let consumed = stdout
                .len()
                .saturating_add(stderr.len())
                .try_into()
                .unwrap_or(u64::MAX);
            let Some(remaining_output) = environment
                .resource_limits
                .max_output_bytes
                .checked_sub(consumed)
                .filter(|remaining| *remaining > 0)
            else {
                return Ok(LeanProcessResult {
                    exit_code: None,
                    stdout,
                    stderr,
                    duration_milliseconds: elapsed.as_millis().try_into().unwrap_or(u64::MAX),
                    timed_out: false,
                    output_limit_exceeded: true,
                    observed_toolchain_version: observed_toolchain_version.trim().to_owned(),
                    memory_limit_enforced: build.memory_limit_enforced,
                    network_isolation_enforced: build.network_isolation_enforced,
                });
            };
            let driver = run_bounded_profiled_process(
                lake_command,
                &["env", "lean", module_file_name],
                workspace,
                remaining_timeout,
                remaining_output,
                environment,
                &lean_num_threads,
            )?;
            append_process_output(&mut stdout, &driver.stdout);
            append_process_output(&mut stderr, &driver.stderr);
            return Ok(LeanProcessResult {
                exit_code: driver.exit_code,
                stdout,
                stderr,
                duration_milliseconds: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                timed_out: driver.timed_out,
                output_limit_exceeded: driver.output_limit_exceeded,
                observed_toolchain_version: observed_toolchain_version.trim().to_owned(),
                memory_limit_enforced: driver.memory_limit_enforced,
                network_isolation_enforced: driver.network_isolation_enforced,
            });
        }
        _ => {
            return Err(AppError::new(
                "MCL_VERIFIER_COMMAND_REJECTED",
                "verifier command mode and project build target disagree",
                false,
                "Use standalone Lean without a project target, or Lake with one validated bound module.",
            ));
        }
    };
    let capture = run_bounded_profiled_process(
        command,
        &arguments,
        workspace,
        Duration::from_secs(environment.resource_limits.timeout_seconds),
        environment.resource_limits.max_output_bytes,
        environment,
        &lean_num_threads,
    )?;
    Ok(LeanProcessResult {
        exit_code: capture.exit_code,
        stdout: capture.stdout,
        stderr: capture.stderr,
        duration_milliseconds: capture.duration_milliseconds,
        timed_out: capture.timed_out,
        output_limit_exceeded: capture.output_limit_exceeded,
        observed_toolchain_version: observed_toolchain_version.trim().to_owned(),
        memory_limit_enforced: capture.memory_limit_enforced,
        network_isolation_enforced: capture.network_isolation_enforced,
    })
}

fn append_process_output(combined: &mut Vec<u8>, next: &[u8]) {
    if next.is_empty() {
        return;
    }
    if !combined.is_empty() && !combined.ends_with(b"\n") {
        combined.push(b'\n');
    }
    combined.extend_from_slice(next);
}

fn lean_num_threads_configuration(concurrency: u16) -> String {
    concurrency.to_string()
}

fn lake_olean_target(module_path: &str) -> String {
    format!("{module_path}:olean")
}

fn run_bounded_profiled_process(
    executable: &str,
    arguments: &[&str],
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: u64,
    environment: &EnvironmentManifest,
    lean_num_threads: &str,
) -> Result<ProcessCapture, AppError> {
    if environment.trust_profile == TrustProfile::Local {
        return run_bounded_lean_process(
            executable,
            arguments,
            workspace,
            timeout,
            max_output_bytes,
            Some(&environment.lean_toolchain),
            lean_num_threads,
        );
    }

    run_bounded_publication_process(
        executable,
        arguments,
        workspace,
        timeout,
        max_output_bytes,
        environment,
        lean_num_threads,
        true,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_bounded_publication_process(
    executable: &str,
    arguments: &[&str],
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: u64,
    environment: &EnvironmentManifest,
    lean_num_threads: &str,
    network_isolated: bool,
    mathlib_cache_directory: Option<&str>,
) -> Result<ProcessCapture, AppError> {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err(publication_isolation_error(
            "publication-profile verification requires a Linux x86-64 worker",
        ));
    }
    let isolated_executable = match executable {
        "lean" => "/opt/bin/lean",
        "lake" => "/opt/bin/lake",
        _ => {
            return Err(publication_isolation_error(format!(
                "publication verifier executable `{executable}` is not allowlisted"
            )));
        }
    };
    let memory_limit = environment
        .resource_limits
        .max_memory_bytes
        .ok_or_else(|| {
            publication_isolation_error(
                "publication-profile verification requires an exact memory limit",
            )
        })?;
    let workspace = std::fs::canonicalize(workspace).map_err(|error| {
        publication_isolation_error(format!(
            "publication workspace could not be resolved: {error}"
        ))
    })?;
    let toolchain_root = publication_toolchain_root(&environment.lean_toolchain)?;
    let uid = publication_numeric_identity("-u")?;
    let gid = publication_numeric_identity("-g")?;
    let sandbox_arguments = publication_sandbox_arguments(
        isolated_executable,
        arguments,
        &workspace,
        &toolchain_root,
        &uid,
        &gid,
        memory_limit,
        lean_num_threads,
        &environment.lean_toolchain,
        network_isolated,
        mathlib_cache_directory,
    );

    let mut capture = run_bounded_external(
        Path::new("/usr/bin/sudo"),
        &sandbox_arguments,
        &workspace,
        timeout,
        max_output_bytes,
        &[],
        "MCL_PUBLICATION_ISOLATION_UNAVAILABLE",
        "publication isolation launcher",
    )?;
    // Only a successful verifier command can prove that the fixed Bubblewrap and
    // prlimit chain reached the inner process. Failed candidates remain conservative.
    if capture.exit_code == Some(0) {
        capture.memory_limit_enforced = true;
        capture.network_isolation_enforced = network_isolated;
    }
    Ok(capture)
}

#[allow(clippy::too_many_arguments)]
fn publication_sandbox_arguments(
    isolated_executable: &str,
    arguments: &[&str],
    workspace: &Path,
    toolchain_root: &Path,
    uid: &str,
    gid: &str,
    memory_limit: u64,
    lean_num_threads: &str,
    lean_toolchain: &str,
    network_isolated: bool,
    mathlib_cache_directory: Option<&str>,
) -> Vec<OsString> {
    // Preserve the worker's host identity across sudo. Launching Bubblewrap as root
    // maps the requested sandbox uid to host root, which cannot access or update
    // runner-owned private mount sources after the user namespace is created.
    let mut sandbox_arguments = ["-n", "-u"]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    sandbox_arguments.push(OsString::from(format!("#{uid}")));
    sandbox_arguments.push(OsString::from("-g"));
    sandbox_arguments.push(OsString::from(format!("#{gid}")));
    sandbox_arguments.extend(
        [
            "--",
            "/usr/bin/bwrap",
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
            "--uid",
        ]
        .into_iter()
        .map(OsString::from),
    );
    sandbox_arguments.push(OsString::from(uid));
    sandbox_arguments.push(OsString::from("--gid"));
    sandbox_arguments.push(OsString::from(gid));
    if !network_isolated {
        sandbox_arguments.push(OsString::from("--share-net"));
    }
    sandbox_arguments.push(OsString::from("--clearenv"));
    for (name, value) in [
        ("HOME", "/tmp"),
        ("PATH", "/opt/bin:/usr/bin:/bin"),
        ("LANG", "C.UTF-8"),
        ("LC_ALL", "C.UTF-8"),
        ("LEAN_NUM_THREADS", lean_num_threads),
        ("ELAN_TOOLCHAIN", lean_toolchain),
    ] {
        sandbox_arguments.push(OsString::from("--setenv"));
        sandbox_arguments.push(OsString::from(name));
        sandbox_arguments.push(OsString::from(value));
    }
    if let Some(cache_directory) = mathlib_cache_directory {
        sandbox_arguments.push(OsString::from("--setenv"));
        sandbox_arguments.push(OsString::from("MATHLIB_CACHE_DIR"));
        sandbox_arguments.push(OsString::from(cache_directory));
    }
    sandbox_arguments.extend(["--ro-bind", "/", "/"].into_iter().map(OsString::from));
    sandbox_arguments.push(OsString::from("--bind"));
    sandbox_arguments.push(workspace.as_os_str().to_owned());
    sandbox_arguments.push(OsString::from("/mnt"));
    sandbox_arguments.push(OsString::from("--ro-bind"));
    sandbox_arguments.push(toolchain_root.as_os_str().to_owned());
    sandbox_arguments.push(OsString::from("/opt"));
    sandbox_arguments.extend(
        [
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/home",
            "--tmpfs",
            "/root",
            "--tmpfs",
            "/run",
            "--tmpfs",
            "/tmp",
            "--chdir",
            "/mnt",
            "/usr/bin/prlimit",
        ]
        .into_iter()
        .map(OsString::from),
    );
    sandbox_arguments.push(OsString::from(format!("--as={memory_limit}")));
    sandbox_arguments.push(OsString::from("--"));
    sandbox_arguments.push(OsString::from(isolated_executable));
    sandbox_arguments.extend(arguments.iter().map(OsString::from));
    sandbox_arguments
}

fn publication_toolchain_root(lean_toolchain: &str) -> Result<PathBuf, AppError> {
    let configured = std::env::var_os("ELAN_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".elan")))
        .ok_or_else(|| {
            publication_isolation_error(
                "publication worker does not expose ELAN_HOME or HOME for the pinned toolchain",
            )
        })?;
    let canonical = std::fs::canonicalize(&configured).map_err(|error| {
        publication_isolation_error(format!(
            "publication ELAN_HOME `{}` could not be resolved: {error}",
            configured.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(publication_isolation_error(format!(
            "publication ELAN_HOME `{}` is not a directory",
            canonical.display()
        )));
    }
    let elan = canonical.join("bin").join("elan");
    if !elan.is_file() {
        return Err(publication_isolation_error(format!(
            "publication Elan executable `{}` is unavailable",
            elan.display()
        )));
    }
    let output = Command::new(&elan)
        .args(["which", "lean"])
        .stdin(Stdio::null())
        .env_clear()
        .env("ELAN_HOME", &canonical)
        .env("ELAN_TOOLCHAIN", lean_toolchain)
        .output()
        .map_err(|error| {
            publication_isolation_error(format!(
                "publication toolchain root could not be resolved: {error}"
            ))
        })?;
    let lean_path = std::str::from_utf8(&output.stdout)
        .unwrap_or_default()
        .trim();
    if !output.status.success()
        || !output.stderr.is_empty()
        || lean_path.is_empty()
        || lean_path.len() > 4_096
    {
        return Err(publication_isolation_error(
            "publication Elan query returned an invalid toolchain path",
        ));
    }
    let lean = std::fs::canonicalize(lean_path).map_err(|error| {
        publication_isolation_error(format!(
            "publication Lean executable `{lean_path}` could not be resolved: {error}"
        ))
    })?;
    let root = lean
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| publication_isolation_error("publication Lean path has no toolchain root"))?
        .to_path_buf();
    let toolchains = std::fs::canonicalize(canonical.join("toolchains")).map_err(|error| {
        publication_isolation_error(format!(
            "publication Elan toolchain directory could not be resolved: {error}"
        ))
    })?;
    if root.parent() != Some(toolchains.as_path())
        || !root.join("bin").join("lean").is_file()
        || !root.join("bin").join("lake").is_file()
    {
        return Err(publication_isolation_error(
            "publication Lean and Lake executables are not one installed Elan toolchain",
        ));
    }
    Ok(root)
}

fn publication_numeric_identity(argument: &str) -> Result<String, AppError> {
    let output = Command::new("/usr/bin/id")
        .arg(argument)
        .stdin(Stdio::null())
        .env_clear()
        .output()
        .map_err(|error| {
            publication_isolation_error(format!(
                "publication worker identity could not be resolved: {error}"
            ))
        })?;
    let value = std::str::from_utf8(&output.stdout)
        .unwrap_or_default()
        .trim();
    if !output.status.success()
        || !output.stderr.is_empty()
        || value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(publication_isolation_error(
            "publication worker identity command returned an invalid numeric identity",
        ));
    }
    Ok(value.to_owned())
}

fn publication_isolation_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_PUBLICATION_ISOLATION_UNAVAILABLE",
        message,
        false,
        "Run the exact publication profile on protected Linux CI with sudo, Bubblewrap, prlimit, and the pinned Elan toolchain.",
    )
}

pub(crate) struct ProcessCapture {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration_milliseconds: u64,
    pub timed_out: bool,
    pub output_limit_exceeded: bool,
    pub memory_limit_enforced: bool,
    pub network_isolation_enforced: bool,
}

fn run_bounded_lean_process(
    executable: &str,
    arguments: &[&str],
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: u64,
    elan_toolchain: Option<&str>,
    lean_num_threads: &str,
) -> Result<ProcessCapture, AppError> {
    let arguments = arguments.iter().map(OsString::from).collect::<Vec<_>>();
    let mut extra_environment = vec![("LEAN_NUM_THREADS", lean_num_threads)];
    if let Some(toolchain) = elan_toolchain {
        extra_environment.push(("ELAN_TOOLCHAIN", toolchain));
    }
    run_bounded_external(
        Path::new(executable),
        &arguments,
        workspace,
        timeout,
        max_output_bytes,
        &extra_environment,
        "MCL_VERIFIER_LAUNCH_FAILED",
        "allowlisted Lean executable",
    )
}

fn run_bounded_cache_process(
    executable: &str,
    arguments: &[&str],
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: u64,
    environment: &EnvironmentManifest,
    lean_num_threads: &str,
) -> Result<ProcessCapture, AppError> {
    if environment.trust_profile == TrustProfile::Publication {
        return run_bounded_publication_process(
            executable,
            arguments,
            workspace,
            timeout,
            max_output_bytes,
            environment,
            lean_num_threads,
            false,
            Some(MATHLIB_CACHE_DIRECTORY),
        );
    }
    let arguments = arguments.iter().map(OsString::from).collect::<Vec<_>>();
    run_bounded_external(
        Path::new(executable),
        &arguments,
        workspace,
        timeout,
        max_output_bytes,
        &[
            ("ELAN_TOOLCHAIN", environment.lean_toolchain.as_str()),
            ("LEAN_NUM_THREADS", lean_num_threads),
            ("MATHLIB_CACHE_DIR", MATHLIB_CACHE_DIRECTORY),
        ],
        "MCL_VERIFIER_LAUNCH_FAILED",
        "allowlisted Lean executable",
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_bounded_external(
    executable: &Path,
    arguments: &[OsString],
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: u64,
    extra_environment: &[(&str, &str)],
    launch_error_code: &'static str,
    executable_label: &'static str,
) -> Result<ProcessCapture, AppError> {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    for name in ["PATH", "HOME", "USERPROFILE", "ELAN_HOME"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    for (name, value) in extra_environment {
        command.env(name, value);
    }
    let mut child = command.spawn().map_err(|error| {
        AppError::new(
            launch_error_code,
            format!("could not launch {executable_label}: {error}"),
            true,
            format!("Install the exact pinned {executable_label} and ensure it is executable."),
        )
    })?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let retained = Arc::new(AtomicU64::new(0));
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout_thread =
        capture_stream(stdout, retained.clone(), exceeded.clone(), max_output_bytes);
    let stderr_thread = capture_stream(stderr, retained, exceeded.clone(), max_output_bytes);
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| AppError::io("poll bounded process", error))?
        {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| AppError::io("reap timed-out bounded process", error))?;
        }
        if exceeded.load(Ordering::Relaxed) {
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| AppError::io("reap output-limited bounded process", error))?;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout_thread.join().map_err(|_| {
        AppError::new(
            "MCL_VERIFIER_CAPTURE_FAILED",
            "bounded stdout capture thread panicked",
            true,
            "Retry the job and inspect worker health if it repeats.",
        )
    })??;
    let stderr = stderr_thread.join().map_err(|_| {
        AppError::new(
            "MCL_VERIFIER_CAPTURE_FAILED",
            "bounded stderr capture thread panicked",
            true,
            "Retry the job and inspect worker health if it repeats.",
        )
    })??;
    Ok(ProcessCapture {
        exit_code: status.code(),
        stdout,
        stderr,
        duration_milliseconds: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        timed_out,
        output_limit_exceeded: exceeded.load(Ordering::Relaxed),
        memory_limit_enforced: false,
        network_isolation_enforced: false,
    })
}

fn capture_stream<R: Read + Send + 'static>(
    mut reader: R,
    retained: Arc<AtomicU64>,
    exceeded: Arc<AtomicBool>,
    max_output_bytes: u64,
) -> thread::JoinHandle<Result<Vec<u8>, AppError>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0_u8; 8_192];
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| AppError::io("capture bounded process output", error))?;
            if read == 0 {
                break;
            }
            let previous = retained.fetch_add(read as u64, Ordering::Relaxed);
            if previous >= max_output_bytes {
                exceeded.store(true, Ordering::Relaxed);
                continue;
            }
            let remaining = (max_output_bytes - previous) as usize;
            let keep = read.min(remaining);
            output.extend_from_slice(&buffer[..keep]);
            if keep < read {
                exceeded.store(true, Ordering::Relaxed);
            }
        }
        Ok(output)
    })
}

fn validate_platform(platform: EnvironmentPlatform) -> Result<(), AppError> {
    let matches = matches!(
        platform,
        EnvironmentPlatform::LinuxX86_64 if cfg!(all(target_os = "linux", target_arch = "x86_64"))
    ) || matches!(
        platform,
        EnvironmentPlatform::WindowsX86_64 if cfg!(all(target_os = "windows", target_arch = "x86_64"))
    );
    if matches {
        Ok(())
    } else {
        Err(AppError::new(
            "MCL_VERIFIER_PLATFORM_MISMATCH",
            "registered verifier platform does not match this worker",
            false,
            "Select an environment registered for this exact worker platform.",
        ))
    }
}

fn lean_identifiers_without_comments_or_strings(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut identifiers = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    let mut block_depth = 0_u32;
    let mut line_comment = false;
    let mut string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
        } else if block_depth > 0 {
            if byte == b'/' && next == Some(b'-') {
                block_depth += 1;
                index += 1;
            } else if byte == b'-' && next == Some(b'/') {
                block_depth -= 1;
                index += 1;
            }
        } else if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
        } else if byte == b'-' && next == Some(b'-') {
            flush_identifier(&mut current, &mut identifiers);
            line_comment = true;
            index += 1;
        } else if byte == b'/' && next == Some(b'-') {
            flush_identifier(&mut current, &mut identifiers);
            block_depth = 1;
            index += 1;
        } else if byte == b'"' {
            flush_identifier(&mut current, &mut identifiers);
            string = true;
        } else if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\'') {
            current.push(byte as char);
        } else {
            flush_identifier(&mut current, &mut identifiers);
        }
        index += 1;
    }
    flush_identifier(&mut current, &mut identifiers);
    identifiers
}

fn flush_identifier(current: &mut String, identifiers: &mut Vec<String>) {
    if !current.is_empty() {
        identifiers.push(std::mem::take(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lake_project_build_uses_closed_concurrency_and_olean_target() {
        assert_eq!(lean_num_threads_configuration(1), "1");
        assert_eq!(lean_num_threads_configuration(16), "16");
        assert_eq!(lake_olean_target("Final.lean"), "Final.lean:olean");
    }

    #[test]
    fn publication_sandbox_separates_contained_fetch_from_networkless_proof() {
        let arguments = publication_sandbox_arguments(
            "/opt/bin/lake",
            &["build", "Final.lean:olean"],
            Path::new("workspace"),
            Path::new("elan"),
            "1001",
            "1001",
            12_884_901_888,
            "1",
            "leanprover/lean4:v4.32.0-rc1",
            true,
            None,
        )
        .into_iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
        assert_eq!(
            &arguments[..9],
            [
                "-n",
                "-u",
                "#1001",
                "-g",
                "#1001",
                "--",
                "/usr/bin/bwrap",
                "--unshare-all",
                "--die-with-parent",
            ]
        );
        let clear = arguments
            .iter()
            .position(|value| value == "--clearenv")
            .expect("clear environment");
        let first_set = arguments
            .iter()
            .position(|value| value == "--setenv")
            .expect("typed environment");
        assert!(clear < first_set);
        assert!(
            arguments
                .windows(2)
                .any(|values| values == ["--uid", "1001"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|values| values == ["--gid", "1001"])
        );
        assert!(
            arguments
                .windows(3)
                .any(|values| values == ["--setenv", "LEAN_NUM_THREADS", "1"])
        );
        assert!(
            arguments
                .windows(3)
                .any(|values| values
                    == ["--setenv", "ELAN_TOOLCHAIN", "leanprover/lean4:v4.32.0-rc1"])
        );
        assert!(
            arguments
                .windows(3)
                .any(|values| values == ["--bind", "workspace", "/mnt"])
        );
        assert!(
            arguments
                .windows(3)
                .any(|values| values == ["--ro-bind", "elan", "/opt"])
        );
        assert!(!arguments.iter().any(|value| value == "--share-net"));
        for masked in ["/home", "/root", "/run", "/tmp"] {
            assert!(
                arguments
                    .windows(2)
                    .any(|values| values == ["--tmpfs", masked])
            );
        }
        assert_eq!(
            &arguments[arguments.len() - 6..],
            [
                "/usr/bin/prlimit",
                "--as=12884901888",
                "--",
                "/opt/bin/lake",
                "build",
                "Final.lean:olean",
            ]
        );

        let preparation = publication_sandbox_arguments(
            "/opt/bin/lake",
            &["exe", "cache", "get-"],
            Path::new("workspace"),
            Path::new("elan"),
            "1001",
            "1001",
            12_884_901_888,
            "1",
            "leanprover/lean4:v4.32.0-rc1",
            false,
            Some(".mathlib-cache"),
        )
        .into_iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
        assert!(preparation.iter().any(|value| value == "--share-net"));
        let unshare = preparation
            .iter()
            .position(|value| value == "--unshare-all")
            .expect("namespace isolation");
        let share = preparation
            .iter()
            .position(|value| value == "--share-net")
            .expect("controlled fetch network");
        let clear = preparation
            .iter()
            .position(|value| value == "--clearenv")
            .expect("clear environment");
        assert!(unshare < share && share < clear);
        assert!(
            preparation
                .windows(3)
                .any(|values| values == ["--setenv", "MATHLIB_CACHE_DIR", ".mathlib-cache"])
        );
    }

    #[test]
    fn unsafe_tokens_are_detected_but_comments_and_strings_do_not_trigger() {
        for (source, expected) in [
            ("theorem x : True := by sorry\n", "sorry"),
            ("theorem x : True := by admit\n", "admit"),
            ("axiom fabricated : False\n", "axiom"),
            ("unsafe def escape := true\n", "unsafe"),
            ("extern \"escape\" opaque escape : True\n", "extern"),
            ("theorem x : True := by native_decide\n", "native_decide"),
            ("run_cmd IO.println \"side effect\"\n", "run_cmd"),
        ] {
            assert_eq!(
                scan_forbidden_source_token(source.as_bytes()).expect("scan"),
                Some(expected.to_owned())
            );
        }
        assert_eq!(
            scan_forbidden_source_token(
                b"-- sorry\n/- unsafe /- admit -/ -/\ndef text := \"native_decide\"\ntheorem x : True := by trivial\n"
            )
            .expect("scan"),
            None
        );
    }

    #[test]
    fn axiom_output_is_declaration_specific_bounded_and_canonical() {
        assert_eq!(
            parse_axiom_dependencies(
                "MathOS.truth",
                b"'MathOS.truth' does not depend on any axioms\n",
                b"",
            )
            .expect("axiom-free output"),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_axiom_dependencies(
                "MathOS.classical",
                b"",
                b"'MathOS.classical' depends on axioms: [propext, Classical.choice, Quot.sound]\n",
            )
            .expect("standard axioms parse"),
            ["Classical.choice", "Quot.sound", "propext"]
        );
        assert_eq!(
            parse_axiom_dependencies(
                "MathOS.incomplete",
                b"'MathOS.incomplete' depends on axioms: [sorryAx, MathOS.customAxiom]\n",
                b"",
            )
            .expect("hidden and custom axioms remain visible"),
            ["MathOS.customAxiom", "sorryAx"]
        );
        assert_eq!(
            parse_axiom_dependencies(
                "MathOS.ambiguous",
                b"'MathOS.ambiguous' does not depend on any axioms\n'MathOS.ambiguous' does not depend on any axioms\n",
                b"",
            )
            .expect("repeated identical no-axiom output"),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_axiom_dependencies(
                "MathOS.repeated",
                b"'MathOS.repeated' depends on axioms: [propext, Classical.choice]\n",
                b"'MathOS.repeated' depends on axioms: [propext, Classical.choice]\n",
            )
            .expect("repeated identical axiom output"),
            ["Classical.choice", "propext"]
        );
        assert_eq!(
            parse_axiom_dependencies(
                "MathOS.conflicting",
                b"'MathOS.conflicting' depends on axioms: [propext]\n",
                b"'MathOS.conflicting' depends on axioms: [Classical.choice]\n",
            )
            .expect_err("conflicting repeated output rejected")
            .code,
            "MCL_AUDIT_OUTPUT_INVALID"
        );
        assert_eq!(
            parse_axiom_dependencies(
                "MathOS.duplicate",
                b"'MathOS.duplicate' depends on axioms: [propext, propext]\n",
                b"",
            )
            .expect_err("duplicate axiom rejected")
            .code,
            "MCL_AUDIT_OUTPUT_INVALID"
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_capture_enforces_output_and_time_bounds() {
        let workspace = tempfile::TempDir::new().expect("workspace");
        let selected = run_bounded_lean_process(
            "sh",
            &[
                "-c",
                "test \"$ELAN_TOOLCHAIN\" = leanprover/lean4:v4.32.0 && test \"$LEAN_NUM_THREADS\" = 1",
            ],
            workspace.path(),
            Duration::from_secs(1),
            64,
            Some("leanprover/lean4:v4.32.0"),
            "1",
        )
        .expect("typed toolchain environment");
        assert_eq!(selected.exit_code, Some(0));

        let leaked_name = std::env::vars()
            .map(|(name, _)| name)
            .find(|name| {
                !matches!(
                    name.as_str(),
                    "PATH"
                        | "HOME"
                        | "USERPROFILE"
                        | "ELAN_HOME"
                        | "SystemRoot"
                        | "PWD"
                        | "OLDPWD"
                        | "SHLVL"
                        | "_"
                ) && name
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
            })
            .expect("test process has one non-allowlisted environment variable");
        let leak_check = format!("test -z \"${{{leaked_name}+present}}\"");
        let cleared = run_bounded_lean_process(
            "sh",
            &["-c", &leak_check],
            workspace.path(),
            Duration::from_secs(1),
            64,
            None,
            "1",
        )
        .expect("cleared child environment");
        assert_eq!(cleared.exit_code, Some(0), "leaked {leaked_name}");

        let output = run_bounded_lean_process(
            "sh",
            &["-c", "printf 123456789"],
            workspace.path(),
            Duration::from_secs(1),
            4,
            None,
            "1",
        )
        .expect("bounded output process");
        assert!(output.output_limit_exceeded);
        assert!(output.stdout.len() + output.stderr.len() <= 4);

        let timeout = run_bounded_lean_process(
            "sh",
            &["-c", "while :; do :; done"],
            workspace.path(),
            Duration::from_millis(30),
            64,
            None,
            "1",
        )
        .expect("bounded timeout process");
        assert!(timeout.timed_out);
    }
}
