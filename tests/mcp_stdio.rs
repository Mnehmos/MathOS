use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;

struct McpProcess {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl McpProcess {
    fn spawn(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mcl"))
            .arg("--root")
            .arg(root)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("MCP server process starts");
        let input = child.stdin.take().expect("MCP stdin");
        let output = BufReader::new(child.stdout.take().expect("MCP stdout"));
        Self {
            child,
            input: Some(input),
            output,
        }
    }

    fn send(&mut self, message: &Value) -> Value {
        let input = self.input.as_mut().expect("MCP stdin remains open");
        serde_json::to_writer(&mut *input, message).expect("request serializes");
        input.write_all(b"\n").expect("request terminator writes");
        input.flush().expect("MCP request flushes");
        self.read()
    }

    fn notify(&mut self, message: &Value) {
        let input = self.input.as_mut().expect("MCP stdin remains open");
        serde_json::to_writer(&mut *input, message).expect("notification serializes");
        input
            .write_all(b"\n")
            .expect("notification terminator writes");
        input.flush().expect("MCP notification flushes");
    }

    fn call(&mut self, id: i64, name: &str, arguments: Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }))
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.output
            .read_line(&mut line)
            .expect("MCP response reads");
        assert!(!line.is_empty(), "MCP server closed before responding");
        serde_json::from_str(&line).unwrap_or_else(|error| {
            panic!("MCP stdout must contain only JSON-RPC; {error}: {line:?}")
        })
    }

    fn close(mut self) {
        drop(self.input.take());
        let status = self.child.wait().expect("MCP process exits on stdin EOF");
        let mut remaining_stdout = String::new();
        self.output
            .read_to_string(&mut remaining_stdout)
            .expect("remaining stdout reads");
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .expect("MCP stderr")
            .read_to_string(&mut stderr)
            .expect("MCP stderr reads");
        assert!(status.success(), "MCP process failed: {stderr}");
        assert!(
            remaining_stdout.is_empty(),
            "protocol shutdown emitted non-protocol stdout: {remaining_stdout:?}"
        );
    }

    fn close_after_rejected_initialization(mut self) {
        drop(self.input.take());
        let status = self.child.wait().expect("rejected MCP process exits");
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .expect("MCP stderr")
            .read_to_string(&mut stderr)
            .expect("MCP stderr reads");
        assert!(!status.success(), "rejected MCP initialization succeeded");
        assert!(
            stderr.contains("MCL_MCP_RUNTIME_FAILED"),
            "rejected initialization lacked a structured process error: {stderr}"
        );
    }
}

fn mcl(root: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mcl"))
        .arg("--root")
        .arg(root)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("mcl process runs")
}

fn mcl_owned(root: &Path, arguments: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mcl"))
        .arg("--root")
        .arg(root)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("mcl process runs")
}

fn initialize_instance(root: &Path) {
    let output = mcl(
        root,
        &[
            "init",
            "--actor",
            "mcp-test",
            "--idempotency-key",
            "mcp-init",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn initialize_protocol(server: &mut McpProcess, protocol_version: &str) -> Value {
    let response = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": protocol_version,
            "capabilities": {},
            "clientInfo": {"name": "mathos-integration-test", "version": "1"}
        }
    }));
    server.notify(&json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }));
    response
}

