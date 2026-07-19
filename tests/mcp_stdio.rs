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
    assert_eq!(names, ["query", "system"]);
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
