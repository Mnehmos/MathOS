use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

fn mcl(root: &Path, arguments: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mcl"))
        .arg("--root")
        .arg(root)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("mcl process runs")
}

fn run(root: &Path, arguments: &[String]) -> Value {
    let output = mcl(root, arguments);
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout JSON")
}

#[test]
fn pinned_lean_worker_elaborates_real_source_without_granting_authority() {
    if std::env::var("MCL_RUN_LEAN_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let root = TempDir::new().expect("temporary root");
    run(
        root.path(),
        &[
            "init".to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-init".to_owned(),
        ],
    );
    run(
        root.path(),
        &[
            "environment".to_owned(),
            "register".to_owned(),
            "--manifest-json".to_owned(),
            include_str!("../fixtures/environment/lean-4.32-local.json").to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-environment".to_owned(),
        ],
    );
    let module = root.path().join("LeanWorkerFixture.lean");
    fs::write(
        &module,
        b"namespace MathOS.LeanWorker\ntheorem truth : True := by trivial\nend MathOS.LeanWorker\n",
    )
    .expect("Lean source writes");
    let artifact = run(
        root.path(),
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            module.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            json!({
                "schema_version": "artifact_metadata/1",
                "media_type": "text/x-lean",
                "creation_source": "user_ingest",
                "license_expression": "PolyForm-Noncommercial-1.0.0",
                "restriction": "restricted",
                "semantic_metadata": {"declaration_name": "MathOS.LeanWorker.truth"}
            })
            .to_string(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-artifact".to_owned(),
        ],
    );
    let artifact_hash = artifact["proposed_artifact_hash"]
        .as_str()
        .expect("artifact hash");
    run(
        root.path(),
        &[
            "verify".to_owned(),
            "check".to_owned(),
            "--environment-hash".to_owned(),
            include_str!("../fixtures/environment/lean-4.32-local.sha256")
                .trim()
                .to_owned(),
            "--module-artifact-hash".to_owned(),
            artifact_hash.to_owned(),
            "--declaration-name".to_owned(),
            "MathOS.LeanWorker.truth".to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-job".to_owned(),
        ],
    );
    let worked = run(
        root.path(),
        &[
            "worker".to_owned(),
            "--worker-id".to_owned(),
            "lean-ci-worker".to_owned(),
            "--lease-seconds".to_owned(),
            "3660".to_owned(),
        ],
    );
    assert_eq!(worked["report"]["classification"], "elaborated");
    assert_eq!(worked["report"]["authoritative"], false);
    assert_eq!(worked["report"]["network_isolation_enforced"], false);
    assert_eq!(worked["report"]["memory_limit_enforced"], false);
    assert!(
        worked["report"]["observed_toolchain_version"]
            .as_str()
            .is_some_and(|version| version.contains("4.32.0"))
    );
    assert_eq!(worked["job"]["state"], "succeeded");
    assert!(worked["job"]["result_artifact_hash"].is_string());

    let rejected_module = root.path().join("LeanWorkerRejected.lean");
    fs::write(
        &rejected_module,
        b"namespace MathOS.LeanWorker\ntheorem rejected : False := by trivial\nend MathOS.LeanWorker\n",
    )
    .expect("rejected Lean source writes");
    let rejected_artifact = run(
        root.path(),
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            rejected_module.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            json!({
                "schema_version": "artifact_metadata/1",
                "media_type": "text/x-lean",
                "creation_source": "user_ingest",
                "license_expression": "PolyForm-Noncommercial-1.0.0",
                "restriction": "restricted",
                "semantic_metadata": {"declaration_name": "MathOS.LeanWorker.rejected"}
            })
            .to_string(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-rejected-artifact".to_owned(),
        ],
    );
    run(
        root.path(),
        &[
            "verify".to_owned(),
            "check".to_owned(),
            "--environment-hash".to_owned(),
            include_str!("../fixtures/environment/lean-4.32-local.sha256")
                .trim()
                .to_owned(),
            "--module-artifact-hash".to_owned(),
            rejected_artifact["proposed_artifact_hash"]
                .as_str()
                .expect("rejected artifact hash")
                .to_owned(),
            "--declaration-name".to_owned(),
            "MathOS.LeanWorker.rejected".to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-rejected-job".to_owned(),
        ],
    );
    let rejected = run(
        root.path(),
        &[
            "worker".to_owned(),
            "--worker-id".to_owned(),
            "lean-ci-worker".to_owned(),
            "--lease-seconds".to_owned(),
            "3660".to_owned(),
        ],
    );
    assert_eq!(rejected["report"]["classification"], "rejected");
    assert_eq!(rejected["report"]["authoritative"], false);
    assert!(
        rejected["report"]["exit_code"]
            .as_i64()
            .is_some_and(|code| code != 0)
    );
    assert!(
        rejected["report"]["stdout_artifact_hash"].is_string()
            || rejected["report"]["stderr_artifact_hash"].is_string()
    );
    assert_eq!(rejected["job"]["state"], "succeeded");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        run(
            root.path(),
            &[
                "verify".to_owned(),
                "check".to_owned(),
                "--environment-hash".to_owned(),
                include_str!("../fixtures/environment/lean-4.32-local.sha256")
                    .trim()
                    .to_owned(),
                "--module-artifact-hash".to_owned(),
                artifact_hash.to_owned(),
                "--declaration-name".to_owned(),
                "MathOS.LeanWorker.truth".to_owned(),
                "--actor".to_owned(),
                "lean-ci".to_owned(),
                "--idempotency-key".to_owned(),
                "lean-ci-mismatched-job".to_owned(),
            ],
        );
        let fake_bin = root.path().join("fake-bin");
        fs::create_dir(&fake_bin).expect("fake binary directory");
        let fake_lean = fake_bin.join("lean");
        fs::write(
            &fake_lean,
            b"#!/bin/sh\nprintf 'Lean (version 4.31.0, fake, Release)\\n'\n",
        )
        .expect("fake Lean writes");
        let mut permissions = fs::metadata(&fake_lean)
            .expect("fake Lean metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake_lean, permissions).expect("fake Lean is executable");
        let mismatch_output = Command::new(env!("CARGO_BIN_EXE_mcl"))
            .arg("--root")
            .arg(root.path())
            .arg("--json")
            .args([
                "worker",
                "--worker-id",
                "lean-ci-worker",
                "--lease-seconds",
                "3660",
            ])
            .env("PATH", &fake_bin)
            .output()
            .expect("mismatched worker runs");
        assert!(mismatch_output.status.success());
        let mismatched: Value =
            serde_json::from_slice(&mismatch_output.stdout).expect("mismatch JSON");
        assert_eq!(mismatched["report"]["classification"], "toolchain_mismatch");
        assert_eq!(mismatched["report"]["authoritative"], false);
        assert_eq!(mismatched["job"]["state"], "failed");
        assert_eq!(
            mismatched["job"]["last_error"]["code"],
            "MCL_VERIFIER_VERSION_MISMATCH"
        );
    }
}