#[test]
fn stdio_lifecycle_is_pinned_lists_only_safe_tools_and_survives_restart() {
    let root = TempDir::new().expect("temporary root");
    initialize_instance(root.path());

    let mut future_client = McpProcess::spawn(root.path());
    let rejected = future_client.send(&json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "initialize",
        "params": {
            "protocolVersion": "2026-07-28",
            "capabilities": {},
            "clientInfo": {"name": "future-client", "version": "1"}
        }
    }));
    assert_eq!(rejected["error"]["code"], -32600);
    assert_eq!(rejected["error"]["data"]["supported"], "2025-11-25");
    future_client.close_after_rejected_initialization();

    let mut server = McpProcess::spawn(root.path());

    let initialized = initialize_protocol(&mut server, "2025-11-25");
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(initialized["result"]["serverInfo"]["name"], "mathos");
    assert_eq!(initialized["result"]["serverInfo"]["version"], "0.1.0");

    let listed = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    let tools = listed["result"]["tools"].as_array().expect("tools array");
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "claim",
            "formalization",
            "query",
            "research",
            "source",
            "system",
            "verify"
        ]
    );
    assert!(tools.iter().all(|tool| tool["inputSchema"].is_object()));
    for forbidden in ["shell", "sql", "mark_proved", "sampling", "publish"] {
        assert!(!names.contains(&forbidden));
    }

    let described = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {"name": "system", "arguments": {"action": "describe"}}
    }));
    assert_eq!(
        described["result"]["structuredContent"]["product"],
        "MathOS"
    );
    assert_eq!(described["result"]["isError"], false);

    let invalid_input = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {"action": "describe", "unknown_field": true}
        }
    }));
    assert_eq!(
        invalid_input["result"]["isError"], true,
        "{invalid_input:#}"
    );

    let invalid_action = server.call(311, "system", json!({"action": "mark_proved"}));
    assert_eq!(
        invalid_action["result"]["isError"], true,
        "{invalid_action:#}"
    );

    let capabilities = server.call(312, "system", json!({"action": "capabilities"}));
    assert_eq!(
        capabilities["result"]["structuredContent"]["verify_actions"],
        json!([
            "review_fidelity",
            "fidelity_status",
            "prepare_publication",
            "ingest_publication",
            "promote_publication_authority"
        ])
    );
    assert_eq!(
        capabilities["result"]["structuredContent"]["authoritative_verification"],
        true
    );

    let caller_authored_request = server.call(
        313,
        "verify",
        json!({"action": "prepare_publication", "request": {}}),
    );
    assert_eq!(caller_authored_request["result"]["isError"], true);
    assert_eq!(
        caller_authored_request["result"]["structuredContent"]["code"],
        "MCL_MCP_FIELD_FORBIDDEN"
    );

    let incomplete_preparation =
        server.call(314, "verify", json!({"action": "prepare_publication"}));
    assert_eq!(incomplete_preparation["result"]["isError"], true);
    assert_eq!(
        incomplete_preparation["result"]["structuredContent"]["code"],
        "MCL_MCP_FIELD_REQUIRED"
    );

    let crossed_action_field = server.call(
        315,
        "verify",
        json!({
            "action": "fidelity_status",
            "source_commit_sha": "1".repeat(40)
        }),
    );
    assert_eq!(crossed_action_field["result"]["isError"], true);
    assert_eq!(
        crossed_action_field["result"]["structuredContent"]["code"],
        "MCL_MCP_FIELD_FORBIDDEN"
    );

    let invalid_outcome = server.call(
        316,
        "verify",
        json!({
            "action": "prepare_publication",
            "formalization_object_id": "01900000-0000-7000-8000-000000000000",
            "formalization_version_hash": "a".repeat(64),
            "outcome": "proved",
            "diagnostic_evidence_id": "01900000-0000-7000-8000-000000000001",
            "proof_closure_evidence_id": "01900000-0000-7000-8000-000000000002",
            "axiom_audit_evidence_id": "01900000-0000-7000-8000-000000000003",
            "source_commit_sha": "1".repeat(40),
            "source_tree_sha": "2".repeat(40),
            "actor": "mcp-test",
            "idempotency_key": "mcp-invalid-publication-outcome"
        }),
    );
    assert_eq!(invalid_outcome["result"]["isError"], true);
    assert_eq!(
        invalid_outcome["result"]["structuredContent"]["code"],
        "MCL_PUBLICATION_OUTCOME_INVALID"
    );

    let incomplete_ingestion = server.call(317, "verify", json!({"action": "ingest_publication"}));
    assert_eq!(incomplete_ingestion["result"]["isError"], true);
    assert_eq!(
        incomplete_ingestion["result"]["structuredContent"]["code"],
        "MCL_MCP_FIELD_REQUIRED"
    );

    let caller_authored_ingestion = server.call(
        318,
        "verify",
        json!({
            "action": "ingest_publication",
            "request": {"authoritative": true},
            "report_artifact_hash": "1".repeat(64),
            "attestation_bundle_artifact_hash": "2".repeat(64),
            "actor": "mcp-test",
            "idempotency_key": "mcp-caller-authored-ingestion"
        }),
    );
    assert_eq!(caller_authored_ingestion["result"]["isError"], true);
    assert_eq!(
        caller_authored_ingestion["result"]["structuredContent"]["code"],
        "MCL_MCP_FIELD_FORBIDDEN"
    );

    let unstaged_ingestion = server.call(
        319,
        "verify",
        json!({
            "action": "ingest_publication",
            "report_artifact_hash": "1".repeat(64),
            "attestation_bundle_artifact_hash": "2".repeat(64),
            "actor": "mcp-test",
            "idempotency_key": "mcp-unstaged-ingestion"
        }),
    );
    assert_eq!(unstaged_ingestion["result"]["isError"], true);
    assert_eq!(
        unstaged_ingestion["result"]["structuredContent"]["code"],
        "MCL_PUBLICATION_STAGE_NOT_FOUND"
    );

    let incomplete_authority = server.call(
        320,
        "verify",
        json!({"action": "promote_publication_authority"}),
    );
    assert_eq!(incomplete_authority["result"]["isError"], true);
    assert_eq!(
        incomplete_authority["result"]["structuredContent"]["code"],
        "MCL_MCP_FIELD_REQUIRED"
    );

    let caller_authored_authority = server.call(
        321,
        "verify",
        json!({
            "action": "promote_publication_authority",
            "publication_receipt_hash": "3".repeat(64),
            "outcome": "proof",
            "request": {"authority_class": "authoritative"},
            "actor": "mcp-test",
            "idempotency_key": "mcp-caller-authored-authority"
        }),
    );
    assert_eq!(caller_authored_authority["result"]["isError"], true);
    assert_eq!(
        caller_authored_authority["result"]["structuredContent"]["code"],
        "MCL_MCP_FIELD_FORBIDDEN"
    );

    let missing_receipt_authority = server.call(
        322,
        "verify",
        json!({
            "action": "promote_publication_authority",
            "publication_receipt_hash": "3".repeat(64),
            "actor": "mcp-test",
            "idempotency_key": "mcp-missing-receipt-authority"
        }),
    );
    assert_eq!(missing_receipt_authority["result"]["isError"], true);
    assert_eq!(
        missing_receipt_authority["result"]["structuredContent"]["code"],
        "MCL_PUBLICATION_RECEIPT_NOT_FOUND"
    );

    let still_alive = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 32,
        "method": "tools/call",
        "params": {"name": "system", "arguments": {"action": "policy"}}
    }));
    assert_eq!(
        still_alive["result"]["structuredContent"]["raw_shell"],
        false
    );

    let forbidden = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "mark_proved", "arguments": {}}
    }));
    assert!(forbidden["error"].is_object());

    server.close();

    let mut restarted = McpProcess::spawn(root.path());
    initialize_protocol(&mut restarted, "2025-11-25");
    let health = restarted.send(&json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {"name": "system", "arguments": {"action": "health"}}
    }));
    assert_eq!(health["result"]["structuredContent"]["healthy"], true);
    restarted.close();
}

