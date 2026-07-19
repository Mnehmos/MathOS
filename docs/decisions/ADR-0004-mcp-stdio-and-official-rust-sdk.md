# ADR-0004: MCP stdio and the official Rust SDK

Date: 2026-07-19

Status: accepted

## Context

MathOS requires Model Context Protocol as one external interface. The protocol evolves independently of this repository, and hand-maintaining JSON-RPC lifecycle, negotiation, tool schemas, and transport framing would create avoidable compatibility and security risk.

The canonical application service already owns implemented domain behavior. MCP must adapt to that service rather than open SQLite or reproduce policy decisions.

## Decision

MathOS will implement its `1.0.0` local MCP interface as newline-delimited UTF-8 JSON-RPC over stdio.

The adapter targets the stable MCP specification revision `2025-11-25`. Protocol upgrades require an explicit dependency review, conformance evidence, migration notes, and an ADR amendment.

The implementation uses the official `modelcontextprotocol/rust-sdk` crate `rmcp`, pinned exactly to release `2.2.0` in `Cargo.toml` and transitively frozen by `Cargo.lock`. Only server, schema, macro, and stdio transport features needed by this interface may be enabled. HTTP, OAuth, client inference, sampling, elicitation, roots, prompts, resources, generic tasks, and child-process features remain disabled.

The public MCP surface uses a small set of product families with closed discriminated actions. Each handler calls `Application`; no handler may open the database, execute a process, route a model, or decide mathematical status.

Stdout is reserved exclusively for MCP protocol messages. Human and structured diagnostics use stderr and must not contain proof bodies or restricted source text by default.

## Consequences

- Protocol lifecycle and schema encoding are delegated to the official SDK.
- The dependency graph grows to include an asynchronous runtime and schema generation.
- Cross-platform CI must compile and exercise the actual stdio server.
- Dependency and protocol upgrades are deliberate rather than automatically floating.
- MCP cannot expose a capability until the shared application service implements and tests it first.

## Rejected alternatives

### Hand-written JSON-RPC server

Rejected because protocol evolution, lifecycle edge cases, framing, and schema compliance would become repository-owned security work without product benefit.

### MCP-specific database adapter

Rejected because it would create a second domain path and allow CLI and MCP trust behavior to diverge.

### Streamable HTTP in `1.0.0`

Rejected because the required deployment is local-first on one machine. HTTP adds authentication, origin, session, and network boundaries without advancing current acceptance criteria.

### MCP sampling or provider integration

Rejected because model inference is external by product invariant. MathOS is the verifier-backed environment, not the model host.

## References

- [MCP stable lifecycle specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
- [MCP stable transport specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [Official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
