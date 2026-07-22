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

fn assert_cli_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn pedagogy_source_payload() -> Value {
    json!({
        "source_type": "user_statement",
        "title_or_label": "Pilot A canonical pedagogy source",
        "authors_or_origin": ["MathOS playtest"],
        "canonical_locator": "local:pilot-a-pedagogy",
        "acquisition_date": "2026-07-21",
        "license_expression": "CC-BY-4.0",
        "redistribution_status": "allowed",
        "content_hash": null,
        "citation_metadata": {},
        "redaction_class": "public",
        "provenance_notes": "Issue #48 CLI fixture",
        "original_text": "Every prime number is odd."
    })
}

fn create_pedagogy_source(root: &TempDir) -> Value {
    let output = mcl_owned(
        root,
        &[
            "source".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            pedagogy_source_payload().to_string(),
            "--searchable-text".to_owned(),
            "Pilot A canonical pedagogy source".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "pedagogy-source".to_owned(),
        ],
    );
    assert_cli_success(&output);
    parse_stdout(&output)["record"].clone()
}

fn create_pedagogy_claim(root: &TempDir, source: &Value) -> Value {
    let reference = json!({
        "object_id": source["object_id"],
        "version_hash": source["version_hash"]
    });
    let payload = json!({
        "source_reference": reference,
        "normalized_informal_statement": "For every natural number n, if n is prime, then n is odd.",
        "claim_kind": "universal",
        "logical_shape": "forall n : Nat, Nat.Prime n -> Odd n",
        "assumptions": ["Variables range over natural numbers."],
        "variables": [],
        "concept_links": [],
        "source_citations": [reference],
        "ambiguity_notes": []
    });
    let output = mcl_owned(
        root,
        &[
            "claim".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            payload.to_string(),
            "--searchable-text".to_owned(),
            "Pilot A repaired statement".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "pedagogy-claim".to_owned(),
        ],
    );
    assert_cli_success(&output);
    parse_stdout(&output)["record"].clone()
}

fn ingest_pedagogy_content(root: &TempDir) -> String {
    let content_path = root.path().join("learning-unit.txt");
    fs::write(
        &content_path,
        "Explain why the counterexample survives while the repair narrows the domain.\n",
    )
    .expect("learning content writes");
    let metadata = json!({
        "schema_version": "artifact_metadata/1",
        "media_type": "text/plain",
        "creation_source": "user_ingest",
        "license_expression": "CC-BY-4.0",
        "restriction": "public",
        "semantic_metadata": {"artifact_role": "learning_unit_content"}
    });
    let output = mcl_owned(
        root,
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            content_path.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            metadata.to_string(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "pedagogy-content".to_owned(),
        ],
    );
    assert_cli_success(&output);
    parse_stdout(&output)["proposed_artifact_hash"]
        .as_str()
        .expect("artifact hash")
        .to_owned()
}

fn pedagogy_payload(
    source: &Value,
    claim: &Value,
    artifact_hash: &str,
    unit_kind: &str,
    objective: &str,
    hard_prerequisites: Vec<Value>,
) -> Value {
    json!({
        "unit_kind": unit_kind,
        "target": {
            "kind": "claim",
            "object_id": claim["object_id"],
            "version_hash": claim["version_hash"]
        },
        "audience_track": "pilot_a_counterexample_repair",
        "entry_assumptions": ["The learner can read a quantified informal statement."],
        "learning_objectives": [objective],
        "hard_prerequisites": hard_prerequisites,
        "soft_prerequisites": [],
        "grounded_source_references": [{
            "object_id": source["object_id"],
            "version_hash": source["version_hash"]
        }],
        "content_artifact_hash": artifact_hash,
        "examples": [],
        "nonexamples": [],
        "counterexamples": [],
        "misconceptions": [],
        "exercises": [],
        "mastery_checks": [],
        "formalization_references": [],
        "application_references": [],
        "frontier_references": [],
        "review": {"state": "draft", "reviewer": null, "notes": []},
        "license_expression": "CC-BY-4.0",
        "training_status": "ineligible"
    })
}

fn propose_pedagogy_unit(root: &TempDir, payload: &Value, key: &str, dry_run: bool) -> Output {
    let mut arguments = vec![
        "pedagogy".to_owned(),
        "propose".to_owned(),
        "--payload-json".to_owned(),
        payload.to_string(),
        "--searchable-text".to_owned(),
        "Pilot A pedagogy".to_owned(),
        "--actor".to_owned(),
        "pedagogy-test".to_owned(),
        "--idempotency-key".to_owned(),
        key.to_owned(),
    ];
    if dry_run {
        arguments.push("--dry-run".to_owned());
    }
    mcl_owned(root, &arguments)
}