#[test]
fn query_tool_matches_cli_state_and_returns_structured_application_errors() {
    let root = TempDir::new().expect("temporary root");
    initialize_instance(root.path());
    let payload = json!({
        "source_type": "user_statement",
        "title_or_label": "MCP parity fixture",
        "authors_or_origin": ["MCP integration test"],
        "canonical_locator": "local:mcp-parity",
        "acquisition_date": "2026-07-19",
        "license_expression": null,
        "redistribution_status": "unknown",
        "content_hash": null,
        "citation_metadata": {},
        "redaction_class": "private",
        "provenance_notes": "MCP and CLI share application state",
        "original_text": "Every prime number is odd."
    });
    let payload = serde_json::to_string(&payload).expect("source payload serializes");
    let created = mcl(
        root.path(),
        &[
            "source",
            "create",
            "--payload-json",
            &payload,
            "--searchable-text",
            "mcp parity exact marker",
            "--actor",
            "mcp-test",
            "--idempotency-key",
            "mcp-source-create",
        ],
    );
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created: Value = serde_json::from_slice(&created.stdout).expect("created JSON");
    let cli_search = mcl(
        root.path(),
        &["search", "--query", "mcp parity", "--limit", "20"],
    );
    let cli_search: Value = serde_json::from_slice(&cli_search.stdout).expect("search JSON");

    let mut server = McpProcess::spawn(root.path());
    initialize_protocol(&mut server, "2025-11-25");
    let searched = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "query",
            "arguments": {"action": "search", "query": "mcp parity", "limit": 20}
        }
    }));
    assert_eq!(searched["result"]["structuredContent"], cli_search);

    let object_id = created["record"]["object_id"]
        .as_str()
        .expect("created object ID");
    let loaded = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "query",
            "arguments": {"action": "get", "object_id": object_id}
        }
    }));
    assert_eq!(
        loaded["result"]["structuredContent"]["version_hash"],
        created["record"]["version_hash"]
    );

    let missing = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "query",
            "arguments": {"action": "get", "object_id": "01900000-0000-7000-8000-000000000000"}
        }
    }));
    assert_eq!(missing["result"]["isError"], true);
    assert_eq!(
        missing["result"]["structuredContent"]["code"],
        "MCL_RECORD_NOT_FOUND"
    );
    assert!(missing["result"]["structuredContent"]["corrective_action"].is_string());

    let invalid_limit = server.send(&json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "query",
            "arguments": {"action": "search", "query": "anything", "limit": 0}
        }
    }));
    assert_eq!(
        invalid_limit["result"]["structuredContent"]["code"],
        "MCL_QUERY_LIMIT_INVALID"
    );
    server.close();
}

