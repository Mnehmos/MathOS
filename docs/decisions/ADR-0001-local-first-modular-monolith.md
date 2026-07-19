# ADR-0001: Local-first Rust modular monolith

Status: Accepted

Date: 2026-07-19

Issue: [#4](https://github.com/Mnehmos/MathOS/issues/4)

## Context

The binding specification requires production software whose mathematical authority, state mutations, and portable releases can be inspected and reproduced. The repository previously contained a Python finite-domain claim kernel that exercised a much narrower lifecycle and was incorrectly described as 1.0.

The canonical architecture must support SQLite state, content-addressed artifacts, typed domain policies, durable jobs, Lean verification, CLI and MCP interfaces, and portable releases on one machine. The proof verifier is a trust boundary, not a model feature.

## Decision

The canonical implementation is one Rust binary named `mcl`, organized as modules inside one crate until a concrete ownership, compile, reuse, or trust boundary requires another crate.

The binary uses:

- one SQLite database in WAL mode;
- one SHA-256 content-addressed artifact directory;
- one SQLite-backed jobs table;
- one narrow, allowlisted Lean 4 subprocess adapter;
- one shared application layer for CLI and MCP;
- explicit immutable versions and evidence-derived truth;
- portable release bundles that do not depend on the operational database.

The former Python implementation remains readable migration input and regression evidence. It is not a second production service and cannot write canonical state after cutover.

## Consequences

- Trust policies can be represented with Rust enums, checked transitions, database constraints, and exhaustiveness.
- Local operation remains understandable and backupable without distributed infrastructure.
- CLI and MCP behavior cannot diverge into separate domain paths.
- SQLite concurrency and one-machine resource limits are explicit product constraints.
- Verifier isolation must be described honestly because a local subprocess is not a hardened virtualization boundary.
- The migration must preserve old identifiers and failures without upgrading their trust.

## Rejected alternatives

### Keep the Python kernel as the canonical product

Rejected because its finite-domain ontology, persistence model, status semantics, verifier path, release model, and feature coverage do not satisfy the binding contract.

### Split the product into services

Rejected because no measured scale or ownership boundary justifies deployment complexity before 1.0.

### Add a generic verifier or plugin platform

Rejected because Lean 4 is the only required proof backend, and generic execution would widen a critical security boundary.

### Add graph or vector databases

Rejected because typed SQLite edges, exact lookup, FTS5, and environment-aware declaration search are the specified simpler path.

## Amendment rule

Changing this deployment shape, canonical language, database, proof backend, or external-model boundary requires a new accepted ADR with measured evidence and migration consequences.
