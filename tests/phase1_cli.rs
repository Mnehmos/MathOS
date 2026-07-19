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
    assert_eq!(value["migration_version"], 7);
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
