use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::domain::{EnvironmentManifest, EnvironmentPlatform, TrustProfile};
use crate::error::AppError;

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
    if no_axiom_count == 1 && list_count == 0 {
        return Ok(Vec::new());
    }
    if no_axiom_count != 0 || list_count != 1 {
        return Err(audit_output_error(
            "audit output did not contain exactly one declaration-specific axiom result",
        ));
    }
    let start = output.find(&list_prefix).expect("single marker exists") + list_prefix.len();
    let tail = &output[start..];
    let end = tail
        .find(']')
        .ok_or_else(|| audit_output_error("audit axiom list did not contain a closing bracket"))?;
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
    Ok(axioms)
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
    command: &str,
    workspace: &Path,
    module_file_name: &str,
    environment: &EnvironmentManifest,
) -> Result<LeanProcessResult, AppError> {
    environment.validate()?;
    if environment.trust_profile == TrustProfile::Publication {
        return Err(AppError::new(
            "MCL_PUBLICATION_ISOLATION_UNAVAILABLE",
            "publication-profile process isolation is not implemented by the local worker",
            false,
            "Use the protected publication CI profile once its network and process isolation controls are implemented.",
        ));
    }
    validate_platform(environment.platform)?;
    if !matches!(command, "lean" | "lean.exe") {
        return Err(AppError::new(
            "MCL_VERIFIER_COMMAND_REJECTED",
            format!("verifier executable `{command}` is not allowlisted"),
            false,
            "Configure only the platform Lean executable name.",
        ));
    }
    let version_capture = run_bounded_process(
        command,
        &["--version"],
        workspace,
        Duration::from_secs(environment.resource_limits.timeout_seconds.min(30)),
        4_096,
        Some(&environment.lean_toolchain),
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

    let capture = run_bounded_process(
        command,
        &[module_file_name],
        workspace,
        Duration::from_secs(environment.resource_limits.timeout_seconds),
        environment.resource_limits.max_output_bytes,
        Some(&environment.lean_toolchain),
    )?;
    Ok(LeanProcessResult {
        exit_code: capture.exit_code,
        stdout: capture.stdout,
        stderr: capture.stderr,
        duration_milliseconds: capture.duration_milliseconds,
        timed_out: capture.timed_out,
        output_limit_exceeded: capture.output_limit_exceeded,
        observed_toolchain_version: observed_toolchain_version.trim().to_owned(),
    })
}

pub(crate) struct ProcessCapture {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration_milliseconds: u64,
    pub timed_out: bool,
    pub output_limit_exceeded: bool,
}

fn run_bounded_process(
    executable: &str,
    arguments: &[&str],
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: u64,
    elan_toolchain: Option<&str>,
) -> Result<ProcessCapture, AppError> {
    let arguments = arguments.iter().map(OsString::from).collect::<Vec<_>>();
    let extra_environment = elan_toolchain
        .map(|toolchain| vec![("ELAN_TOOLCHAIN", toolchain)])
        .unwrap_or_default();
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
            .expect_err("duplicate declaration output rejected")
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
        let selected = run_bounded_process(
            "sh",
            &["-c", "test \"$ELAN_TOOLCHAIN\" = leanprover/lean4:v4.32.0"],
            workspace.path(),
            Duration::from_secs(1),
            64,
            Some("leanprover/lean4:v4.32.0"),
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
        let cleared = run_bounded_process(
            "sh",
            &["-c", &leak_check],
            workspace.path(),
            Duration::from_secs(1),
            64,
            None,
        )
        .expect("cleared child environment");
        assert_eq!(cleared.exit_code, Some(0), "leaked {leaked_name}");

        let output = run_bounded_process(
            "sh",
            &["-c", "printf 123456789"],
            workspace.path(),
            Duration::from_secs(1),
            4,
            None,
        )
        .expect("bounded output process");
        assert!(output.output_limit_exceeded);
        assert!(output.stdout.len() + output.stderr.len() <= 4);

        let timeout = run_bounded_process(
            "sh",
            &["-c", "while :; do :; done"],
            workspace.path(),
            Duration::from_millis(30),
            64,
            None,
        )
        .expect("bounded timeout process");
        assert!(timeout.timed_out);
    }
}
