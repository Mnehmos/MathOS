import json
from contextlib import contextmanager
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from tests.helpers import load_fixture


class McpTests(unittest.TestCase):
    @contextmanager
    def server(self, database: Path):
        process = subprocess.Popen(
            [sys.executable, "-m", "mathos.mcp_server", "--db", str(database)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            yield process
        finally:
            process.terminate()
            process.wait(timeout=5)
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None:
                    stream.close()

    def request(self, process: subprocess.Popen, message: dict) -> dict:
        assert process.stdin is not None
        assert process.stdout is not None
        process.stdin.write(json.dumps(message, separators=(",", ":")) + "\n")
        process.stdin.flush()
        line = process.stdout.readline()
        self.assertTrue(line)
        return json.loads(line)

    def test_stdio_handshake_and_tool_listing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = Path(directory) / "mcp.db"
            with self.server(db_path) as process:
                denied = self.request(
                    process,
                    {"jsonrpc": "2.0", "id": 0, "method": "tools/list", "params": {}},
                )
                self.assertEqual(denied["error"]["code"], -32002)

                initialized = self.request(
                    process,
                    {
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "initialize",
                        "params": {
                            "protocolVersion": "2025-11-25",
                            "capabilities": {},
                            "clientInfo": {"name": "mathos-test", "version": "1"},
                        },
                    },
                )
                self.assertEqual(
                    initialized["result"]["protocolVersion"], "2025-11-25"
                )

                assert process.stdin is not None
                process.stdin.write(
                    '{"jsonrpc":"2.0","method":"notifications/initialized"}\n'
                )
                process.stdin.flush()

                listed = self.request(
                    process,
                    {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
                )
                names = {tool["name"] for tool in listed["result"]["tools"]}
                self.assertEqual(
                    names,
                    {
                        "mathos_submit_claim",
                        "mathos_run_claim",
                        "mathos_get_claim",
                        "mathos_export_rl",
                        "mathos_verify_ledger",
                        "mathos_validate_rl",
                    },
                )

                fixture = load_fixture("proved")
                submitted = self.request(
                    process,
                    {
                        "jsonrpc": "2.0",
                        "id": 3,
                        "method": "tools/call",
                        "params": {
                            "name": "mathos_submit_claim",
                            "arguments": {
                                "informal_statement": fixture["informal_statement"],
                                "formal_spec": fixture["formal_spec"],
                            },
                        },
                    },
                )
                claim_id = submitted["result"]["structuredContent"]["claim_id"]
                completed = self.request(
                    process,
                    {
                        "jsonrpc": "2.0",
                        "id": 4,
                        "method": "tools/call",
                        "params": {
                            "name": "mathos_run_claim",
                            "arguments": {"claim_id": claim_id, "max_assignments": 16},
                        },
                    },
                )
                self.assertEqual(
                    completed["result"]["structuredContent"]["claim"]["status"],
                    "verified_proved",
                )
                exported = self.request(
                    process,
                    {
                        "jsonrpc": "2.0",
                        "id": 5,
                        "method": "tools/call",
                        "params": {
                            "name": "mathos_export_rl",
                            "arguments": {"claim_id": claim_id},
                        },
                    },
                )["result"]["structuredContent"]
                validated = self.request(
                    process,
                    {
                        "jsonrpc": "2.0",
                        "id": 6,
                        "method": "tools/call",
                        "params": {
                            "name": "mathos_validate_rl",
                            "arguments": {"trajectory": exported},
                        },
                    },
                )
                self.assertTrue(validated["result"]["structuredContent"]["valid"])

    def test_parse_error_does_not_kill_server(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.server(Path(directory) / "mcp.db") as process:
                assert process.stdin is not None
                assert process.stdout is not None
                process.stdin.write("{not-json}\n")
                process.stdin.flush()
                parsed = json.loads(process.stdout.readline())
                self.assertEqual(parsed["error"]["code"], -32700)

                initialized = self.request(
                    process,
                    {
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "initialize",
                        "params": {
                            "protocolVersion": "2025-11-25",
                            "capabilities": {},
                            "clientInfo": {"name": "recovery-test", "version": "1"},
                        },
                    },
                )
                self.assertEqual(initialized["result"]["serverInfo"]["name"], "MathOS")


if __name__ == "__main__":
    unittest.main()