#[test]
fn controlled_mcp_mutations_preserve_idempotency_cas_and_non_authoritative_runs() {
    let root = TempDir::new().expect("temporary root");
    initialize_instance(root.path());
    let registered_environment = mcl(
        root.path(),
        &[
            "environment",
            "register",
            "--manifest-json",
            include_str!("../fixtures/environment/lean-4.32-local.json"),
            "--actor",
            "mcp-test",
            "--idempotency-key",
            "mcp-environment-register",
        ],
    );
    assert!(
        registered_environment.status.success(),
        "{}",
        String::from_utf8_lossy(&registered_environment.stderr)
    );
    let environment_hash = include_str!("../fixtures/environment/lean-4.32-local.sha256").trim();
    let module_path = root.path().join("McpFixture.lean");
    fs::write(&module_path, b"theorem primeParity : True := by trivial\n")
        .expect("Lean fixture writes");
    let artifact_metadata = json!({
        "schema_version": "artifact_metadata/1",
        "media_type": "text/x-lean",
        "creation_source": "user_ingest",
        "license_expression": "PolyForm-Noncommercial-1.0.0",
        "restriction": "restricted",
        "semantic_metadata": {"declaration_name": "MathOS.Pilot.primeParity"}
    });
    let ingested = mcl_owned(
        root.path(),
        &[
            "artifact".to_owned(),
            "ingest".to_owned(),
            "--input-file".to_owned(),
            module_path.to_string_lossy().into_owned(),
            "--metadata-json".to_owned(),
            artifact_metadata.to_string(),
            "--actor".to_owned(),
            "mcp-test".to_owned(),
            "--idempotency-key".to_owned(),
            "mcp-artifact-register".to_owned(),
        ],
    );
    assert!(
        ingested.status.success(),
        "{}",
        String::from_utf8_lossy(&ingested.stderr)
    );
    let ingested: Value = serde_json::from_slice(&ingested.stdout).expect("artifact ingest JSON");
    let module_artifact_hash = ingested["proposed_artifact_hash"]
        .as_str()
        .expect("artifact hash");
    let mut server = McpProcess::spawn(root.path());
    initialize_protocol(&mut server, "2025-11-25");

    let source_payload = json!({
        "source_type": "user_statement",
        "title_or_label": "Controlled MCP mutation",
        "authors_or_origin": ["MCP integration test"],
        "canonical_locator": "local:controlled-mcp-mutation",
        "acquisition_date": "2026-07-19",
        "license_expression": null,
        "redistribution_status": "unknown",
        "content_hash": null,
        "citation_metadata": {},
        "redaction_class": "private",
        "provenance_notes": "Exercises MCP mutation controls",
        "original_text": "Every prime number is odd."
    });
    let source_request = json!({
        "action": "propose",
        "payload": source_payload,
        "searchable_text": "controlled mcp mutation marker",
        "actor": "mcp-test",
        "idempotency_key": "mcp-controlled-source",
        "dry_run": true
    });
    let preview = server.call(10, "source", source_request.clone());
    assert_eq!(preview["result"]["structuredContent"]["dry_run"], true);
    assert_eq!(
        preview["result"]["structuredContent"]["record"],
        Value::Null
    );
    let absent = server.call(
        11,
        "query",
        json!({"action": "search", "query": "controlled mcp mutation marker"}),
    );
    assert_eq!(absent["result"]["structuredContent"], json!([]));

    let mut committed_request = source_request;
    committed_request["dry_run"] = json!(false);
    let created = server.call(12, "source", committed_request.clone());
    assert_eq!(created["result"]["isError"], false);
    let retried = server.call(13, "source", committed_request);
    assert_eq!(
        retried["result"]["structuredContent"]["record"],
        created["result"]["structuredContent"]["record"]
    );
    let source = &created["result"]["structuredContent"]["record"];
    let source_id = source["object_id"].as_str().expect("source object ID");
    let source_hash = source["version_hash"].as_str().expect("source hash");

    let cli_loaded = mcl(root.path(), &["source", "get", "--object-id", source_id]);
    assert!(cli_loaded.status.success());
    let cli_loaded: Value = serde_json::from_slice(&cli_loaded.stdout).expect("CLI source JSON");
    assert_eq!(cli_loaded, *source);

    let claim_payload = json!({
        "source_reference": {"object_id": source_id, "version_hash": source_hash},
        "normalized_informal_statement": "Every prime number is odd.",
        "claim_kind": "universal",
        "logical_shape": "forall p, prime p implies odd p",
        "assumptions": [],
        "variables": [{"symbol": "p", "domain": "natural numbers", "notes": "prime"}],
        "concept_links": [],
        "source_citations": [],
        "ambiguity_notes": []
    });
    let claim_created = server.call(
        14,
        "claim",
        json!({
            "action": "propose",
            "payload": claim_payload,
            "searchable_text": "prime parity claim",
            "actor": "mcp-test",
            "idempotency_key": "mcp-controlled-claim"
        }),
    );
    assert_eq!(claim_created["result"]["isError"], false);
    let claim = &claim_created["result"]["structuredContent"]["record"];
    let claim_id = claim["object_id"].as_str().expect("claim object ID");
    let claim_hash = claim["version_hash"].as_str().expect("claim hash");

    let formalization_payload = json!({
        "claim_version": {"object_id": claim_id, "version_hash": claim_hash},
        "formal_system": "lean4",
        "environment_hash": environment_hash,
        "module_artifact_hash": module_artifact_hash,
        "declaration_name": "MathOS.Pilot.primeParity",
        "exact_theorem_type": "forall p : Nat, Nat.Prime p -> Odd p",
        "declaration_hash": "c".repeat(64),
        "import_manifest": ["Mathlib"],
        "formalization_notes": "An unreviewed interpretation, not a verdict.",
        "fidelity_evidence_references": [],
        "verification_evidence_references": []
    });
    let formalization = server.call(
        15,
        "formalization",
        json!({
            "action": "propose",
            "payload": formalization_payload,
            "searchable_text": "unreviewed prime parity formalization",
            "actor": "mcp-test",
            "idempotency_key": "mcp-controlled-formalization"
        }),
    );
    assert_eq!(formalization["result"]["isError"], false);
    let formalization_record = &formalization["result"]["structuredContent"]["record"];
    let formalization_id = formalization_record["object_id"]
        .as_str()
        .expect("formalization object ID");
    let formalization_hash = formalization_record["version_hash"]
        .as_str()
        .expect("formalization version hash");

    let prohibited = server.call(
        16,
        "formalization",
        json!({
            "action": "propose",
            "payload": {
                "claim_version": {"object_id": claim_id, "version_hash": claim_hash},
                "formal_system": "lean4",
                "environment_hash": environment_hash,
                "module_artifact_hash": module_artifact_hash,
                "declaration_name": "MathOS.Pilot.falseAuthority",
                "exact_theorem_type": "True",
                "declaration_hash": "d".repeat(64),
                "import_manifest": ["Mathlib"],
                "formalization_notes": "must fail",
                "fidelity_evidence_references": [],
                "verification_evidence_references": [],
                "proved": true
            },
            "searchable_text": "forbidden authority",
            "actor": "mcp-test",
            "idempotency_key": "mcp-forbidden-authority"
        }),
    );
    assert_eq!(prohibited["result"]["isError"], true);
    assert_eq!(
        prohibited["result"]["structuredContent"]["code"],
        "MCL_SCHEMA_VALIDATION_FAILED"
    );

    let started_request = json!({
        "action": "start",
        "run_kind": "prove",
        "budget": {"max_steps": 4},
        "actor": "mcp-test",
        "idempotency_key": "mcp-controlled-run"
    });
    let started = server.call(17, "research", started_request.clone());
    assert_eq!(started["result"]["isError"], false);
    let run = &started["result"]["structuredContent"]["run"];
    let run_id = run["run_id"].as_str().expect("run ID");
    let origin_head = run["event_head_hash"].as_str().expect("origin head");
    let retried_start = server.call(18, "research", started_request);
    assert_eq!(retried_start["result"]["structuredContent"]["run"], *run);

    let submit_request = json!({
        "action": "submit",
        "run_id": run_id,
        "expected_head": origin_head,
        "event_kind": "action_submitted",
        "payload": {"action": "give_up", "reason": "No unverified proof should be promoted."},
        "actor": "mcp-test",
        "idempotency_key": "mcp-controlled-event",
        "dry_run": true
    });
    let event_preview = server.call(19, "research", submit_request.clone());
    assert_eq!(
        event_preview["result"]["structuredContent"]["dry_run"],
        true
    );
    assert_eq!(
        event_preview["result"]["structuredContent"]["event"],
        Value::Null
    );

    let mut committed_submit = submit_request;
    committed_submit["dry_run"] = json!(false);
    let submitted = server.call(20, "research", committed_submit.clone());
    assert_eq!(submitted["result"]["isError"], false);
    let submitted_retry = server.call(21, "research", committed_submit);
    assert_eq!(
        submitted_retry["result"]["structuredContent"]["event"],
        submitted["result"]["structuredContent"]["event"]
    );

    let observed = server.call(
        22,
        "research",
        json!({"action": "observe", "run_id": run_id}),
    );
    assert_eq!(
        observed["result"]["structuredContent"]["events"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        observed["result"]["structuredContent"]["run"]["event_count"],
        2
    );

    let stale = server.call(
        23,
        "research",
        json!({
            "action": "submit",
            "run_id": run_id,
            "expected_head": origin_head,
            "event_kind": "diagnostic",
            "payload": {"message": "stale writer"},
            "actor": "mcp-test",
            "idempotency_key": "mcp-stale-event"
        }),
    );
    assert_eq!(stale["result"]["isError"], true);
    assert_eq!(
        stale["result"]["structuredContent"]["code"],
        "MCL_RUN_EVENT_CONFLICT"
    );

    let unreviewed = server.call(
        24,
        "verify",
        json!({
            "action": "fidelity_status",
            "formalization_object_id": formalization_id,
            "formalization_version_hash": formalization_hash
        }),
    );
    assert_eq!(unreviewed["result"]["isError"], false);
    assert_eq!(
        unreviewed["result"]["structuredContent"]["status"],
        "unreviewed"
    );

    let review_request = json!({
        "schema_version": "fidelity_review_request/1",
        "source": {"object_id": source_id, "version_hash": source_hash},
        "claim": {"object_id": claim_id, "version_hash": claim_hash},
        "formalization": {
            "object_id": formalization_id,
            "version_hash": formalization_hash
        },
        "review_level": "mathematical_statement",
        "verdict": "verified",
        "reviewer_identity": "independent-reviewer",
        "findings": ["The quantifier, domain, predicate, and conclusion match the recorded source claim."],
        "ambiguity_disposition": "no_ambiguity",
        "definition_mappings": [],
        "supporting_artifact_hashes": [],
        "producing_run_id": run_id,
        "supersedes_evidence_id": null
    });
    let mut same_author_request = review_request.clone();
    same_author_request["reviewer_identity"] = json!("mcp-test");
    let same_author = server.call(
        25,
        "verify",
        json!({
            "action": "review_fidelity",
            "request": same_author_request,
            "actor": "mcp-test",
            "idempotency_key": "fidelity-same-author"
        }),
    );
    assert_eq!(same_author["result"]["isError"], true);
    assert_eq!(
        same_author["result"]["structuredContent"]["code"],
        "MCL_FIDELITY_REPORT_INVALID"
    );
    let mut missing_artifact_request = review_request.clone();
    missing_artifact_request["supporting_artifact_hashes"] = json!(["f".repeat(64)]);
    let missing_artifact = server.call(
        31,
        "verify",
        json!({
            "action": "review_fidelity",
            "request": missing_artifact_request,
            "actor": "independent-reviewer",
            "idempotency_key": "fidelity-missing-artifact"
        }),
    );
    assert_eq!(missing_artifact["result"]["isError"], true);
    assert_eq!(
        missing_artifact["result"]["structuredContent"]["code"],
        "MCL_ARTIFACT_NOT_FOUND"
    );

    let other_source = server.call(
        32,
        "source",
        json!({
            "action": "propose",
            "payload": {
                "source_type": "user_statement",
                "title_or_label": "Unrelated source",
                "authors_or_origin": ["Adversarial fixture"],
                "canonical_locator": "local:unrelated-source",
                "acquisition_date": "2026-07-19",
                "license_expression": null,
                "redistribution_status": "unknown",
                "content_hash": null,
                "citation_metadata": {},
                "redaction_class": "private",
                "provenance_notes": "Must not be substituted into another claim lineage",
                "original_text": "There are infinitely many primes."
            },
            "searchable_text": "unrelated fidelity source",
            "actor": "mcp-test",
            "idempotency_key": "fidelity-unrelated-source"
        }),
    );
    let other_source_record = &other_source["result"]["structuredContent"]["record"];
    let mut wrong_lineage_request = review_request.clone();
    wrong_lineage_request["source"] = json!({
        "object_id": other_source_record["object_id"],
        "version_hash": other_source_record["version_hash"]
    });
    let wrong_lineage = server.call(
        33,
        "verify",
        json!({
            "action": "review_fidelity",
            "request": wrong_lineage_request,
            "actor": "independent-reviewer",
            "idempotency_key": "fidelity-wrong-lineage"
        }),
    );
    assert_eq!(wrong_lineage["result"]["isError"], true);
    assert_eq!(
        wrong_lineage["result"]["structuredContent"]["code"],
        "MCL_FIDELITY_LINEAGE_MISMATCH"
    );

    let ambiguous_claim = server.call(
        34,
        "claim",
        json!({
            "action": "propose",
            "payload": {
                "source_reference": {"object_id": source_id, "version_hash": source_hash},
                "normalized_informal_statement": "Every prime number is odd.",
                "claim_kind": "universal",
                "logical_shape": "forall p, prime p implies odd p",
                "assumptions": [],
                "variables": [{"symbol": "p", "domain": "unspecified integer or natural domain", "notes": "ambiguous source domain"}],
                "concept_links": [],
                "source_citations": [],
                "ambiguity_notes": ["The source does not explicitly fix the ambient number domain."]
            },
            "searchable_text": "ambiguous prime parity claim",
            "actor": "mcp-test",
            "idempotency_key": "fidelity-ambiguous-claim"
        }),
    );
    let ambiguous_claim_record = &ambiguous_claim["result"]["structuredContent"]["record"];
    let ambiguous_formalization = server.call(
        35,
        "formalization",
        json!({
            "action": "propose",
            "payload": {
                "claim_version": {
                    "object_id": ambiguous_claim_record["object_id"],
                    "version_hash": ambiguous_claim_record["version_hash"]
                },
                "formal_system": "lean4",
                "environment_hash": environment_hash,
                "module_artifact_hash": module_artifact_hash,
                "declaration_name": "MathOS.Pilot.ambiguousPrimeParity",
                "exact_theorem_type": "forall p : Nat, Nat.Prime p -> Odd p",
                "declaration_hash": "e".repeat(64),
                "import_manifest": ["Mathlib"],
                "formalization_notes": "This variant cannot erase the recorded ambiguity.",
                "fidelity_evidence_references": [],
                "verification_evidence_references": []
            },
            "searchable_text": "ambiguous prime parity formalization",
            "actor": "mcp-test",
            "idempotency_key": "fidelity-ambiguous-formalization"
        }),
    );
    let ambiguous_formalization_record =
        &ambiguous_formalization["result"]["structuredContent"]["record"];
    let mut erased_ambiguity_request = review_request.clone();
    erased_ambiguity_request["claim"] = json!({
        "object_id": ambiguous_claim_record["object_id"],
        "version_hash": ambiguous_claim_record["version_hash"]
    });
    erased_ambiguity_request["formalization"] = json!({
        "object_id": ambiguous_formalization_record["object_id"],
        "version_hash": ambiguous_formalization_record["version_hash"]
    });
    let erased_ambiguity = server.call(
        36,
        "verify",
        json!({
            "action": "review_fidelity",
            "request": erased_ambiguity_request,
            "actor": "independent-reviewer",
            "idempotency_key": "fidelity-erased-ambiguity"
        }),
    );
    assert_eq!(erased_ambiguity["result"]["isError"], true);
    assert_eq!(
        erased_ambiguity["result"]["structuredContent"]["code"],
        "MCL_FIDELITY_AMBIGUITY_INVALID"
    );
    let review_arguments = |dry_run: bool, key: &str| {
        let mut arguments = vec![
            "verify".to_owned(),
            "review-fidelity".to_owned(),
            "--request-json".to_owned(),
            review_request.to_string(),
            "--actor".to_owned(),
            "independent-reviewer".to_owned(),
            "--idempotency-key".to_owned(),
            key.to_owned(),
        ];
        if dry_run {
            arguments.push("--dry-run".to_owned());
        }
        arguments
    };
    let review_preview = mcl_owned(root.path(), &review_arguments(true, "fidelity-preview"));
    assert!(
        review_preview.status.success(),
        "{}",
        String::from_utf8_lossy(&review_preview.stderr)
    );
    let preview: Value = serde_json::from_slice(&review_preview.stdout).expect("review preview");
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["evidence"], Value::Null);

    let reviewed_output = mcl_owned(root.path(), &review_arguments(false, "fidelity-review"));
    assert!(
        reviewed_output.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed_output.stderr)
    );
    let reviewed: Value = serde_json::from_slice(&reviewed_output.stdout).expect("review output");
    assert_eq!(
        reviewed["evidence"]["payload"]["authority_class"],
        "reviewed"
    );
    assert_eq!(
        reviewed["evidence"]["payload"]["evidence_kind"],
        "statement_fidelity_review"
    );
    let first_evidence_id = reviewed["evidence"]["evidence_id"]
        .as_str()
        .expect("first fidelity evidence ID");
    let retried = mcl_owned(root.path(), &review_arguments(false, "fidelity-review"));
    assert!(retried.status.success());
    let retried: Value = serde_json::from_slice(&retried.stdout).expect("review retry");
    assert_eq!(retried["evidence"], reviewed["evidence"]);

    let verified = server.call(
        26,
        "verify",
        json!({
            "action": "fidelity_status",
            "formalization_object_id": formalization_id,
            "formalization_version_hash": formalization_hash
        }),
    );
    assert_eq!(verified["result"]["isError"], false);
    assert_eq!(
        verified["result"]["structuredContent"]["status"],
        "verified"
    );
    assert_eq!(
        verified["result"]["structuredContent"]["head_evidence_id"],
        first_evidence_id
    );

    let stale_review = server.call(
        27,
        "verify",
        json!({
            "action": "review_fidelity",
            "request": review_request,
            "actor": "independent-reviewer",
            "idempotency_key": "fidelity-stale-review"
        }),
    );
    assert_eq!(stale_review["result"]["isError"], true);
    assert_eq!(
        stale_review["result"]["structuredContent"]["code"],
        "MCL_FIDELITY_REVIEW_CONFLICT"
    );

    let replacement_request = json!({
        "schema_version": "fidelity_review_request/1",
        "source": {"object_id": source_id, "version_hash": source_hash},
        "claim": {"object_id": claim_id, "version_hash": claim_hash},
        "formalization": {
            "object_id": formalization_id,
            "version_hash": formalization_hash
        },
        "review_level": "expert_domain_review",
        "verdict": "rejected",
        "reviewer_identity": "second-independent-reviewer",
        "findings": ["Independent re-review rejects the claimed source correspondence."],
        "ambiguity_disposition": "no_ambiguity",
        "definition_mappings": [],
        "supporting_artifact_hashes": [],
        "producing_run_id": run_id,
        "supersedes_evidence_id": first_evidence_id
    });
    let replaced = server.call(
        28,
        "verify",
        json!({
            "action": "review_fidelity",
            "request": replacement_request,
            "actor": "second-independent-reviewer",
            "idempotency_key": "fidelity-replacement"
        }),
    );
    assert_eq!(replaced["result"]["isError"], false);
    let replacement_evidence_id =
        replaced["result"]["structuredContent"]["evidence"]["evidence_id"]
            .as_str()
            .expect("replacement evidence ID");

    let rejected = mcl_owned(
        root.path(),
        &[
            "verify".to_owned(),
            "fidelity-status".to_owned(),
            "--formalization-object-id".to_owned(),
            formalization_id.to_owned(),
            "--formalization-version-hash".to_owned(),
            formalization_hash.to_owned(),
        ],
    );
    assert!(
        rejected.status.success(),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    let rejected: Value = serde_json::from_slice(&rejected.stdout).expect("rejected status");
    assert_eq!(rejected["status"], "rejected");
    assert_eq!(rejected["head_evidence_id"], replacement_evidence_id);
    assert_eq!(rejected["history"].as_array().map(Vec::len), Some(2));
    assert_eq!(rejected["history"][0]["status"], "rejected");
    assert_eq!(rejected["history"][1]["status"], "superseded");

    let report_hash = rejected["history"][0]["report_artifact_hash"]
        .as_str()
        .expect("head report artifact hash");
    let report_path = root
        .path()
        .join(".mcl/artifacts/sha256")
        .join(&report_hash[0..2])
        .join(&report_hash[2..4])
        .join(report_hash);
    fs::write(report_path, b"corrupted review report").expect("corrupt review report fixture");
    let corrupted = server.call(
        29,
        "verify",
        json!({
            "action": "fidelity_status",
            "formalization_object_id": formalization_id,
            "formalization_version_hash": formalization_hash
        }),
    );
    assert_eq!(corrupted["result"]["isError"], true);
    assert_eq!(
        corrupted["result"]["structuredContent"]["code"],
        "MCL_ARTIFACT_INTEGRITY_FAILED"
    );
    let mut post_corruption_request = replacement_request.clone();
    post_corruption_request["reviewer_identity"] = json!("third-independent-reviewer");
    post_corruption_request["findings"] =
        json!(["A corrupted predecessor must block further review mutation."]);
    let mutation_after_corruption = server.call(
        30,
        "verify",
        json!({
            "action": "review_fidelity",
            "request": post_corruption_request,
            "actor": "third-independent-reviewer",
            "idempotency_key": "fidelity-after-corruption"
        }),
    );
    assert_eq!(mutation_after_corruption["result"]["isError"], true);
    assert_eq!(
        mutation_after_corruption["result"]["structuredContent"]["code"],
        "MCL_ARTIFACT_INTEGRITY_FAILED"
    );

    server.close();
}
