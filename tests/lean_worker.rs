use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
                "claim_polarity": "claim",
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

    let proof_closure = audit_records
        .iter()
        .find(|evidence| evidence["payload"]["evidence_kind"] == "proof_closure_scan")
        .expect("proof-closure evidence");
    let axiom_audit = audit_records
        .iter()
        .find(|evidence| evidence["payload"]["evidence_kind"] == "axiom_audit")
        .expect("axiom-audit evidence");
    let formalization_object_id = formalization["record"]["object_id"]
        .as_str()
        .expect("formalization ID")
        .to_owned();
    let formalization_version_hash = formalization["record"]["version_hash"]
        .as_str()
        .expect("formalization hash")
        .to_owned();
    let diagnostic_evidence_id = diagnostic["evidence"]["evidence_id"]
        .as_str()
        .expect("diagnostic evidence ID")
        .to_owned();
    let proof_closure_evidence_id = proof_closure["evidence_id"]
        .as_str()
        .expect("proof-closure evidence ID")
        .to_owned();
    let axiom_audit_evidence_id = axiom_audit["evidence_id"]
        .as_str()
        .expect("axiom-audit evidence ID")
        .to_owned();
    let source_commit_sha = "a".repeat(40);
    let source_tree_sha = "b".repeat(40);
    let publication_arguments = |proof_evidence_id: &str, idempotency_key: &str, dry_run: bool| {
        let mut arguments = vec![
            "verify".to_owned(),
            "prepare-publication".to_owned(),
            "--formalization-object-id".to_owned(),
            formalization_object_id.clone(),
            "--formalization-version-hash".to_owned(),
            formalization_version_hash.clone(),
            "--outcome".to_owned(),
            "proof".to_owned(),
            "--diagnostic-evidence-id".to_owned(),
            diagnostic_evidence_id.clone(),
            "--proof-closure-evidence-id".to_owned(),
            proof_evidence_id.to_owned(),
            "--axiom-audit-evidence-id".to_owned(),
            axiom_audit_evidence_id.clone(),
            "--source-commit-sha".to_owned(),
            source_commit_sha.clone(),
            "--source-tree-sha".to_owned(),
            source_tree_sha.clone(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            idempotency_key.to_owned(),
        ];
        if dry_run {
            arguments.push("--dry-run".to_owned());
        }
        arguments
    };

    let dry_publication = run(
        root.path(),
        &publication_arguments(
            &proof_closure_evidence_id,
            "lean-ci-publication-request-dry-run",
            true,
        ),
    );
    assert_eq!(dry_publication["dry_run"], true);
    assert!(dry_publication["artifact"].is_null());
    assert_eq!(
        dry_publication["proposed_request_hash"],
        dry_publication["proposed_artifact_hash"]
    );
    let publication_artifact_hash = dry_publication["proposed_artifact_hash"]
        .as_str()
        .expect("publication request artifact hash")
        .to_owned();
    let dry_artifact_lookup = mcl(
        root.path(),
        &[
            "artifact".to_owned(),
            "get".to_owned(),
            "--artifact-hash".to_owned(),
            publication_artifact_hash.clone(),
        ],
    );
    assert!(
        !dry_artifact_lookup.status.success(),
        "dry-run unexpectedly registered the publication request"
    );
    let dry_lookup_error: Value =
        serde_json::from_slice(&dry_artifact_lookup.stderr).expect("artifact lookup error JSON");
    assert_eq!(dry_lookup_error["code"], "MCL_ARTIFACT_NOT_FOUND");

    let publication = run(
        root.path(),
        &publication_arguments(
            &proof_closure_evidence_id,
            "lean-ci-publication-request",
            false,
        ),
    );
    assert_eq!(publication["dry_run"], false);
    assert_eq!(
        publication["proposed_request_hash"],
        dry_publication["proposed_request_hash"]
    );
    assert_eq!(
        publication["proposed_artifact_hash"],
        dry_publication["proposed_artifact_hash"]
    );
    assert_eq!(publication["request"], dry_publication["request"]);
    assert_eq!(
        publication["request"]["schema_version"],
        "publication_request/1"
    );
    assert_eq!(publication["request"]["outcome"], "proof");
    assert_eq!(
        publication["request"]["subject"],
        json!({
            "object_id": formalization_object_id,
            "version_hash": formalization_version_hash
        })
    );
    assert_eq!(
        publication["request"]["diagnostic_evidence_id"],
        diagnostic_evidence_id
    );
    assert_eq!(
        publication["request"]["diagnostic_evidence_hash"],
        diagnostic["evidence"]["evidence_hash"]
    );
    assert_eq!(
        publication["request"]["proof_closure_evidence_id"],
        proof_closure_evidence_id
    );
    assert_eq!(
        publication["request"]["proof_closure_evidence_hash"],
        proof_closure["evidence_hash"]
    );
    assert_eq!(
        publication["request"]["axiom_audit_evidence_id"],
        axiom_audit_evidence_id
    );
    assert_eq!(
        publication["request"]["axiom_audit_evidence_hash"],
        axiom_audit["evidence_hash"]
    );
    assert_eq!(publication["request"]["environment_hash"], environment_hash);
    assert_eq!(
        publication["request"]["module_artifact_hash"],
        artifact_hash
    );
    assert_eq!(
        publication["request"]["declaration_name"],
        "MathOS.LeanWorker.truth"
    );
    assert_eq!(
        publication["request"]["policy_hash"],
        include_str!("../policies/lean-publication-1.sha256").trim()
    );
    assert_eq!(
        publication["request"]["source_commit_sha"],
        source_commit_sha
    );
    assert_eq!(publication["request"]["source_tree_sha"], source_tree_sha);
    assert_eq!(
        publication["artifact"]["artifact_hash"],
        publication_artifact_hash
    );
    assert_eq!(publication["artifact"]["media_type"], "application/json");
    assert_eq!(publication["artifact"]["creation_source"], "generated");
    assert_eq!(publication["artifact"]["restriction"], "private");
    assert!(publication["artifact"]["license_expression"].is_null());
    assert_eq!(
        publication["artifact"]["semantic_metadata"]["artifact_role"],
        "publication_request"
    );
    assert_eq!(
        publication["artifact"]["semantic_metadata"]["request_hash"],
        publication["proposed_request_hash"]
    );
    assert_eq!(
        publication["artifact"]["semantic_metadata"]["formalization_object_id"],
        publication["request"]["subject"]["object_id"]
    );
    assert_eq!(
        publication["artifact"]["semantic_metadata"]["formalization_version_hash"],
        publication["request"]["subject"]["version_hash"]
    );
    assert_eq!(
        publication["artifact"]["semantic_metadata"]["source_commit_sha"],
        publication["request"]["source_commit_sha"]
    );
    assert_eq!(
        publication["artifact"]["semantic_metadata"]["source_tree_sha"],
        publication["request"]["source_tree_sha"]
    );

    let request_artifact_path = root
        .path()
        .join(".mcl")
        .join("artifacts")
        .join("sha256")
        .join(&publication_artifact_hash[..2])
        .join(&publication_artifact_hash[2..4])
        .join(&publication_artifact_hash);
    let request_bytes = fs::read(request_artifact_path).expect("publication request bytes");
    assert_eq!(
        format!("{:x}", Sha256::digest(&request_bytes)),
        publication_artifact_hash
    );
    let stored_request: Value =
        serde_json::from_slice(&request_bytes).expect("publication request JSON");
    assert_eq!(stored_request, publication["request"]);
    assert!(stored_request.get("authoritative").is_none());
    assert!(!String::from_utf8_lossy(&request_bytes).contains("\"authoritative\""));

    let publication_retry = run(
        root.path(),
        &publication_arguments(
            &proof_closure_evidence_id,
            "lean-ci-publication-request",
            false,
        ),
    );
    assert_eq!(publication_retry, publication);

    let wrong_kind = mcl(
        root.path(),
        &publication_arguments(
            &diagnostic_evidence_id,
            "lean-ci-publication-request-wrong-kind",
            false,
        ),
    );
    assert!(
        !wrong_kind.status.success(),
        "diagnostic evidence was accepted as proof-closure evidence"
    );
    let wrong_kind_error: Value =
        serde_json::from_slice(&wrong_kind.stderr).expect("wrong-kind error JSON");
    assert_eq!(wrong_kind_error["code"], "MCL_PUBLICATION_EVIDENCE_INVALID");

    let mut mismatched_outcome_arguments = publication_arguments(
        &proof_closure_evidence_id,
        "lean-ci-publication-request-mismatched-outcome",
        false,
    );
    let mismatched_outcome_index = mismatched_outcome_arguments
        .iter()
        .position(|argument| argument == "--outcome")
        .expect("outcome argument")
        + 1;
    mismatched_outcome_arguments[mismatched_outcome_index] = "refutation".to_owned();
    let mismatched_outcome = mcl(root.path(), &mismatched_outcome_arguments);
    assert!(!mismatched_outcome.status.success());
    let mismatched_outcome_error: Value = serde_json::from_slice(&mismatched_outcome.stderr)
        .expect("mismatched publication outcome error JSON");
    assert_eq!(
        mismatched_outcome_error["code"],
        "MCL_PUBLICATION_OUTCOME_MISMATCH"
    );

    let mut invalid_outcome_arguments = publication_arguments(
        &proof_closure_evidence_id,
        "lean-ci-publication-request-invalid-outcome",
        false,
    );
    let outcome_index = invalid_outcome_arguments
        .iter()
        .position(|argument| argument == "--outcome")
        .expect("outcome argument")
        + 1;
    invalid_outcome_arguments[outcome_index] = "proved".to_owned();
    let invalid_outcome = mcl(root.path(), &invalid_outcome_arguments);
    assert!(!invalid_outcome.status.success());
    let invalid_outcome_error: Value = serde_json::from_slice(&invalid_outcome.stderr)
        .expect("invalid publication outcome error JSON");
    assert_eq!(
        invalid_outcome_error["code"],
        "MCL_PUBLICATION_OUTCOME_INVALID"
    );

    let mut missing_evidence_arguments = publication_arguments(
        &proof_closure_evidence_id,
        "lean-ci-publication-request-missing-evidence",
        false,
    );
    let diagnostic_index = missing_evidence_arguments
        .iter()
        .position(|argument| argument == "--diagnostic-evidence-id")
        .expect("diagnostic evidence argument")
        + 1;
    missing_evidence_arguments[diagnostic_index] =
        "01900000-0000-7000-8000-000000000000".to_owned();
    let missing_evidence = mcl(root.path(), &missing_evidence_arguments);
    assert!(!missing_evidence.status.success());
    let missing_evidence_error: Value = serde_json::from_slice(&missing_evidence.stderr)
        .expect("missing publication evidence error JSON");
    assert_eq!(missing_evidence_error["code"], "MCL_EVIDENCE_NOT_FOUND");

    let audit_report_hash = proof_closure["payload"]["artifact_hashes"]
        .as_array()
        .expect("proof-closure artifacts")
        .iter()
        .find_map(|hash| {
            let hash = hash.as_str().expect("proof-closure artifact hash");
            let artifact = run(
                root.path(),
                &[
                    "artifact".to_owned(),
                    "get".to_owned(),
                    "--artifact-hash".to_owned(),
                    hash.to_owned(),
                ],
            );
            (artifact["semantic_metadata"]["artifact_role"] == "audit_report")
                .then(|| hash.to_owned())
        })
        .expect("controlled audit report artifact");
    let audit_report_path = root
        .path()
        .join(".mcl")
        .join("artifacts")
        .join("sha256")
        .join(&audit_report_hash[..2])
        .join(&audit_report_hash[2..4])
        .join(&audit_report_hash);
    fs::write(audit_report_path, b"corrupted retained audit evidence")
        .expect("audit evidence corruption fixture writes");
    let corrupted_evidence = mcl(
        root.path(),
        &publication_arguments(
            &proof_closure_evidence_id,
            "lean-ci-publication-request-corrupt-evidence",
            false,
        ),
    );
    assert!(!corrupted_evidence.status.success());
    let corrupted_evidence_error: Value = serde_json::from_slice(&corrupted_evidence.stderr)
        .expect("corrupted publication evidence error JSON");
    assert_eq!(
        corrupted_evidence_error["code"],
        "MCL_ARTIFACT_INTEGRITY_FAILED"
    );

    let mut revised_formalization = formalization["record"]["payload"].clone();
    revised_formalization["formalization_notes"] =
        json!("superseding exact formalization version for stale-publication testing");
    let successor = run(
        root.path(),
        &[
            "formalization".to_owned(),
            "version".to_owned(),
            "--object-id".to_owned(),
            formalization_object_id.clone(),
            "--expected-head".to_owned(),
            formalization_version_hash.clone(),
            "--payload-json".to_owned(),
            revised_formalization.to_string(),
            "--searchable-text".to_owned(),
            "superseded Lean publication fixture".to_owned(),
            "--actor".to_owned(),
            "lean-ci".to_owned(),
            "--idempotency-key".to_owned(),
            "lean-ci-publication-successor".to_owned(),
        ],
    );
    assert_ne!(
        successor["record"]["version_hash"],
        formalization_version_hash
    );
    let stale_subject = mcl(
        root.path(),
        &publication_arguments(
            &proof_closure_evidence_id,
            "lean-ci-publication-request-stale-subject",
            false,
        ),
    );
    assert!(!stale_subject.status.success());
    let stale_subject_error: Value = serde_json::from_slice(&stale_subject.stderr)
        .expect("stale publication subject error JSON");
    assert_eq!(stale_subject_error["code"], "MCL_PUBLICATION_SUBJECT_STALE");

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
