use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn mcl(root: &TempDir, arguments: &[&str]) -> Output {
    mcl_at(root.path(), arguments)
}

fn mcl_at(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mcl"))
        .arg("--root")
        .arg(root)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("mcl process runs")
}

fn mcl_owned(root: &TempDir, arguments: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mcl"))
        .arg("--root")
        .arg(root.path())
        .arg("--json")
        .args(arguments)
        .output()
        .expect("mcl process runs")
}

fn parse_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

fn normalize_hash(value: &mut Value, pointer: &str) {
    *value.pointer_mut(pointer).expect("hash field") = json!("<sha256>");
}

fn normalize_uuid(value: &mut Value, pointer: &str) {
    *value.pointer_mut(pointer).expect("UUID field") = json!("<uuidv7>");
}

fn golden(input: &str) -> Value {
    serde_json::from_str(input).expect("golden JSON")
}

fn create_source_record(root: &TempDir, label: &str, idempotency_key: &str) -> Value {
    let payload = serde_json::to_string(&json!({
        "source_type": "user_statement",
        "title_or_label": label,
        "authors_or_origin": ["CLI fixture"],
        "canonical_locator": format!("local:{label}"),
        "acquisition_date": "2026-07-19",
        "license_expression": null,
        "redistribution_status": "unknown",
        "content_hash": null,
        "citation_metadata": {},
        "redaction_class": "private",
        "provenance_notes": "CLI graph fixture",
        "original_text": label
    }))
    .expect("source JSON");
    let output = mcl_owned(
        root,
        &[
            "source".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            payload,
            "--searchable-text".to_owned(),
            label.to_owned(),
            "--actor".to_owned(),
            "cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            idempotency_key.to_owned(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse_stdout(&output)["record"].clone()
}

#[test]
fn init_creates_real_storage_and_health_passes() {
    let root = TempDir::new().expect("temporary root");
    let initialized = mcl(
        &root,
        &[
            "init",
            "--actor",
            "phase1-test",
            "--idempotency-key",
            "init-001",
        ],
    );
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let value = parse_stdout(&initialized);
    assert_eq!(value["migration_version"], 9);
    assert_eq!(value["journal_mode"], "wal");
    assert!(root.path().join("mcl.toml").is_file());
    assert!(root.path().join(".mcl/state.sqlite3").is_file());

    let health = mcl(&root, &["health"]);
    assert!(
        health.status.success(),
        "{}",
        String::from_utf8_lossy(&health.stdout)
    );
    assert_eq!(parse_stdout(&health)["healthy"], true);
}

#[test]
fn environment_cli_registers_dry_runs_retries_and_survives_restart() {
    let root = TempDir::new().expect("temporary root");
    assert!(
        mcl(
            &root,
            &[
                "init",
                "--actor",
                "environment-cli-test",
                "--idempotency-key",
                "environment-cli-init",
            ],
        )
        .status
        .success()
    );
    let manifest = include_str!("../fixtures/environment/lean-4.32-local.json").to_owned();
    let register = |dry_run: bool, idempotency_key: &str| {
        let mut arguments = vec![
            "environment".to_owned(),
            "register".to_owned(),
            "--manifest-json".to_owned(),
            manifest.clone(),
            "--actor".to_owned(),
            "environment-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            idempotency_key.to_owned(),
        ];
        if dry_run {
            arguments.push("--dry-run".to_owned());
        }
        mcl_owned(&root, &arguments)
    };

    let preview = register(true, "environment-cli-preview");
    assert!(preview.status.success());
    assert_eq!(parse_stdout(&preview)["dry_run"], true);
    assert_eq!(parse_stdout(&preview)["environment"], Value::Null);
    assert_eq!(
        parse_stdout(&mcl(&root, &["environment", "list"])),
        json!([])
    );

    let created = register(false, "environment-cli-register");
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created = parse_stdout(&created);
    let expected_hash = include_str!("../fixtures/environment/lean-4.32-local.sha256").trim();
    assert_eq!(created["proposed_environment_hash"], expected_hash);
    assert_eq!(created["environment"]["environment_hash"], expected_hash);
    assert_eq!(created["environment"]["created_by"], "environment-cli-test");

    let retried = parse_stdout(&register(false, "environment-cli-register"));
    assert_eq!(retried["environment"], created["environment"]);
    let loaded = mcl(
        &root,
        &["environment", "get", "--environment-hash", expected_hash],
    );
    assert!(loaded.status.success());
    assert_eq!(parse_stdout(&loaded), created["environment"]);
    assert_eq!(
        parse_stdout(&mcl(&root, &["environment", "list", "--limit", "10"]))
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let mut invalid: Value = serde_json::from_str(&manifest).expect("manifest JSON");
    invalid["machine_name"] = json!("must-not-enter-identity");
    let invalid = mcl_owned(
        &root,
        &[
            "environment".to_owned(),
            "register".to_owned(),
            "--manifest-json".to_owned(),
            invalid.to_string(),
            "--actor".to_owned(),
            "environment-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "environment-cli-invalid".to_owned(),
        ],
    );
    assert!(!invalid.status.success());
    let error: Value = serde_json::from_slice(&invalid.stderr).expect("environment error JSON");
    assert_eq!(error["code"], "MCL_ENVIRONMENT_JSON_INVALID");

    let doctor = mcl(&root, &["doctor"]);
    let doctor = parse_stdout(&doctor);
    let environment_check = doctor["checks"]
        .as_array()
        .expect("doctor checks")
        .iter()
        .find(|check| check["name"] == "environments")
        .expect("environment doctor check");
    assert_eq!(environment_check["healthy"], true);
    assert!(
        environment_check["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("registered=1"))
    );
}

#[test]
fn artifact_cli_ingests_dry_runs_retries_verifies_and_detects_corruption() {
    let root = TempDir::new().expect("temporary root");
    assert!(
        mcl(
            &root,
            &[
                "init",
                "--actor",
                "artifact-cli-test",
                "--idempotency-key",
                "artifact-cli-init",
            ],
        )
        .status
        .success()
    );
    let module = root.path().join("ArtifactFixture.lean");
    fs::write(&module, b"theorem artifactFixture : True := by trivial\n").expect("fixture writes");
    let metadata = json!({
        "schema_version": "artifact_metadata/1",
        "media_type": "text/x-lean",
        "creation_source": "user_ingest",
        "license_expression": "PolyForm-Noncommercial-1.0.0",
        "restriction": "restricted",
        "semantic_metadata": {"declaration_name": "MathOS.ArtifactFixture"}
    })
    .to_string();
    let ingest = |dry_run: bool, idempotency_key: &str| {
        let mut arguments = vec![
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            module.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            metadata.clone(),
            "--actor".to_owned(),
            "artifact-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            idempotency_key.to_owned(),
        ];
        if dry_run {
            arguments.push("--dry-run".to_owned());
        }
        mcl_owned(&root, &arguments)
    };

    let preview = parse_stdout(&ingest(true, "artifact-cli-preview"));
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["artifact"], Value::Null);
    assert_eq!(parse_stdout(&mcl(&root, &["artifact", "list"])), json!([]));

    let forged_provenance = mcl_owned(
        &root,
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            module.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            json!({
                "schema_version": "artifact_metadata/1",
                "media_type": "text/x-lean",
                "creation_source": "verifier",
                "license_expression": null,
                "restriction": "private",
                "semantic_metadata": {}
            })
            .to_string(),
            "--actor".to_owned(),
            "artifact-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "artifact-cli-forged-provenance".to_owned(),
        ],
    );
    assert!(!forged_provenance.status.success());
    let forged_error: Value =
        serde_json::from_slice(&forged_provenance.stderr).expect("forged provenance error JSON");
    assert_eq!(
        forged_error["code"],
        "MCL_ARTIFACT_CREATION_SOURCE_FORBIDDEN"
    );

    let created_output = ingest(false, "artifact-cli-register");
    assert!(
        created_output.status.success(),
        "{}",
        String::from_utf8_lossy(&created_output.stderr)
    );
    let created = parse_stdout(&created_output);
    let hash = created["proposed_artifact_hash"]
        .as_str()
        .expect("artifact hash");
    assert_eq!(created["artifact"]["artifact_hash"], hash);
    assert_eq!(created["artifact"]["media_type"], "text/x-lean");
    assert_eq!(
        parse_stdout(&ingest(false, "artifact-cli-register"))["artifact"],
        created["artifact"]
    );
    assert_eq!(
        parse_stdout(&mcl(&root, &["artifact", "get", "--artifact-hash", hash])),
        created["artifact"]
    );
    let verified = parse_stdout(&mcl(
        &root,
        &["artifact", "verify", "--artifact-hash", hash],
    ));
    assert_eq!(verified["content_hash_verified"], true);
    assert_eq!(verified["metadata_verified"], true);

    let orphan_bytes = b"crash window orphan";
    let orphan_hash = format!("{:x}", Sha256::digest(orphan_bytes));
    let orphan_path = root
        .path()
        .join(".mcl/artifacts/sha256")
        .join(&orphan_hash[0..2])
        .join(&orphan_hash[2..4])
        .join(&orphan_hash);
    fs::create_dir_all(orphan_path.parent().expect("orphan parent"))
        .expect("orphan directory creates");
    fs::write(orphan_path, orphan_bytes).expect("orphan bytes write");
    let doctor = parse_stdout(&mcl(&root, &["doctor"]));
    let inventory = doctor["checks"]
        .as_array()
        .expect("doctor checks")
        .iter()
        .find(|check| check["name"] == "artifact_inventory")
        .expect("artifact inventory check");
    assert_eq!(inventory["healthy"], true);
    assert!(
        inventory["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("unregistered_orphans=1"))
    );

    let outside = TempDir::new().expect("outside root");
    let outside_file = outside.path().join("Outside.lean");
    fs::write(&outside_file, b"theorem outside : True := by trivial\n")
        .expect("outside fixture writes");
    let unsafe_input = mcl_owned(
        &root,
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            outside_file.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            metadata,
            "--actor".to_owned(),
            "artifact-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "artifact-cli-outside".to_owned(),
        ],
    );
    assert!(!unsafe_input.status.success());
    let unsafe_error: Value =
        serde_json::from_slice(&unsafe_input.stderr).expect("unsafe input error JSON");
    assert_eq!(unsafe_error["code"], "MCL_ARTIFACT_INPUT_UNSAFE");

    let cas_path = root
        .path()
        .join(".mcl/artifacts/sha256")
        .join(&hash[0..2])
        .join(&hash[2..4])
        .join(hash);
    fs::write(cas_path, b"one byte changed").expect("test corrupts CAS bytes");
    let corrupted = mcl(&root, &["artifact", "verify", "--artifact-hash", hash]);
    assert!(!corrupted.status.success());
    let error: Value = serde_json::from_slice(&corrupted.stderr).expect("artifact error JSON");
    assert_eq!(error["code"], "MCL_ARTIFACT_INTEGRITY_FAILED");
}

#[test]
fn verifier_job_cli_dry_runs_enqueues_retries_and_survives_restart() {
    let root = TempDir::new().expect("temporary root");
    assert!(
        mcl(
            &root,
            &[
                "init",
                "--actor",
                "verifier-cli-test",
                "--idempotency-key",
                "verifier-cli-init",
            ],
        )
        .status
        .success()
    );
    let environment = mcl(
        &root,
        &[
            "environment",
            "register",
            "--manifest-json",
            include_str!("../fixtures/environment/lean-4.32-local.json"),
            "--actor",
            "verifier-cli-test",
            "--idempotency-key",
            "verifier-cli-environment",
        ],
    );
    assert!(environment.status.success());
    let environment_hash = include_str!("../fixtures/environment/lean-4.32-local.sha256").trim();
    let module = root.path().join("VerifierJob.lean");
    fs::write(&module, b"theorem jobFixture : True := by trivial\n").expect("module writes");
    let metadata = json!({
        "schema_version": "artifact_metadata/1",
        "media_type": "text/x-lean",
        "creation_source": "user_ingest",
        "license_expression": "PolyForm-Noncommercial-1.0.0",
        "restriction": "restricted",
        "semantic_metadata": {"declaration_name": "MathOS.Verifier.jobFixture"}
    });
    let ingested = mcl_owned(
        &root,
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            module.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            metadata.to_string(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "verifier-cli-artifact".to_owned(),
        ],
    );
    assert!(ingested.status.success());
    let ingested = parse_stdout(&ingested);
    let artifact_hash = ingested["proposed_artifact_hash"]
        .as_str()
        .expect("artifact hash");
    let check = |dry_run: bool, idempotency_key: &str| {
        let mut arguments = vec![
            "verify".to_owned(),
            "check".to_owned(),
            "--environment-hash".to_owned(),
            environment_hash.to_owned(),
            "--module-artifact-hash".to_owned(),
            artifact_hash.to_owned(),
            "--declaration-name".to_owned(),
            "MathOS.Verifier.jobFixture".to_owned(),
            "--priority".to_owned(),
            "7".to_owned(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            idempotency_key.to_owned(),
        ];
        if dry_run {
            arguments.push("--dry-run".to_owned());
        }
        mcl_owned(&root, &arguments)
    };

    let preview = parse_stdout(&check(true, "verifier-cli-preview"));
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["job"], Value::Null);
    assert_eq!(parse_stdout(&mcl(&root, &["verify", "list"])), json!([]));

    let created = parse_stdout(&check(false, "verifier-cli-check"));
    assert_eq!(created["job"]["state"], "queued");
    assert_eq!(created["job"]["attempt_count"], 0);
    assert_eq!(
        parse_stdout(&check(false, "verifier-cli-check"))["job"],
        created["job"]
    );
    let job_id = created["job"]["job_id"].as_str().expect("job ID");
    assert_eq!(
        parse_stdout(&mcl(&root, &["verify", "status", "--job-id", job_id])),
        created["job"]
    );
    assert_eq!(
        parse_stdout(&mcl(&root, &["verify", "list", "--limit", "10"]))
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let injection = mcl(
        &root,
        &[
            "verify",
            "check",
            "--environment-hash",
            environment_hash,
            "--module-artifact-hash",
            artifact_hash,
            "--declaration-name",
            "Truth;rm",
            "--actor",
            "verifier-cli-test",
            "--idempotency-key",
            "verifier-cli-injection",
        ],
    );
    assert!(!injection.status.success());
    let error: Value = serde_json::from_slice(&injection.stderr).expect("verifier error JSON");
    assert_eq!(error["code"], "MCL_VERIFIER_REQUEST_INVALID");

    let unsafe_module = root.path().join("UnsafeVerifierJob.lean");
    fs::write(
        &unsafe_module,
        b"theorem unsafeFixture : True := by sorry\n",
    )
    .expect("unsafe module writes");
    let unsafe_ingest = mcl_owned(
        &root,
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            unsafe_module.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            metadata.to_string(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "verifier-cli-unsafe-artifact".to_owned(),
        ],
    );
    assert!(unsafe_ingest.status.success());
    let unsafe_ingest = parse_stdout(&unsafe_ingest);
    let unsafe_hash = unsafe_ingest["proposed_artifact_hash"]
        .as_str()
        .expect("unsafe artifact hash");
    let unsafe_job = mcl_owned(
        &root,
        &[
            "verify".to_owned(),
            "check".to_owned(),
            "--environment-hash".to_owned(),
            environment_hash.to_owned(),
            "--module-artifact-hash".to_owned(),
            unsafe_hash.to_owned(),
            "--declaration-name".to_owned(),
            "MathOS.Verifier.unsafeFixture".to_owned(),
            "--priority".to_owned(),
            "100".to_owned(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "verifier-cli-unsafe-job".to_owned(),
        ],
    );
    assert!(unsafe_job.status.success());
    let unsafe_job = parse_stdout(&unsafe_job);
    let unsafe_job_id = unsafe_job["job"]["job_id"]
        .as_str()
        .expect("unsafe job ID")
        .to_owned();
    let worked = mcl(
        &root,
        &[
            "worker",
            "--worker-id",
            "cli-worker",
            "--lease-seconds",
            "3660",
        ],
    );
    assert!(
        worked.status.success(),
        "{}",
        String::from_utf8_lossy(&worked.stderr)
    );
    let worked = parse_stdout(&worked);
    assert_eq!(worked["report"]["classification"], "unsafe_source");
    assert_eq!(worked["report"]["forbidden_source_token"], "sorry");
    assert_eq!(worked["report"]["authoritative"], false);
    assert_eq!(worked["job"]["state"], "succeeded");
    assert!(worked["job"]["result_artifact_hash"].is_string());
    let original_report_hash = worked["job"]["result_artifact_hash"]
        .as_str()
        .expect("original report hash")
        .to_owned();
    let forged_report_file = root.path().join("ForgedVerifierReport.json");
    let mut forged_report_value = worked["report"].clone();
    forged_report_value["authoritative"] = json!(true);
    fs::write(&forged_report_file, forged_report_value.to_string())
        .expect("forged report fixture writes");
    let forged_report = mcl_owned(
        &root,
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            forged_report_file.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            json!({
                "schema_version": "artifact_metadata/1",
                "media_type": "application/json",
                "creation_source": "user_ingest",
                "license_expression": null,
                "restriction": "private",
                "semantic_metadata": {
                    "job_id": unsafe_job_id,
                    "artifact_role": "verifier_report"
                }
            })
            .to_string(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "evidence-forged-report-artifact".to_owned(),
        ],
    );
    assert!(
        forged_report.status.success(),
        "{}",
        String::from_utf8_lossy(&forged_report.stderr)
    );
    let forged_report_hash = parse_stdout(&forged_report)["artifact"]["artifact_hash"]
        .as_str()
        .expect("forged report hash")
        .to_owned();

    let source_payload = json!({
        "source_type": "user_statement",
        "title_or_label": "Unsafe evidence fixture",
        "authors_or_origin": ["CLI test"],
        "canonical_locator": "local:unsafe-evidence",
        "acquisition_date": "2026-07-19",
        "license_expression": null,
        "redistribution_status": "unknown",
        "content_hash": null,
        "citation_metadata": {},
        "redaction_class": "private",
        "provenance_notes": "diagnostic evidence integration fixture",
        "original_text": "True has a proof."
    });
    let source = parse_stdout(&mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            source_payload.to_string(),
            "--searchable-text".to_owned(),
            "unsafe evidence source".to_owned(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "evidence-source".to_owned(),
        ],
    ));
    let source_record = &source["record"];
    let claim_payload = json!({
        "source_reference": {
            "object_id": source_record["object_id"],
            "version_hash": source_record["version_hash"]
        },
        "normalized_informal_statement": "True has a proof.",
        "claim_kind": "existential",
        "logical_shape": "True",
        "assumptions": [],
        "variables": [],
        "concept_links": [],
        "source_citations": [],
        "ambiguity_notes": []
    });
    let claim = parse_stdout(&mcl_owned(
        &root,
        &[
            "claim".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            claim_payload.to_string(),
            "--searchable-text".to_owned(),
            "true proof claim".to_owned(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "evidence-claim".to_owned(),
        ],
    ));
    let claim_record = &claim["record"];
    let formalization_payload = json!({
        "claim_version": {
            "object_id": claim_record["object_id"],
            "version_hash": claim_record["version_hash"]
        },
        "formal_system": "lean4",
        "environment_hash": environment_hash,
        "module_artifact_hash": unsafe_hash,
        "declaration_name": "MathOS.Verifier.unsafeFixture",
        "exact_theorem_type": "True",
        "declaration_hash": "1".repeat(64),
        "import_manifest": [],
        "formalization_notes": "unsafe fixture remains diagnostic",
        "fidelity_evidence_references": [],
        "verification_evidence_references": []
    });
    let formalization = parse_stdout(&mcl_owned(
        &root,
        &[
            "formalization".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            formalization_payload.to_string(),
            "--searchable-text".to_owned(),
            "unsafe evidence formalization".to_owned(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "evidence-formalization".to_owned(),
        ],
    ));
    let formalization_record = &formalization["record"];
    let mut mismatched_payload = formalization_payload.clone();
    mismatched_payload["declaration_name"] = json!("MathOS.Verifier.differentFixture");
    let mismatched_formalization = parse_stdout(&mcl_owned(
        &root,
        &[
            "formalization".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            mismatched_payload.to_string(),
            "--searchable-text".to_owned(),
            "mismatched evidence formalization".to_owned(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "evidence-mismatched-formalization".to_owned(),
        ],
    ));
    let mismatched_record = &mismatched_formalization["record"];
    let mismatched = mcl_owned(
        &root,
        &[
            "verify".to_owned(),
            "promote-diagnostic".to_owned(),
            "--formalization-object-id".to_owned(),
            mismatched_record["object_id"]
                .as_str()
                .expect("mismatched formalization ID")
                .to_owned(),
            "--formalization-version-hash".to_owned(),
            mismatched_record["version_hash"]
                .as_str()
                .expect("mismatched formalization hash")
                .to_owned(),
            "--job-id".to_owned(),
            unsafe_job_id.clone(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "evidence-mismatched-promotion".to_owned(),
        ],
    );
    assert!(!mismatched.status.success());
    let mismatched_error: Value =
        serde_json::from_slice(&mismatched.stderr).expect("mismatch error JSON");
    assert_eq!(
        mismatched_error["code"],
        "MCL_EVIDENCE_FORMALIZATION_MISMATCH"
    );
    assert_eq!(
        parse_stdout(&mcl(&root, &["verify", "evidence-list"])),
        json!([])
    );
    let cross_object_version = mcl_owned(
        &root,
        &[
            "verify".to_owned(),
            "promote-diagnostic".to_owned(),
            "--formalization-object-id".to_owned(),
            formalization_record["object_id"]
                .as_str()
                .expect("formalization ID")
                .to_owned(),
            "--formalization-version-hash".to_owned(),
            mismatched_record["version_hash"]
                .as_str()
                .expect("other formalization version")
                .to_owned(),
            "--job-id".to_owned(),
            unsafe_job_id.clone(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "evidence-cross-object-version".to_owned(),
        ],
    );
    assert!(!cross_object_version.status.success());
    let version_error: Value = serde_json::from_slice(&cross_object_version.stderr)
        .expect("cross-object version error JSON");
    assert_eq!(version_error["code"], "MCL_EVIDENCE_SUBJECT_INVALID");
    let promote = |dry_run: bool, key: &str| {
        let mut arguments = vec![
            "verify".to_owned(),
            "promote-diagnostic".to_owned(),
            "--formalization-object-id".to_owned(),
            formalization_record["object_id"]
                .as_str()
                .expect("formalization ID")
                .to_owned(),
            "--formalization-version-hash".to_owned(),
            formalization_record["version_hash"]
                .as_str()
                .expect("formalization hash")
                .to_owned(),
            "--job-id".to_owned(),
            unsafe_job_id.clone(),
            "--actor".to_owned(),
            "verifier-cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            key.to_owned(),
        ];
        if dry_run {
            arguments.push("--dry-run".to_owned());
        }
        mcl_owned(&root, &arguments)
    };
    let database = rusqlite::Connection::open(root.path().join(".mcl/state.sqlite3"))
        .expect("adversarial database opens");
    database
        .execute("DROP TRIGGER jobs_reject_terminal_rewrite", [])
        .expect("test removes terminal mutation guard");
    database
        .execute(
            "UPDATE jobs SET result_artifact_hash = ?2 WHERE job_id = ?1",
            rusqlite::params![unsafe_job_id, forged_report_hash],
        )
        .expect("test points terminal job at forged report");
    let forged_promotion = promote(false, "evidence-forged-report-promotion");
    assert!(!forged_promotion.status.success());
    let forged_promotion_error: Value = serde_json::from_slice(&forged_promotion.stderr)
        .expect("forged report promotion error JSON");
    assert_eq!(
        forged_promotion_error["code"],
        "MCL_EVIDENCE_REPORT_INVALID"
    );
    database
        .execute(
            "UPDATE jobs SET result_artifact_hash = ?2 WHERE job_id = ?1",
            rusqlite::params![unsafe_job_id, original_report_hash],
        )
        .expect("test restores canonical report");
    drop(database);
    let preview = parse_stdout(&promote(true, "evidence-preview"));
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["evidence"], Value::Null);
    assert_eq!(
        parse_stdout(&mcl(&root, &["verify", "evidence-list"])),
        json!([])
    );
    let promoted = parse_stdout(&promote(false, "evidence-promote"));
    assert_eq!(
        promoted["evidence"]["payload"]["authority_class"],
        "diagnostic"
    );
    assert_eq!(promoted["evidence"]["payload"]["result"], "rejected");
    assert_eq!(
        promoted["evidence"]["payload"]["producing_job_id"],
        unsafe_job_id
    );
    assert_eq!(parse_stdout(&promote(false, "evidence-promote")), promoted);
    let evidence_id = promoted["evidence"]["evidence_id"]
        .as_str()
        .expect("evidence ID");
    assert_eq!(
        parse_stdout(&mcl(
            &root,
            &["verify", "evidence", "--evidence-id", evidence_id]
        )),
        promoted["evidence"]
    );
    assert_eq!(
        parse_stdout(&mcl(&root, &["verify", "evidence-list"]))
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let report_hash = worked["job"]["result_artifact_hash"]
        .as_str()
        .expect("report hash");
    let report_path = root
        .path()
        .join(".mcl/artifacts/sha256")
        .join(&report_hash[..2])
        .join(&report_hash[2..4])
        .join(report_hash);
    fs::write(report_path, b"{}").expect("corrupt verifier report fixture");
    let corrupted = promote(false, "evidence-after-report-corruption");
    assert!(!corrupted.status.success());
    let corrupted_error: Value =
        serde_json::from_slice(&corrupted.stderr).expect("corruption error JSON");
    assert_eq!(corrupted_error["code"], "MCL_ARTIFACT_INTEGRITY_FAILED");
}

#[test]
fn dry_run_writes_no_instance_state() {
    let root = TempDir::new().expect("temporary root");
    let output = mcl(
        &root,
        &[
            "init",
            "--dry-run",
            "--actor",
            "phase1-test",
            "--idempotency-key",
            "preview-001",
        ],
    );
    assert!(output.status.success());
    assert_eq!(parse_stdout(&output)["dry_run"], true);
    assert!(!root.path().join("mcl.toml").exists());
    assert!(!root.path().join(".mcl").exists());
}

#[test]
fn health_does_not_create_a_missing_database() {
    let root = TempDir::new().expect("temporary root");
    let output = mcl(&root, &["health"]);
    assert!(!output.status.success());
    assert_eq!(parse_stdout(&output)["healthy"], false);
    assert!(!root.path().join(".mcl/state.sqlite3").exists());
}

#[test]
fn dry_run_does_not_create_a_missing_root() {
    let parent = TempDir::new().expect("temporary parent");
    let missing = parent.path().join("missing");
    let output = mcl_at(
        &missing,
        &[
            "init",
            "--dry-run",
            "--actor",
            "phase1-test",
            "--idempotency-key",
            "preview-missing-001",
        ],
    );

    assert!(!output.status.success());
    assert!(!missing.exists());
    let error: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON");
    assert_eq!(error["code"], "MCL_DRY_RUN_ROOT_MISSING");
}

#[test]
fn canonical_source_cli_uses_one_service_for_dry_run_create_version_get_and_search() {
    let root = TempDir::new().expect("temporary root");
    assert!(
        mcl(
            &root,
            &[
                "init",
                "--actor",
                "cli-test",
                "--idempotency-key",
                "cli-init",
            ],
        )
        .status
        .success()
    );

    let source = json!({
        "source_type": "user_statement",
        "title_or_label": "Prime parity",
        "authors_or_origin": ["CLI fixture"],
        "canonical_locator": "local:prime-parity",
        "acquisition_date": "2026-07-19",
        "license_expression": null,
        "redistribution_status": "unknown",
        "content_hash": null,
        "citation_metadata": {},
        "redaction_class": "private",
        "provenance_notes": "CLI integration fixture",
        "original_text": "Every prime number is odd."
    });
    let payload = serde_json::to_string(&source).expect("source JSON");
    let dry_run = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            payload.clone(),
            "--searchable-text".to_owned(),
            "uncommitted preview marker".to_owned(),
            "--dry-run".to_owned(),
            "--actor".to_owned(),
            "cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "source-preview".to_owned(),
        ],
    );
    assert!(dry_run.status.success());
    assert_eq!(parse_stdout(&dry_run)["dry_run"], true);
    let absent = mcl(
        &root,
        &["search", "--query", "uncommitted", "--limit", "10"],
    );
    assert!(absent.status.success());
    assert_eq!(parse_stdout(&absent), json!([]));

    let created = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            payload,
            "--searchable-text".to_owned(),
            "prime parity original".to_owned(),
            "--actor".to_owned(),
            "cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "source-create".to_owned(),
        ],
    );
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created = parse_stdout(&created);
    let mut normalized_record = created.clone();
    normalize_hash(&mut normalized_record, "/proposed_version_hash");
    normalize_uuid(&mut normalized_record, "/record/object_id");
    normalize_hash(&mut normalized_record, "/record/version_hash");
    normalized_record["record"]["created_at"] = json!("<timestamp>");
    assert_eq!(
        normalized_record,
        golden(include_str!("../fixtures/cli/record-mutation.json"))
    );
    let object_id = created["record"]["object_id"]
        .as_str()
        .expect("object ID")
        .to_owned();
    let first_hash = created["record"]["version_hash"]
        .as_str()
        .expect("version hash")
        .to_owned();

    let retried = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            serde_json::to_string(&source).expect("retry JSON"),
            "--searchable-text".to_owned(),
            "prime parity original".to_owned(),
            "--actor".to_owned(),
            "cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "source-create".to_owned(),
        ],
    );
    assert!(retried.status.success());
    assert_eq!(parse_stdout(&retried)["record"], created["record"]);

    let loaded = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "get".to_owned(),
            "--object-id".to_owned(),
            object_id.clone(),
        ],
    );
    assert!(loaded.status.success());
    assert_eq!(parse_stdout(&loaded)["version_hash"], first_hash);

    let mut revised = source;
    revised["original_text"] = json!("Every prime greater than two is odd.");
    let versioned = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "version".to_owned(),
            "--object-id".to_owned(),
            object_id.clone(),
            "--expected-head".to_owned(),
            first_hash.clone(),
            "--payload-json".to_owned(),
            serde_json::to_string(&revised).expect("revised JSON"),
            "--searchable-text".to_owned(),
            "prime parity repaired".to_owned(),
            "--actor".to_owned(),
            "cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "source-version".to_owned(),
        ],
    );
    assert!(
        versioned.status.success(),
        "{}",
        String::from_utf8_lossy(&versioned.stderr)
    );
    let second_hash = parse_stdout(&versioned)["record"]["version_hash"]
        .as_str()
        .expect("second hash")
        .to_owned();
    assert_ne!(first_hash, second_hash);

    let mut stale_revision = revised.clone();
    stale_revision["provenance_notes"] = json!("stale writer");
    let stale = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "version".to_owned(),
            "--object-id".to_owned(),
            object_id.clone(),
            "--expected-head".to_owned(),
            first_hash.clone(),
            "--payload-json".to_owned(),
            serde_json::to_string(&stale_revision).expect("stale JSON"),
            "--searchable-text".to_owned(),
            "stale write".to_owned(),
            "--actor".to_owned(),
            "cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            "source-stale-version".to_owned(),
        ],
    );
    assert!(!stale.status.success());
    let stale_error: Value = serde_json::from_slice(&stale.stderr).expect("stale error JSON");
    assert_eq!(stale_error["code"], "MCL_VERSION_CONFLICT");

    let historical = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "get".to_owned(),
            "--object-id".to_owned(),
            object_id.clone(),
            "--version-hash".to_owned(),
            first_hash.clone(),
        ],
    );
    assert!(historical.status.success());
    assert_eq!(parse_stdout(&historical)["version_hash"], first_hash);

    let search = mcl(&root, &["search", "--query", "repaired", "--limit", "10"]);
    assert!(search.status.success());
    assert_eq!(parse_stdout(&search)[0]["version_hash"], second_hash);

    let wrong_family = mcl_owned(
        &root,
        &[
            "claim".to_owned(),
            "get".to_owned(),
            "--object-id".to_owned(),
            object_id,
        ],
    );
    assert!(!wrong_family.status.success());
    let error: Value = serde_json::from_slice(&wrong_family.stderr).expect("error JSON");
    assert_eq!(error["code"], "MCL_RECORD_KIND_MISMATCH");
}