fn review_pedagogy_unit(
    root: &TempDir,
    object_id: &str,
    expected_head: &str,
    key: &str,
    dry_run: bool,
) -> Output {
    let mut arguments = vec![
        "pedagogy".to_owned(),
        "review".to_owned(),
        "--object-id".to_owned(),
        object_id.to_owned(),
        "--expected-head".to_owned(),
        expected_head.to_owned(),
        "--decision".to_owned(),
        "reviewed".to_owned(),
        "--training-status".to_owned(),
        "eligible_public".to_owned(),
        "--notes-json".to_owned(),
        "[\"Grounding, objective, and license reviewed.\"]".to_owned(),
        "--actor".to_owned(),
        "pedagogy-reviewer".to_owned(),
        "--idempotency-key".to_owned(),
        key.to_owned(),
    ];
    if dry_run {
        arguments.push("--dry-run".to_owned());
    }
    mcl_owned(root, &arguments)
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
    assert_eq!(value["migration_version"], 11);
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
fn claim_status_cli_exposes_exact_read_only_help_and_fails_closed_for_a_missing_claim() {
    let root = TempDir::new().expect("temporary root");
    let help = mcl(&root, &["verify", "claim-status", "--help"]);
    assert!(
        help.status.success(),
        "{}",
        String::from_utf8_lossy(&help.stderr)
    );
    let help = String::from_utf8(help.stdout).expect("help is UTF-8");
    assert!(help.contains("--claim-object-id <CLAIM_OBJECT_ID>"));
    assert!(help.contains("--claim-version-hash <CLAIM_VERSION_HASH>"));
    for forbidden in ["--actor", "--idempotency-key", "--dry-run", "--outcome"] {
        assert!(
            !help.contains(forbidden),
            "claim-status exposed {forbidden}"
        );
    }

    let fidelity_help = mcl(&root, &["verify", "review-fidelity", "--help"]);
    assert!(fidelity_help.status.success());
    let fidelity_help = String::from_utf8(fidelity_help.stdout).expect("help is UTF-8");
    assert!(fidelity_help.contains("fidelity_review_request/1"));
    assert!(fidelity_help.contains("fidelity_review_request/2"));

    let initialized = mcl(
        &root,
        &[
            "init",
            "--actor",
            "claim-status-test",
            "--idempotency-key",
            "claim-status-init",
        ],
    );
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let missing = mcl(
        &root,
        &[
            "verify",
            "claim-status",
            "--claim-object-id",
            "01900000-0000-7000-8000-000000000000",
            "--claim-version-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ],
    );
    assert!(!missing.status.success());
    let error: Value = serde_json::from_slice(&missing.stderr).expect("missing claim error JSON");
    assert_eq!(error["code"], "MCL_RECORD_VERSION_NOT_FOUND");
}

#[test]
fn counterexample_cli_exposes_closed_repair_and_read_paths() {
    let root = TempDir::new().expect("temporary root");
    let repair_help = mcl(&root, &["counterexample", "repair", "--help"]);
    assert!(repair_help.status.success());
    let repair_help = String::from_utf8(repair_help.stdout).expect("help is UTF-8");
    for required in [
        "--request-json <REQUEST_JSON>",
        "--actor <ACTOR>",
        "--idempotency-key <IDEMPOTENCY_KEY>",
        "--dry-run",
        "counterexample_repair_request/1",
    ] {
        assert!(
            repair_help.contains(required),
            "repair help omitted {required}"
        );
    }

    let get_help = mcl(&root, &["counterexample", "get", "--help"]);
    assert!(get_help.status.success());
    let get_help = String::from_utf8(get_help.stdout).expect("help is UTF-8");
    assert!(get_help.contains("--artifact-hash <ARTIFACT_HASH>"));
    for forbidden in [
        "--actor",
        "--idempotency-key",
        "--dry-run",
        "--request-json",
    ] {
        assert!(!get_help.contains(forbidden), "get exposed {forbidden}");
    }

    assert!(
        mcl(
            &root,
            &[
                "init",
                "--actor",
                "counterexample-cli-test",
                "--idempotency-key",
                "counterexample-cli-init",
            ],
        )
        .status
        .success()
    );
    let malformed = mcl(
        &root,
        &[
            "counterexample",
            "repair",
            "--request-json",
            "{}",
            "--actor",
            "counterexample-cli-test",
            "--idempotency-key",
            "counterexample-cli-malformed",
            "--dry-run",
        ],
    );
    assert!(!malformed.status.success());
    let error: Value = serde_json::from_slice(&malformed.stderr).expect("error JSON");
    assert_eq!(error["code"], "MCL_COUNTEREXAMPLE_JSON_INVALID");

    let missing = mcl(
        &root,
        &["counterexample", "get", "--artifact-hash", &"a".repeat(64)],
    );
    assert!(!missing.status.success());
    let error: Value = serde_json::from_slice(&missing.stderr).expect("error JSON");
    assert_eq!(error["code"], "MCL_ARTIFACT_NOT_FOUND");
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
fn release_verify_does_not_require_an_instance_root_or_configuration() {
    let parent = TempDir::new().expect("temporary parent");
    let missing_root = parent.path().join("missing-instance");
    let bundle = parent.path().join("empty-release");
    fs::create_dir(&bundle).expect("empty release directory creates");

    let output = mcl_at(
        &missing_root,
        &[
            "release",
            "verify",
            "--bundle-dir",
            bundle.to_str().expect("bundle path is UTF-8"),
            "--expected-manifest-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ],
    );

    assert!(!output.status.success());
    assert!(!missing_root.exists());
    let error: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON");
    assert_eq!(error["code"], "MCL_IO_ERROR");
    assert_ne!(error["code"], "MCL_INSTANCE_NOT_INITIALIZED");
}

#[test]
fn corpus_export_does_not_require_an_instance_root_or_configuration() {
    let parent = TempDir::new().expect("temporary parent");
    let missing_root = parent.path().join("missing-instance");
    let bundle = parent.path().join("empty-release");
    let output_dir = parent.path().join("corpus-export");
    fs::create_dir(&bundle).expect("empty release directory creates");

    let output = mcl_at(
        &missing_root,
        &[
            "release",
            "export",
            "--bundle-dir",
            bundle.to_str().expect("bundle path is UTF-8"),
            "--expected-manifest-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--packet-id",
            "mathos.number_theory.pilot_a_repair.v1",
            "--domain",
            "number_theory",
            "--level",
            "L1_proof_basics",
            "--difficulty-bin",
            "D1",
            "--output-dir",
            output_dir.to_str().expect("output path is UTF-8"),
        ],
    );

    assert!(!output.status.success());
    assert!(!missing_root.exists());
    assert!(!output_dir.exists());
    let error: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON");
    assert_eq!(error["code"], "MCL_IO_ERROR");
    assert_ne!(error["code"], "MCL_INSTANCE_NOT_INITIALIZED");
}

#[test]
fn corpus_export_verification_is_database_independent() {
    let parent = TempDir::new().expect("temporary parent");
    let missing_root = parent.path().join("missing-instance");
    let export = parent.path().join("empty-export");
    let source = parent.path().join("empty-release");
    fs::create_dir(&export).expect("empty export directory creates");
    fs::create_dir(&source).expect("empty release directory creates");

    let output = mcl_at(
        &missing_root,
        &[
            "release",
            "verify-export",
            "--export-dir",
            export.to_str().expect("export path is UTF-8"),
            "--expected-manifest-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--source-bundle-dir",
            source.to_str().expect("source path is UTF-8"),
        ],
    );

    assert!(!output.status.success());
    assert!(!missing_root.exists());
    let error: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON");
    assert_eq!(error["code"], "MCL_IO_ERROR");
    assert_ne!(error["code"], "MCL_INSTANCE_NOT_INITIALIZED");
}

#[test]
fn rl_export_and_verification_do_not_require_an_instance_root_or_configuration() {
    let parent = TempDir::new().expect("temporary parent");
    let missing_root = parent.path().join("missing-instance");
    let source_root = parent.path().join("source-releases");
    let export_dir = parent.path().join("rl-export");
    let output_dir = parent.path().join("new-rl-export");
    let plan_path = parent.path().join("plan.json");
    fs::create_dir(&source_root).expect("source root creates");
    fs::create_dir(&export_dir).expect("empty export creates");
    fs::write(
        &plan_path,
        serde_json::to_vec(&json!({
            "schema_version": "rl_export_plan/1",
            "publication_cutoff": "2026-07-21",
            "releases": [{
                "release_id": "fixture",
                "expected_manifest_hash": "a".repeat(64),
                "split": "held_out_evaluation",
                "published_on": "2026-07-22",
                "benchmark_identity": "fixture-benchmark",
                "leakage_labels": {
                    "theorem_dependency_components": ["fixture-dependency"],
                    "equivalent_formalizations": ["fixture-equivalence"],
                    "shared_sources": ["fixture-source"],
                    "certificate_families": ["fixture-certificate"],
                    "proof_variants": ["fixture-proof"]
                }
            }]
        }))
        .expect("plan JSON"),
    )
    .expect("plan writes");

    let export = mcl_at(
        &missing_root,
        &[
            "release",
            "export-rl",
            "--plan",
            plan_path.to_str().expect("plan path is UTF-8"),
            "--source-root",
            source_root.to_str().expect("source root is UTF-8"),
            "--output-dir",
            output_dir.to_str().expect("output path is UTF-8"),
        ],
    );
    assert!(!export.status.success());
    assert!(!missing_root.exists());
    assert!(!output_dir.exists());
    let export_error: Value = serde_json::from_slice(&export.stderr).expect("export error JSON");
    assert_eq!(export_error["code"], "MCL_IO_ERROR");
    assert_ne!(export_error["code"], "MCL_INSTANCE_NOT_INITIALIZED");

    let verify = mcl_at(
        &missing_root,
        &[
            "release",
            "verify-rl-export",
            "--export-dir",
            export_dir.to_str().expect("export path is UTF-8"),
            "--expected-manifest-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--plan",
            plan_path.to_str().expect("plan path is UTF-8"),
            "--source-root",
            source_root.to_str().expect("source root is UTF-8"),
        ],
    );
    assert!(!verify.status.success());
    assert!(!missing_root.exists());
    let verify_error: Value = serde_json::from_slice(&verify.stderr).expect("verify error JSON");
    assert_eq!(verify_error["code"], "MCL_IO_ERROR");
    assert_ne!(verify_error["code"], "MCL_INSTANCE_NOT_INITIALIZED");
}

#[test]
fn comparator_export_and_verification_do_not_require_an_instance_root_or_configuration() {
    let parent = TempDir::new().expect("temporary parent");
    let missing_root = parent.path().join("missing-instance");
    let bundle = parent.path().join("empty-release");
    let package = parent.path().join("empty-comparator-package");
    let run = parent.path().join("empty-comparator-run");
    let output_dir = parent.path().join("new-comparator-package");
    let plan_path = parent.path().join("plan.json");
    fs::create_dir(&bundle).expect("empty release creates");
    fs::create_dir(&package).expect("empty package creates");
    fs::create_dir(&run).expect("empty Comparator run creates");
    fs::write(&plan_path, b"{}").expect("invalid fixture plan writes");

    let export = mcl_at(
        &missing_root,
        &[
            "release",
            "export-comparator",
            "--plan",
            plan_path.to_str().expect("plan path is UTF-8"),
            "--bundle-dir",
            bundle.to_str().expect("bundle path is UTF-8"),
            "--expected-release-manifest-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--output-dir",
            output_dir.to_str().expect("output path is UTF-8"),
        ],
    );
    assert!(!export.status.success());
    assert!(!missing_root.exists());
    assert!(!output_dir.exists());
    let export_error: Value = serde_json::from_slice(&export.stderr).expect("export error JSON");
    assert_eq!(export_error["code"], "MCL_COMPARATOR_JSON_INVALID");

    let verify = mcl_at(
        &missing_root,
        &[
            "release",
            "verify-comparator-package",
            "--package-dir",
            package.to_str().expect("package path is UTF-8"),
            "--expected-verification-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--plan",
            plan_path.to_str().expect("plan path is UTF-8"),
            "--bundle-dir",
            bundle.to_str().expect("bundle path is UTF-8"),
            "--expected-release-manifest-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ],
    );
    assert!(!verify.status.success());
    assert!(!missing_root.exists());
    let verify_error: Value = serde_json::from_slice(&verify.stderr).expect("verify error JSON");
    assert_eq!(
        verify_error["code"],
        "MCL_COMPARATOR_PACKAGE_INVENTORY_MISMATCH"
    );

    let verify_run = mcl_at(
        &missing_root,
        &[
            "release",
            "verify-comparator-run",
            "--run-dir",
            run.to_str().expect("run path is UTF-8"),
            "--expected-report-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--expected-package-verification-hash",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ],
    );
    assert!(!verify_run.status.success());
    assert!(!missing_root.exists());
    let run_error: Value = serde_json::from_slice(&verify_run.stderr).expect("run error JSON");
    assert_eq!(run_error["code"], "MCL_COMPARATOR_RUN_INVENTORY_MISMATCH");
}

#[test]
fn corpus_export_rejects_unpinned_curation_before_writing() {
    let parent = TempDir::new().expect("temporary parent");
    let missing_root = parent.path().join("missing-instance");
    let bundle = parent.path().join("empty-release");
    let output_dir = parent.path().join("corpus-export");
    fs::create_dir(&bundle).expect("empty release directory creates");

    let output = mcl_at(
        &missing_root,
        &[
            "release",
            "export",
            "--bundle-dir",
            bundle.to_str().expect("bundle path is UTF-8"),
            "--expected-manifest-hash",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--packet-id",
            "mathos.logic.item.v1",
            "--domain",
            "invented_domain",
            "--level",
            "L1_proof_basics",
            "--difficulty-bin",
            "D1",
            "--output-dir",
            output_dir.to_str().expect("output path is UTF-8"),
        ],
    );

    assert!(!output.status.success());
    assert!(!missing_root.exists());
    assert!(!output_dir.exists());
    let error: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON");
    assert_eq!(error["code"], "MCL_CORPUS_EXPORT_CURATION_INVALID");
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
    fs::write(root.path().join("attestation.json"), b"{}").expect("attestation placeholder");

    let outside = TempDir::new().expect("outside publication root");
    let outside_report = outside.path().join("outside-report.json");
    fs::write(&outside_report, b"{}").expect("outside publication report");
    let unsafe_stage = mcl_owned(
        &root,
        &[
            "verify".to_owned(),
            "stage-publication-candidate".to_owned(),
            "--report-file".to_owned(),
            outside_report.to_string_lossy().into_owned(),
            "--retained-closure-file".to_owned(),
            "retained-closure.json".to_owned(),
            "--retained-root".to_owned(),
            ".".to_owned(),
            "--attestation-bundle-file".to_owned(),
            "attestation.json".to_owned(),
            "--actor".to_owned(),
            "publication-candidate-test".to_owned(),
            "--idempotency-key".to_owned(),
            "unsafe-publication-stage".to_owned(),
        ],
    );
    assert!(!unsafe_stage.status.success());
    let error: Value =
        serde_json::from_slice(&unsafe_stage.stderr).expect("unsafe stage error JSON");
    assert_eq!(error["code"], "MCL_PUBLICATION_CANDIDATE_INPUT_UNSAFE");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside_report, root.path().join("linked-report.json"))
            .expect("publication report symlink");
        let linked_stage = mcl(
            &root,
            &[
                "verify",
                "stage-publication-candidate",
                "--report-file",
                "linked-report.json",
                "--retained-closure-file",
                "retained-closure.json",
                "--retained-root",
                ".",
                "--attestation-bundle-file",
                "attestation.json",
                "--actor",
                "publication-candidate-test",
                "--idempotency-key",
                "linked-publication-stage",
            ],
        );
        assert!(!linked_stage.status.success());
        let error: Value =
            serde_json::from_slice(&linked_stage.stderr).expect("linked stage error JSON");
        assert_eq!(error["code"], "MCL_PUBLICATION_CANDIDATE_INPUT_UNSAFE");
    }

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

    let unstaged = mcl(
        &root,
        &[
            "verify",
            "ingest-publication",
            "--report-artifact-hash",
            &"1".repeat(64),
            "--attestation-bundle-artifact-hash",
            &"2".repeat(64),
            "--actor",
            "publication-candidate-test",
            "--idempotency-key",
            "unstaged-publication-ingestion",
        ],
    );
    assert!(!unstaged.status.success());
    let error: Value = serde_json::from_slice(&unstaged.stderr).expect("ingestion error JSON");
    assert_eq!(error["code"], "MCL_PUBLICATION_STAGE_NOT_FOUND");

    let missing_receipt = mcl(
        &root,
        &[
            "verify",
            "promote-publication-authority",
            "--publication-receipt-hash",
            &"3".repeat(64),
            "--actor",
            "publication-candidate-test",
            "--idempotency-key",
            "missing-publication-authority-receipt",
        ],
    );
    assert!(!missing_receipt.status.success());
    let error: Value =
        serde_json::from_slice(&missing_receipt.stderr).expect("authority error JSON");
    assert_eq!(error["code"], "MCL_PUBLICATION_RECEIPT_NOT_FOUND");
}

#[test]
fn canonical_pedagogy_cli_reviews_validates_and_restarts_a_prerequisite_path() {
    let root = TempDir::new().expect("temporary root");
    assert_cli_success(&mcl(
        &root,
        &[
            "init",
            "--actor",
            "pedagogy-test",
            "--idempotency-key",
            "pedagogy-init",
        ],
    ));
    let source = create_pedagogy_source(&root);
    let claim = create_pedagogy_claim(&root, &source);
    let artifact_hash = ingest_pedagogy_content(&root);

    let explanation_payload = pedagogy_payload(
        &source,
        &claim,
        &artifact_hash,
        "explanation",
        "Explain why a repaired claim does not rewrite its disproved predecessor.",
        Vec::new(),
    );
    let preview = propose_pedagogy_unit(&root, &explanation_payload, "explanation-preview", true);
    assert_cli_success(&preview);
    let preview = parse_stdout(&preview);
    assert_eq!(preview["record"], Value::Null);

    let created = propose_pedagogy_unit(&root, &explanation_payload, "explanation-create", false);
    assert_cli_success(&created);
    let created = parse_stdout(&created);
    assert_eq!(
        created["proposed_version_hash"],
        preview["proposed_version_hash"]
    );
    let retried = propose_pedagogy_unit(&root, &explanation_payload, "explanation-create", false);
    assert_cli_success(&retried);
    assert_eq!(parse_stdout(&retried)["record"], created["record"]);
    let explanation_draft = created["record"].clone();

    let review_preview = review_pedagogy_unit(
        &root,
        explanation_draft["object_id"].as_str().expect("object ID"),
        explanation_draft["version_hash"]
            .as_str()
            .expect("draft hash"),
        "explanation-review-preview",
        true,
    );
    assert_cli_success(&review_preview);
    let reviewed = review_pedagogy_unit(
        &root,
        explanation_draft["object_id"].as_str().expect("object ID"),
        explanation_draft["version_hash"]
            .as_str()
            .expect("draft hash"),
        "explanation-review",
        false,
    );
    assert_cli_success(&reviewed);
    let explanation = parse_stdout(&reviewed)["record"].clone();
    assert_eq!(explanation["payload"]["review"]["state"], "reviewed");
    assert_eq!(explanation["payload"]["training_status"], "eligible_public");
    let review_retry = review_pedagogy_unit(
        &root,
        explanation_draft["object_id"].as_str().expect("object ID"),
        explanation_draft["version_hash"]
            .as_str()
            .expect("draft hash"),
        "explanation-review",
        false,
    );
    assert_cli_success(&review_retry);
    assert_eq!(parse_stdout(&review_retry)["record"], explanation);

    for (kind, objective) in [
        (
            "counterexample",
            "Use the retained witness to refute the original universal claim.",
        ),
        (
            "misconception",
            "Reject the misconception that a repair erases a refutation.",
        ),
        (
            "mastery_check",
            "Check that claim lineage and evidence authority remain distinct.",
        ),
    ] {
        let unit_payload =
            pedagogy_payload(&source, &claim, &artifact_hash, kind, objective, Vec::new());
        let draft = propose_pedagogy_unit(&root, &unit_payload, &format!("{kind}-create"), false);
        assert_cli_success(&draft);
        let draft = parse_stdout(&draft)["record"].clone();
        let reviewed = review_pedagogy_unit(
            &root,
            draft["object_id"].as_str().expect("unit ID"),
            draft["version_hash"].as_str().expect("draft hash"),
            &format!("{kind}-review"),
            false,
        );
        assert_cli_success(&reviewed);
        assert_eq!(
            parse_stdout(&reviewed)["record"]["payload"]["unit_kind"],
            kind
        );
    }

    let exercise_payload = pedagogy_payload(
        &source,
        &claim,
        &artifact_hash,
        "exercise",
        "Classify a repair without erasing retained counterexample evidence.",
        vec![json!({
            "object_id": explanation["object_id"],
            "version_hash": explanation["version_hash"]
        })],
    );
    let exercise_draft = propose_pedagogy_unit(&root, &exercise_payload, "exercise-create", false);
    assert_cli_success(&exercise_draft);
    let exercise_draft = parse_stdout(&exercise_draft)["record"].clone();
    let exercise = review_pedagogy_unit(
        &root,
        exercise_draft["object_id"].as_str().expect("exercise ID"),
        exercise_draft["version_hash"].as_str().expect("draft hash"),
        "exercise-review",
        false,
    );
    assert_cli_success(&exercise);
    let exercise = parse_stdout(&exercise)["record"].clone();

    let incomplete = mcl_owned(
        &root,
        &[
            "pedagogy".to_owned(),
            "validate".to_owned(),
            "--object-id".to_owned(),
            exercise["object_id"]
                .as_str()
                .expect("exercise ID")
                .to_owned(),
            "--version-hash".to_owned(),
            exercise["version_hash"]
                .as_str()
                .expect("exercise hash")
                .to_owned(),
        ],
    );
    assert!(!incomplete.status.success());
    let error: Value =
        serde_json::from_slice(&incomplete.stderr).expect("incomplete pedagogy error JSON");
    assert_eq!(error["code"], "MCL_PEDAGOGY_PREREQUISITE_LINK_MISMATCH");

    let link = mcl_owned(
        &root,
        &[
            "pedagogy".to_owned(),
            "link".to_owned(),
            "--kind".to_owned(),
            "pedagogy.hard_prerequisite".to_owned(),
            "--source-object-id".to_owned(),
            exercise["object_id"]
                .as_str()
                .expect("exercise ID")
                .to_owned(),
            "--source-version-hash".to_owned(),
            exercise["version_hash"]
                .as_str()
                .expect("exercise hash")
                .to_owned(),
            "--target-object-id".to_owned(),
            explanation["object_id"]
                .as_str()
                .expect("explanation ID")
                .to_owned(),
            "--target-version-hash".to_owned(),
            explanation["version_hash"]
                .as_str()
                .expect("explanation hash")
                .to_owned(),
            "--payload-json".to_owned(),
            "{\"rationale\":\"The exercise assumes the explanation.\"}".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "exercise-explanation-link".to_owned(),
        ],
    );
    assert_cli_success(&link);

    let validate = mcl_owned(
        &root,
        &[
            "pedagogy".to_owned(),
            "validate".to_owned(),
            "--object-id".to_owned(),
            exercise["object_id"]
                .as_str()
                .expect("exercise ID")
                .to_owned(),
            "--version-hash".to_owned(),
            exercise["version_hash"]
                .as_str()
                .expect("exercise hash")
                .to_owned(),
        ],
    );
    assert_cli_success(&validate);
    assert_eq!(parse_stdout(&validate)["valid"], true);

    let path = mcl_owned(
        &root,
        &[
            "pedagogy".to_owned(),
            "path".to_owned(),
            "--root-object-id".to_owned(),
            exercise["object_id"]
                .as_str()
                .expect("exercise ID")
                .to_owned(),
            "--root-version-hash".to_owned(),
            exercise["version_hash"]
                .as_str()
                .expect("exercise hash")
                .to_owned(),
            "--mode".to_owned(),
            "prerequisites".to_owned(),
        ],
    );
    assert_cli_success(&path);
    let path = parse_stdout(&path);
    assert_eq!(path["units"].as_array().expect("path units").len(), 2);
    assert_eq!(
        path["units"][0]["unit"]["object_id"],
        explanation["object_id"]
    );
    assert_eq!(path["units"][1]["unit"]["object_id"], exercise["object_id"]);

    let recommended_link = mcl_owned(
        &root,
        &[
            "pedagogy".to_owned(),
            "link".to_owned(),
            "--kind".to_owned(),
            "pedagogy.recommended_next".to_owned(),
            "--source-object-id".to_owned(),
            explanation["object_id"]
                .as_str()
                .expect("explanation ID")
                .to_owned(),
            "--source-version-hash".to_owned(),
            explanation["version_hash"]
                .as_str()
                .expect("explanation hash")
                .to_owned(),
            "--target-object-id".to_owned(),
            exercise["object_id"]
                .as_str()
                .expect("exercise ID")
                .to_owned(),
            "--target-version-hash".to_owned(),
            exercise["version_hash"]
                .as_str()
                .expect("exercise hash")
                .to_owned(),
            "--payload-json".to_owned(),
            "{\"rationale\":\"Practice follows the explanation.\"}".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "explanation-exercise-recommended".to_owned(),
        ],
    );
    assert_cli_success(&recommended_link);
    let recommended_path = mcl_owned(
        &root,
        &[
            "pedagogy".to_owned(),
            "path".to_owned(),
            "--root-object-id".to_owned(),
            explanation["object_id"]
                .as_str()
                .expect("explanation ID")
                .to_owned(),
            "--root-version-hash".to_owned(),
            explanation["version_hash"]
                .as_str()
                .expect("explanation hash")
                .to_owned(),
            "--mode".to_owned(),
            "recommended".to_owned(),
        ],
    );
    assert_cli_success(&recommended_path);
    let recommended_path = parse_stdout(&recommended_path);
    assert_eq!(
        recommended_path["units"][0]["unit"]["object_id"],
        explanation["object_id"]
    );
    assert_eq!(
        recommended_path["units"][1]["unit"]["object_id"],
        exercise["object_id"]
    );

    let mut changed_source = pedagogy_source_payload();
    changed_source["provenance_notes"] = json!("Source head advanced after review.");
    changed_source["redistribution_status"] = json!("restricted");
    changed_source["redaction_class"] = json!("private");
    let source_version = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "version".to_owned(),
            "--object-id".to_owned(),
            source["object_id"].as_str().expect("source ID").to_owned(),
            "--expected-head".to_owned(),
            source["version_hash"]
                .as_str()
                .expect("source hash")
                .to_owned(),
            "--payload-json".to_owned(),
            changed_source.to_string(),
            "--searchable-text".to_owned(),
            "Pilot A canonical pedagogy source".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "pedagogy-source-version".to_owned(),
        ],
    );
    assert_cli_success(&source_version);
    let source_version = parse_stdout(&source_version)["record"].clone();
    let stale = mcl_owned(
        &root,
        &[
            "pedagogy".to_owned(),
            "validate".to_owned(),
            "--object-id".to_owned(),
            explanation["object_id"]
                .as_str()
                .expect("explanation ID")
                .to_owned(),
            "--version-hash".to_owned(),
            explanation["version_hash"]
                .as_str()
                .expect("explanation hash")
                .to_owned(),
        ],
    );
    assert!(!stale.status.success());
    let error: Value = serde_json::from_slice(&stale.stderr).expect("stale error JSON");
    assert_eq!(error["code"], "MCL_PEDAGOGY_REFERENCE_STALE");

    let stale_propose_retry =
        propose_pedagogy_unit(&root, &explanation_payload, "explanation-create", false);
    assert_cli_success(&stale_propose_retry);
    assert_eq!(
        parse_stdout(&stale_propose_retry)["record"],
        explanation_draft
    );
    let stale_review_retry = review_pedagogy_unit(
        &root,
        explanation_draft["object_id"].as_str().expect("object ID"),
        explanation_draft["version_hash"]
            .as_str()
            .expect("draft hash"),
        "explanation-review",
        false,
    );
    assert_cli_success(&stale_review_retry);
    assert_eq!(parse_stdout(&stale_review_retry)["record"], explanation);

    let restricted_payload = pedagogy_payload(
        &source_version,
        &claim,
        &artifact_hash,
        "example",
        "Keep restricted grounding out of public training projections.",
        Vec::new(),
    );
    let restricted = propose_pedagogy_unit(
        &root,
        &restricted_payload,
        "restricted-example-create",
        false,
    );
    assert_cli_success(&restricted);
    let restricted = parse_stdout(&restricted)["record"].clone();
    let rejected_eligibility = review_pedagogy_unit(
        &root,
        restricted["object_id"]
            .as_str()
            .expect("restricted unit ID"),
        restricted["version_hash"]
            .as_str()
            .expect("restricted unit hash"),
        "restricted-example-review",
        false,
    );
    assert!(!rejected_eligibility.status.success());
    let error: Value =
        serde_json::from_slice(&rejected_eligibility.stderr).expect("training-policy error JSON");
    assert_eq!(error["code"], "MCL_PEDAGOGY_TRAINING_POLICY");

    let taxonomy_reference = json!({
        "object_id": source_version["object_id"],
        "version_hash": source_version["version_hash"]
    });
    let concept_payload = json!({
        "name": "Prime-number parity",
        "aliases": ["parity of primes"],
        "description": "The parity classification used by Pilot A.",
        "subject_domains": ["number theory"],
        "formal_declarations": [],
        "external_taxonomy_crosswalks": [{
            "taxonomy_name": "MSC2020",
            "external_id": "11A41",
            "source_reference": taxonomy_reference,
            "license_expression": "CC-BY-4.0"
        }],
        "pedagogy_metadata_references": [],
        "provenance_references": [taxonomy_reference]
    });
    let concept = mcl_owned(
        &root,
        &[
            "concept".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            concept_payload.to_string(),
            "--searchable-text".to_owned(),
            "Prime-number parity MSC2020".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "pedagogy-taxonomy-crosswalk".to_owned(),
        ],
    );
    assert_cli_success(&concept);
    assert_eq!(
        parse_stdout(&concept)["record"]["payload"]["external_taxonomy_crosswalks"][0]["external_id"],
        "11A41"
    );

    let mut wrong_license = concept_payload;
    wrong_license["name"] = json!("Wrongly licensed taxonomy concept");
    wrong_license["external_taxonomy_crosswalks"][0]["license_expression"] = json!("CC0-1.0");
    let rejected_crosswalk = mcl_owned(
        &root,
        &[
            "concept".to_owned(),
            "create".to_owned(),
            "--payload-json".to_owned(),
            wrong_license.to_string(),
            "--searchable-text".to_owned(),
            "Wrong taxonomy license".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "pedagogy-taxonomy-wrong-license".to_owned(),
        ],
    );
    assert!(!rejected_crosswalk.status.success());
    let error: Value =
        serde_json::from_slice(&rejected_crosswalk.stderr).expect("taxonomy-license error JSON");
    assert_eq!(error["code"], "MCL_TAXONOMY_LICENSE_MISMATCH");

    let mut revised_explanation = pedagogy_payload(
        &source_version,
        &claim,
        &artifact_hash,
        "explanation",
        "Explain the repaired claim after rebasing its exact source grounding.",
        Vec::new(),
    );
    revised_explanation["audience_track"] = json!("pilot_a_counterexample_repair_revised");
    let version_arguments = vec![
        "pedagogy".to_owned(),
        "version".to_owned(),
        "--object-id".to_owned(),
        explanation["object_id"]
            .as_str()
            .expect("explanation ID")
            .to_owned(),
        "--expected-head".to_owned(),
        explanation["version_hash"]
            .as_str()
            .expect("explanation hash")
            .to_owned(),
        "--payload-json".to_owned(),
        revised_explanation.to_string(),
        "--searchable-text".to_owned(),
        "Pilot A revised explanation".to_owned(),
        "--actor".to_owned(),
        "pedagogy-test".to_owned(),
        "--idempotency-key".to_owned(),
        "explanation-version-current-source".to_owned(),
    ];
    let revised = mcl_owned(&root, &version_arguments);
    assert_cli_success(&revised);
    let revised = parse_stdout(&revised)["record"].clone();

    let stale_link_retry = mcl_owned(
        &root,
        &[
            "pedagogy".to_owned(),
            "link".to_owned(),
            "--kind".to_owned(),
            "pedagogy.hard_prerequisite".to_owned(),
            "--source-object-id".to_owned(),
            exercise["object_id"]
                .as_str()
                .expect("exercise ID")
                .to_owned(),
            "--source-version-hash".to_owned(),
            exercise["version_hash"]
                .as_str()
                .expect("exercise hash")
                .to_owned(),
            "--target-object-id".to_owned(),
            explanation["object_id"]
                .as_str()
                .expect("explanation ID")
                .to_owned(),
            "--target-version-hash".to_owned(),
            explanation["version_hash"]
                .as_str()
                .expect("explanation hash")
                .to_owned(),
            "--payload-json".to_owned(),
            "{\"rationale\":\"The exercise assumes the explanation.\"}".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "exercise-explanation-link".to_owned(),
        ],
    );
    assert_cli_success(&stale_link_retry);
    assert_eq!(parse_stdout(&stale_link_retry), parse_stdout(&link));

    let mut twice_changed_source = pedagogy_source_payload();
    twice_changed_source["provenance_notes"] =
        json!("Source head advanced again after a successful pedagogy version.");
    twice_changed_source["redistribution_status"] = json!("restricted");
    twice_changed_source["redaction_class"] = json!("private");
    let second_source_version = mcl_owned(
        &root,
        &[
            "source".to_owned(),
            "version".to_owned(),
            "--object-id".to_owned(),
            source_version["object_id"]
                .as_str()
                .expect("source ID")
                .to_owned(),
            "--expected-head".to_owned(),
            source_version["version_hash"]
                .as_str()
                .expect("source hash")
                .to_owned(),
            "--payload-json".to_owned(),
            twice_changed_source.to_string(),
            "--searchable-text".to_owned(),
            "Pilot A canonical pedagogy source second revision".to_owned(),
            "--actor".to_owned(),
            "pedagogy-test".to_owned(),
            "--idempotency-key".to_owned(),
            "pedagogy-source-version-again".to_owned(),
        ],
    );
    assert_cli_success(&second_source_version);

    let stale_version_retry = mcl_owned(&root, &version_arguments);
    assert_cli_success(&stale_version_retry);
    assert_eq!(parse_stdout(&stale_version_retry)["record"], revised);
}
