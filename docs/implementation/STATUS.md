# Implementation Status

Last updated: 2026-07-19

## Release truth

MathOS 1.0.0 is not complete and has not been released.

The binding contract is the root [SPEC.md](../../SPEC.md). The former Python finite-domain implementation is retained as legacy migration input. It is not the canonical product implementation and does not satisfy the 1.0 Definition of Done.

## Current phase

Phase 2: Canonical records and trace.

Active issue: [#6, enforce typed version-bound graph edges](https://github.com/Mnehmos/MathOS/issues/6).

Active branch: `feat/spec-driven-rust-rebuild`.

## Completed criteria with evidence

- Root normative specification exists and includes sections 0 through 37.
- The package and legacy Python adapter report version `0.1.0`, removing the false local 1.0 assertion.
- The Rust `mcl` binary compiles from `Cargo.lock` and exposes only implemented Phase 1 commands.
- `mcl init` creates a real SQLite database in WAL mode, applies migration 1, and writes a SHA-256 content-addressed canary.
- `mcl health` checks database integrity, migration state, WAL mode, FTS5, and artifact-root containment without creating a missing database.
- `mcl doctor` adds artifact round-trip, stale-lease, and Lean availability checks and exits nonzero when unhealthy.
- Artifact paths reject malformed hashes, parent traversal, and a symlinked artifact root.
- Configured database and artifact paths reject parent traversal and existing-ancestor symlink escape.
- Rust unit and CLI integration tests use real temporary SQLite databases and artifact stores.
- Lean is pinned in `lean-toolchain` to `leanprover/lean4:v4.32.0`.
- Canonical JSON uses RFC 8785 with fail-closed IEEE-754 safe-integer validation and a golden cross-language hash vector.
- Stable canonical objects use UUIDv7; immutable versions use the specified schema-bound SHA-256 formula.
- Create and version mutations persist actor attribution and immutable idempotency receipts.
- Compare-and-swap heads serialize concurrent writers into one winner and one structured conflict.
- Database triggers reject version rewrites, head clearing, head downgrade, cross-object heads, identity rewrites, and idempotency-receipt mutation.
- Exact object and version lookup, restart persistence, and current-head FTS5 projection work through the real SQLite store.
- All 30 specified logical, pedagogical, research, provenance, and implementation edges are exhaustive Rust variants.
- Edge endpoints bind exact versions owned by exact stable objects; edge payloads are canonical JSON and edge rows are immutable.
- Hard pedagogical prerequisites remain acyclic through both application checks and SQLite triggers, while logical equivalence cycles remain valid.

These items establish only part of Phase 1. They do not establish any mathematical claim, Lean proof authority, MCP behavior, pilot, portable release, or 1.0 acceptance result.

## Active work

- Publish the three local controlled commits and run the Phase 1 CI matrix once the required GitHub CLI is available.
- Validate the pinned Lean toolchain on a fresh Linux CI runner because the current managed execution sandbox cannot launch the Lean runtime.
- Continue Phase 2 with hash-chained run events.
- Generate and commit the first typed JSON Schemas from the domain contract.

## Next highest-priority criteria

1. Close Phase 1 issue #4 and Phase 2 issue #5 with remote CI evidence.
2. Implement runs, hash-chained events, and graph traversal queries.
3. Establish source and claim schemas on the shared application path.
4. Add the MCP adapter only after it can call that same real application path.
5. Begin the Lean authority path only after the environment is pinned and executable.

## Exact last validation commands

Run from the repository root with the repo-local Rust toolchain on `PATH`:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
target/debug/mcl --root /tmp/mathos-phase1-evidence-20260719 --json init --actor gpt-5.6-sol --idempotency-key phase1-cli-001
target/debug/mcl --root /tmp/mathos-phase1-evidence-20260719 --json health
target/debug/mcl --root /tmp/mathos-phase1-evidence-20260719 --json doctor
PYTHONPATH=src PYTHONWARNINGS=error::ResourceWarning python -m unittest discover -s tests -v
git diff --check
```

Observed Rust evidence before this update:

- formatting passed;
- warnings-denied Clippy passed;
- 22 Rust unit tests passed;
- 4 Rust CLI integration tests passed;
- manual initialization exited 0 with migrations through version 4 and WAL mode;
- manual health exited 0 after an FTS5 probe defect was reproduced and repaired;
- manual doctor exited 1 only because Lean could not execute in the managed local sandbox.

- 39 legacy Python regression tests passed;
- patch whitespace validation passed.

## Release readiness

Not ready. The release checklist is overwhelmingly open, all four mandatory pilots are incomplete in the specified architecture, MCP is not implemented in Rust, and no authoritative Lean evidence has been produced.
