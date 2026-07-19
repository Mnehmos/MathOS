use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::domain::{EnvironmentManifest, EnvironmentPlatform, TrustProfile};
use crate::error::AppError;

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
    const FORBIDDEN: &[&str] = &[
        "sorry",
        "admit",
        "sorryAx",
        "axiom",
        "constant",
        "unsafe",
        "extern",
        "implemented_by",
        "native_decide",
        "run_cmd",
        "run_tac",
        "elab",
        "macro",
        "syntax",
        "initialize",
        "builtin_initialize",
        "include_str",
        "include_bytes",
        "eval",
        "reduce",
    ];
    Ok(identifiers
        .into_iter()
        .find(|identifier| FORBIDDEN.contains(&identifier.as_str())))
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
    if version_capture.timed_out
        || version_capture.output_limit_exceeded
        || version_capture.exit_code != Some(0)
        || !observed_toolchain_version.contains(&format!("version {expected},"))
    {
        return Err(AppError::new(
            "MCL_VERIFIER_VERSION_MISMATCH",
            format!(
                "observed Lean version does not match pinned release {expected}: {}",
                observed_toolchain_version.trim()
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

struct ProcessCapture {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    duration_milliseconds: u64,
    timed_out: bool,
    output_limit_exceeded: bool,
}

fn run_bounded_process(
    executable: &str,
    arguments: &[&str],
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: u64,
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
    let mut child = command.spawn().map_err(|error| {
        AppError::new(
            "MCL_VERIFIER_LAUNCH_FAILED",
            format!("could not launch allowlisted Lean executable: {error}"),
            true,
            "Install the exact pinned Lean toolchain and ensure it is available on PATH.",
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
            .map_err(|error| AppError::io("poll Lean process", error))?
        {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| AppError::io("reap timed-out Lean process", error))?;
        }
        if exceeded.load(Ordering::Relaxed) {
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| AppError::io("reap output-limited Lean process", error))?;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout_thread.join().map_err(|_| {
        AppError::new(
            "MCL_VERIFIER_CAPTURE_FAILED",
            "Lean stdout capture thread panicked",
            true,
            "Retry the job and inspect worker health if it repeats.",
        )
    })??;
    let stderr = stderr_thread.join().map_err(|_| {
        AppError::new(
            "MCL_VERIFIER_CAPTURE_FAILED",
            "Lean stderr capture thread panicked",
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
                .map_err(|error| AppError::io("capture Lean process output", error))?;
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
        assert_eq!(
            scan_forbidden_source_token(b"theorem x : True := by sorry\n").expect("scan"),
            Some("sorry".to_owned())
        );
        assert_eq!(
            scan_forbidden_source_token(b"run_cmd IO.println \"side effect\"\n").expect("scan"),
            Some("run_cmd".to_owned())
        );
        assert_eq!(
            scan_forbidden_source_token(b"axiom fabricated : False\n").expect("scan"),
            Some("axiom".to_owned())
        );
        assert_eq!(
            scan_forbidden_source_token(
                b"-- sorry\n/- unsafe /- admit -/ -/\ndef text := \"native_decide\"\ntheorem x : True := by trivial\n"
            )
            .expect("scan"),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_capture_enforces_output_and_time_bounds() {
        let workspace = tempfile::TempDir::new().expect("workspace");
        let output = run_bounded_process(
            "sh",
            &["-c", "printf 123456789"],
            workspace.path(),
            Duration::from_secs(1),
            4,
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
        )
        .expect("bounded timeout process");
        assert!(timeout.timed_out);
    }
}
