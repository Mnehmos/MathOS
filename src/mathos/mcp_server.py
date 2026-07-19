import argparse
import json
from pathlib import Path
import sqlite3
import sys
from typing import Any

from . import __version__
from .engine import ClaimEngine
from .trajectory import verify_trajectory


PROTOCOL_VERSIONS = {"2025-11-25", "2025-06-18"}


TOOLS: list[dict[str, Any]] = [
    {
        "name": "mathos_submit_claim",
        "title": "Submit Mathematical Claim",
        "description": "Store an informal claim and optional finite formal specification.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "informal_statement": {"type": "string", "minLength": 1},
                "formal_spec": {"type": ["object", "null"]},
            },
            "required": ["informal_statement"],
            "additionalProperties": False,
        },
    },
    {
        "name": "mathos_run_claim",
        "title": "Search and Verify Claim",
        "description": "Run untrusted search followed by independent verification.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "claim_id": {"type": "string"},
                "max_assignments": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100000,
                },
            },
            "required": ["claim_id"],
            "additionalProperties": False,
        },
    },
    {
        "name": "mathos_get_claim",
        "title": "Get Claim",
        "description": "Retrieve a claim and its provenance events.",
        "inputSchema": {
            "type": "object",
            "properties": {"claim_id": {"type": "string"}},
            "required": ["claim_id"],
            "additionalProperties": False,
        },
    },
    {
        "name": "mathos_export_rl",
        "title": "Export RL Trajectory",
        "description": "Return the versioned trajectory for one claim.",
        "inputSchema": {
            "type": "object",
            "properties": {"claim_id": {"type": "string"}},
            "required": ["claim_id"],
            "additionalProperties": False,
        },
    },
    {
        "name": "mathos_verify_ledger",
        "title": "Verify Provenance Ledger",
        "description": "Replay and validate the tamper-evident event chain.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "additionalProperties": False,
        },
    },
    {
        "name": "mathos_validate_rl",
        "title": "Validate RL Trajectory",
        "description": "Validate hashes, evidence links, and outcome semantics in an export.",
        "inputSchema": {
            "type": "object",
            "properties": {"trajectory": {"type": "object"}},
            "required": ["trajectory"],
            "additionalProperties": False,
        },
    },
]


def _response(request_id: str | int | None, result: Any) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def _error(
    request_id: str | int | None,
    code: int,
    message: str,
    data: Any | None = None,
) -> dict[str, Any]:
    error: dict[str, Any] = {"code": code, "message": message}
    if data is not None:
        error["data"] = data
    return {"jsonrpc": "2.0", "id": request_id, "error": error}


def _tool_result(value: Any, *, is_error: bool = False) -> dict[str, Any]:
    return {
        "content": [
            {
                "type": "text",
                "text": json.dumps(value, ensure_ascii=False, sort_keys=True),
            }
        ],
        "structuredContent": value,
        "isError": is_error,
    }


class MathOSMcpServer:
    def __init__(self, engine: ClaimEngine) -> None:
        self.engine = engine
        self.initialized = False
        self.protocol_version: str | None = None

    def handle(self, message: dict[str, Any]) -> dict[str, Any] | None:
        request_id = message.get("id")
        if message.get("jsonrpc") != "2.0" or not isinstance(message.get("method"), str):
            return _error(request_id, -32600, "Invalid Request")
        method = message["method"]
        params = message.get("params", {})
        if not isinstance(params, dict):
            return _error(request_id, -32602, "Invalid params")

        if method == "initialize":
            requested = params.get("protocolVersion")
            if requested not in PROTOCOL_VERSIONS:
                requested = "2025-11-25"
            self.protocol_version = requested
            return _response(
                request_id,
                {
                    "protocolVersion": requested,
                    "capabilities": {"tools": {"listChanged": False}},
                    "serverInfo": {"name": "MathOS", "version": __version__},
                    "instructions": (
                        "Search output is untrusted until MathOS returns a verified outcome."
                    ),
                },
            )
        if method == "notifications/initialized":
            self.initialized = True
            return None
        if not self.initialized:
            return _error(request_id, -32002, "Server not initialized")
        if method == "ping":
            return _response(request_id, {})
        if method == "tools/list":
            return _response(request_id, {"tools": TOOLS})
        if method == "tools/call":
            return _response(request_id, self._call_tool(params))
        if request_id is None:
            return None
        return _error(request_id, -32601, "Method not found")

    def _call_tool(self, params: dict[str, Any]) -> dict[str, Any]:
        name = params.get("name")
        arguments = params.get("arguments", {})
        if not isinstance(arguments, dict):
            return _tool_result(
                {"error": "arguments must be an object"}, is_error=True
            )
        try:
            if name == "mathos_submit_claim":
                value = self.engine.submit(
                    arguments["informal_statement"], arguments.get("formal_spec")
                ).to_dict()
            elif name == "mathos_run_claim":
                value = self.engine.process(
                    arguments["claim_id"],
                    max_assignments=arguments.get("max_assignments", 10_000),
                ).to_dict()
            elif name == "mathos_get_claim":
                claim_id = arguments["claim_id"]
                value = {
                    "claim": self.engine.get_claim(claim_id).to_dict(),
                    "events": self.engine.ledger.events_for_claim(claim_id),
                }
            elif name == "mathos_export_rl":
                value = self.engine.export_trajectory(arguments["claim_id"])
            elif name == "mathos_verify_ledger":
                value = self.engine.verify_provenance().to_dict()
            elif name == "mathos_validate_rl":
                value = verify_trajectory(arguments["trajectory"]).to_dict()
            else:
                return _tool_result({"error": f"unknown tool: {name}"}, is_error=True)
            return _tool_result(value)
        except (KeyError, OSError, TypeError, ValueError, sqlite3.Error) as error:
            return _tool_result(
                {"error": type(error).__name__, "message": str(error)},
                is_error=True,
            )


def serve(database: Path) -> int:
    engine = ClaimEngine.open(database)
    server = MathOSMcpServer(engine)
    try:
        for line in sys.stdin:
            try:
                message = json.loads(line)
                if not isinstance(message, dict):
                    response = _error(None, -32600, "Invalid Request")
                else:
                    response = server.handle(message)
            except json.JSONDecodeError as error:
                response = _error(None, -32700, "Parse error", {"detail": str(error)})
            except Exception as error:
                print(f"MathOS MCP internal error: {error}", file=sys.stderr)
                response = _error(None, -32603, "Internal error")
            if response is not None:
                sys.stdout.write(
                    json.dumps(response, ensure_ascii=False, separators=(",", ":")) + "\n"
                )
                sys.stdout.flush()
    finally:
        engine.close()
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="mathos-mcp")
    parser.add_argument("--db", required=True, type=Path)
    args = parser.parse_args(argv)
    return serve(args.db)


if __name__ == "__main__":
    raise SystemExit(main())