#[test]
fn research_and_graph_cli_share_the_service_and_preserve_conflicts_and_dry_runs() {
    let root = TempDir::new().expect("temporary root");
    assert!(
        mcl(
            &root,
            &[
                "init",
                "--actor",
                "cli-test",
                "--idempotency-key",
                "bridge-init",
            ],
        )
        .status
        .success()
    );
    let source = create_source_record(&root, "source node", "bridge-source");
    let target = create_source_record(&root, "target node", "bridge-target");
    let source_id = source["object_id"].as_str().expect("source ID").to_owned();
    let source_hash = source["version_hash"]
        .as_str()
        .expect("source hash")
        .to_owned();
    let target_id = target["object_id"].as_str().expect("target ID").to_owned();
    let target_hash = target["version_hash"]
        .as_str()
        .expect("target hash")
        .to_owned();

    let edge_arguments = |dry_run: bool, key: &str| {
        let mut arguments = vec![
            "edge".to_owned(),
            "create".to_owned(),
            "--kind".to_owned(),
            "logic.depends_on".to_owned(),
            "--source-object-id".to_owned(),
            source_id.clone(),
            "--source-version-hash".to_owned(),
            source_hash.clone(),
            "--target-object-id".to_owned(),
            target_id.clone(),
            "--target-version-hash".to_owned(),
            target_hash.clone(),
            "--payload-json".to_owned(),
            "{\"reason\":\"fixture\"}".to_owned(),
            "--actor".to_owned(),
            "cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            key.to_owned(),
        ];
        if dry_run {
            arguments.push("--dry-run".to_owned());
        }
        arguments
    };
    let preview = mcl_owned(&root, &edge_arguments(true, "edge-preview"));
    assert!(preview.status.success());
    assert_eq!(parse_stdout(&preview)["dry_run"], true);
    let empty_graph = mcl_owned(
        &root,
        &[
            "graph".to_owned(),
            "--root-object-id".to_owned(),
            source_id.clone(),
            "--root-version-hash".to_owned(),
            source_hash.clone(),
            "--direction".to_owned(),
            "outgoing".to_owned(),
            "--edge-kind".to_owned(),
            "logic.depends_on".to_owned(),
        ],
    );
    assert!(empty_graph.status.success());
    assert_eq!(parse_stdout(&empty_graph), json!([]));

    let edge = mcl_owned(&root, &edge_arguments(false, "edge-create"));
    assert!(edge.status.success());
    let edge_outcome = parse_stdout(&edge);
    let mut normalized_edge = edge_outcome.clone();
    normalize_uuid(&mut normalized_edge, "/edge/edge_id");
    normalize_uuid(&mut normalized_edge, "/edge/source_object_id");
    normalize_hash(&mut normalized_edge, "/edge/source_version_hash");
    normalize_uuid(&mut normalized_edge, "/edge/target_object_id");
    normalize_hash(&mut normalized_edge, "/edge/target_version_hash");
    normalized_edge["edge"]["created_at"] = json!("<timestamp>");
    assert_eq!(
        normalized_edge,
        golden(include_str!("../fixtures/cli/edge-mutation.json"))
    );
    let edge = edge_outcome["edge"].clone();
    let edge_id = edge["edge_id"].as_str().expect("edge ID");
    let loaded_edge = mcl_owned(
        &root,
        &[
            "edge".to_owned(),
            "get".to_owned(),
            "--edge-id".to_owned(),
            edge_id.to_owned(),
        ],
    );
    assert!(loaded_edge.status.success());
    assert_eq!(parse_stdout(&loaded_edge), edge);
    let graph = mcl_owned(
        &root,
        &[
            "graph".to_owned(),
            "--root-object-id".to_owned(),
            source_id.clone(),
            "--root-version-hash".to_owned(),
            source_hash.clone(),
            "--direction".to_owned(),
            "outgoing".to_owned(),
            "--edge-kind".to_owned(),
            "logic.depends_on".to_owned(),
            "--max-depth".to_owned(),
            "2".to_owned(),
            "--limit".to_owned(),
            "10".to_owned(),
        ],
    );
    assert!(graph.status.success());
    assert_eq!(parse_stdout(&graph)[0]["edge"]["edge_id"], edge_id);

    let run_preview = mcl(
        &root,
        &[
            "research",
            "start",
            "--kind",
            "formalize",
            "--budget-json",
            "{\"steps\":2}",
            "--dry-run",
            "--actor",
            "cli-test",
            "--idempotency-key",
            "run-preview",
        ],
    );
    assert!(run_preview.status.success());
    assert_eq!(parse_stdout(&run_preview)["dry_run"], true);
    assert_eq!(parse_stdout(&run_preview)["run"], Value::Null);

    let started = mcl(
        &root,
        &[
            "research",
            "start",
            "--kind",
            "formalize",
            "--budget-json",
            "{\"steps\":2}",
            "--actor",
            "cli-test",
            "--idempotency-key",
            "run-start",
        ],
    );
    assert!(started.status.success());
    let run = parse_stdout(&started)["run"].clone();
    let run_id = run["run_id"].as_str().expect("run ID").to_owned();
    let initial_head = run["event_head_hash"]
        .as_str()
        .expect("initial head")
        .to_owned();
    let submit_arguments = |key: &str, dry_run: bool| {
        let mut arguments = vec![
            "research".to_owned(),
            "submit".to_owned(),
            "--run-id".to_owned(),
            run_id.clone(),
            "--expected-head".to_owned(),
            initial_head.clone(),
            "--kind".to_owned(),
            "observation".to_owned(),
            "--payload-json".to_owned(),
            "{\"note\":\"boundary case\"}".to_owned(),
            "--actor".to_owned(),
            "cli-test".to_owned(),
            "--idempotency-key".to_owned(),
            key.to_owned(),
        ];
        if dry_run {
            arguments.push("--dry-run".to_owned());
        }
        arguments
    };
    let event_preview = mcl_owned(&root, &submit_arguments("event-preview", true));
    assert!(event_preview.status.success());
    assert_eq!(parse_stdout(&event_preview)["event"], Value::Null);
    let unchanged = mcl_owned(
        &root,
        &[
            "research".to_owned(),
            "get".to_owned(),
            "--run-id".to_owned(),
            run_id.clone(),
        ],
    );
    assert_eq!(parse_stdout(&unchanged)["event_count"], 1);

    let submitted = mcl_owned(&root, &submit_arguments("event-submit", false));
    assert!(submitted.status.success());
    let event = parse_stdout(&submitted)["event"].clone();
    let retried = mcl_owned(&root, &submit_arguments("event-submit", false));
    assert!(retried.status.success());
    assert_eq!(parse_stdout(&retried)["event"], event);
    let conflict = mcl_owned(&root, &submit_arguments("event-conflict", false));
    assert!(!conflict.status.success());
    let conflict: Value = serde_json::from_slice(&conflict.stderr).expect("conflict JSON");
    assert_eq!(conflict["code"], "MCL_RUN_EVENT_CONFLICT");

    let events = mcl_owned(
        &root,
        &[
            "research".to_owned(),
            "events".to_owned(),
            "--run-id".to_owned(),
            run_id.clone(),
        ],
    );
    assert!(events.status.success());
    assert_eq!(parse_stdout(&events).as_array().expect("events").len(), 2);
    let verified = mcl_owned(
        &root,
        &[
            "research".to_owned(),
            "verify".to_owned(),
            "--run-id".to_owned(),
            run_id.clone(),
        ],
    );
    assert!(verified.status.success());
    let verified = parse_stdout(&verified);
    assert_eq!(verified["valid"], true);
    assert_eq!(verified["event_count"], 2);
    let mut normalized_report = verified;
    normalize_uuid(&mut normalized_report, "/run_id");
    normalize_hash(&mut normalized_report, "/head_hash");
    assert_eq!(
        normalized_report,
        golden(include_str!("../fixtures/cli/run-chain-report.json"))
    );
}

#[test]
fn publication_candidate_cli_bounds_workflow_json_inputs() {
    let root = TempDir::new().expect("temporary root");
    assert!(
        mcl(
            &root,
            &[
                "init",
                "--actor",
                "publication-candidate-test",
                "--idempotency-key",
                "publication-candidate-init",
            ],
        )
        .status
        .success()
    );
    fs::write(root.path().join("report.json"), vec![b'x'; 1_048_577])
        .expect("oversized publication report");
    fs::write(root.path().join("retained-closure.json"), b"{}")
        .expect("retained closure placeholder");

    let output = mcl(
        &root,
        &[
            "verify",
            "validate-publication-candidate",
            "--report-file",
            "report.json",
            "--retained-closure-file",
            "retained-closure.json",
            "--retained-root",
            ".",
        ],
    );
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).expect("candidate error JSON");
    assert_eq!(error["code"], "MCL_PUBLICATION_CANDIDATE_INPUT_TOO_LARGE");
}
