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
    let mut environment_manifest: Value =
        serde_json::from_str(include_str!("../fixtures/environment/lean-4.32-local.json"))
            .expect("environment fixture JSON");
    if cfg!(windows) {
        environment_manifest["platform"] = json!("windows_x86_64");
    }
    let environment = run(
        root.path(),
        &[
            "environment".to_owned(),
            "register".to_owned(),
            "--manifest-json".to_owned(),
            environment_manifest.to_string(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-environment".to_owned(),
        ],
    );
    let environment_hash = environment["proposed_environment_hash"]
        .as_str()
        .expect("environment hash")
        .to_owned();
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
    let accepted_job = run(
        root.path(),
        &[
            "verify".to_owned(),
            "check".to_owned(),
            "--environment-hash".to_owned(),
            environment_hash.clone(),
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
    let accepted_job_id = accepted_job["job"]["job_id"]
        .as_str()
        .expect("accepted job ID");
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
    assert_eq!(
        worked["report"]["classification"], "elaborated",
        "unexpected worker outcome: {worked:#}"
    );
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

    let source = run(
        root.path(),
        &[
            "source".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            json!({
                "source_type": "user_statement",
                "title_or_label": "Lean audit fixture",
                "authors_or_origin": ["CI"],
                "canonical_locator": "local:lean-audit-fixture",
                "acquisition_date": "2026-07-19",
                "license_expression": null,
                "redistribution_status": "unknown",
                "content_hash": null,
                "citation_metadata": {},
                "redaction_class": "private",
                "provenance_notes": "real Lean audit integration",
                "original_text": "True is inhabited."
            })
            .to_string(),
            "--searchable-text".to_owned(),
            "Lean audit fixture".to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-source".to_owned(),
        ],
    );
    let claim = run(
        root.path(),
        &[
            "claim".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            json!({
                "source_reference": {
                    "object_id": source["record"]["object_id"],
                    "version_hash": source["record"]["version_hash"]
                },
                "normalized_informal_statement": "True is inhabited.",
                "claim_kind": "existential",
                "logical_shape": "True",
                "assumptions": [],
                "variables": [],
                "concept_links": [],
                "source_citations": [],
                "ambiguity_notes": []
            })
            .to_string(),
            "--searchable-text".to_owned(),
            "True is inhabited".to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-claim".to_owned(),
        ],
    );
    let formalization = run(
        root.path(),
        &[
            "formalization".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            json!({
                "claim_version": {
                    "object_id": claim["record"]["object_id"],
                    "version_hash": claim["record"]["version_hash"]
                },
                "formal_system": "lean4",
                "environment_hash": environment_hash.clone(),
                "module_artifact_hash": artifact_hash,
                "declaration_name": "MathOS.LeanWorker.truth",
                "exact_theorem_type": "True",
                "declaration_hash": "1".repeat(64),
                "import_manifest": [],
                "formalization_notes": "real audit fixture",
                "fidelity_evidence_references": [],
                "verification_evidence_references": []
            })
            .to_string(),
            "--searchable-text".to_owned(),
            "MathOS LeanWorker truth".to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-formalization".to_owned(),
        ],
    );
    let diagnostic = run(
        root.path(),
        &[
            "verify".to_owned(),
            "promote-diagnostic".to_owned(),
            "--formalization-object-id".to_owned(),
            formalization["record"]["object_id"]
                .as_str()
                .expect("formalization ID")
                .to_owned(),
            "--formalization-version-hash".to_owned(),
            formalization["record"]["version_hash"]
                .as_str()
                .expect("formalization hash")
                .to_owned(),
            "--job-id".to_owned(),
            accepted_job_id.to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-diagnostic".to_owned(),
        ],
    );
    assert_eq!(diagnostic["evidence"]["payload"]["result"], "accepted");
    assert_eq!(
        diagnostic["evidence"]["payload"]["authority_class"],
        "diagnostic"
    );
    let audit = run(
        root.path(),
        &[
            "verify".to_owned(),
            "audit".to_owned(),
            "--formalization-object-id".to_owned(),
            formalization["record"]["object_id"]
                .as_str()
                .expect("formalization ID")
                .to_owned(),
            "--formalization-version-hash".to_owned(),
            formalization["record"]["version_hash"]
                .as_str()
                .expect("formalization hash")
                .to_owned(),
            "--diagnostic-evidence-id".to_owned(),
            diagnostic["evidence"]["evidence_id"]
                .as_str()
                .expect("diagnostic evidence ID")
                .to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-audit".to_owned(),
        ],
    );
    assert_eq!(audit["job"]["state"], "queued");
    let audited = run(
        root.path(),
        &[
            "worker".to_owned(),
            "--job-kind".to_owned(),
            "audit".to_owned(),
            "--worker-id".to_owned(),
            "lean-ci-audit-worker".to_owned(),
            "--lease-seconds".to_owned(),
            "3660".to_owned(),
        ],
    );
    assert_eq!(audited["report"]["classification"], "passed");
    assert_eq!(audited["report"]["observed_axioms"], json!([]));
    assert_eq!(audited["report"]["unexpected_axioms"], json!([]));
    assert_eq!(audited["report"]["dependency_closure_complete"], true);
    assert_eq!(audited["report"]["authoritative"], false);
    assert_eq!(audited["job"]["state"], "succeeded");
    let audit_evidence = run(
        root.path(),
        &[
            "verify".to_owned(),
            "promote-audit".to_owned(),
            "--formalization-object-id".to_owned(),
            formalization["record"]["object_id"]
                .as_str()
                .expect("formalization ID")
                .to_owned(),
            "--formalization-version-hash".to_owned(),
            formalization["record"]["version_hash"]
                .as_str()
                .expect("formalization hash")
                .to_owned(),
            "--job-id".to_owned(),
            audited["job"]["job_id"]
                .as_str()
                .expect("audit job ID")
                .to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-audit-evidence".to_owned(),
        ],
    );
    let audit_records = audit_evidence["evidence"]
        .as_array()
        .expect("audit evidence pair");
    assert_eq!(audit_records.len(), 2);
    assert!(audit_records.iter().all(|evidence| {
        evidence["payload"]["authority_class"] == "diagnostic"
            && evidence["payload"]["result"] == "accepted"
    }));
    let mut evidence_kinds = audit_records
        .iter()
        .map(|evidence| {
            evidence["payload"]["evidence_kind"]
                .as_str()
                .expect("audit evidence kind")
        })
        .collect::<Vec<_>>();
    evidence_kinds.sort();
    assert_eq!(evidence_kinds, ["axiom_audit", "proof_closure_scan"]);

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
            environment_hash.clone(),
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
                environment_hash,
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
