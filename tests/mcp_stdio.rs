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
            "system"
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
        "module_artifact_hash": "b".repeat(64),
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

    let prohibited = server.call(
        16,
        "formalization",
        json!({
            "action": "propose",
            "payload": {
                "claim_version": {"object_id": claim_id, "version_hash": claim_hash},
                "formal_system": "lean4",
                "environment_hash": environment_hash,
                "module_artifact_hash": "b".repeat(64),
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

    server.close();
}
