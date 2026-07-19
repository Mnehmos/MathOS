# Implementation Status

Last updated: 2026-07-19

## Release truth

MathOS 1.0.0 is not complete and has not been released.

The binding contract is the root [SPEC.md](../../SPEC.md). The former Python finite-domain implementation is retained as legacy migration input. It is not the canonical product implementation and does not satisfy the 1.0 Definition of Done.

## Current phase

Phase 1: Governance and executable skeleton.

Active issue: [#4, establish the governed Rust application skeleton](https://github.com/Mnehmos/MathOS/issues/4).

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

These items establish only part of Phase 1. They do not establish any mathematical claim, Lean proof authority, MCP behavior, pilot, portable release, or 1.0 acceptance result.

## Active work

- Complete the Phase 1 CI matrix and documentation controls.
- Validate the pinned Lean toolchain on a fresh Linux CI runner because the current managed execution sandbox cannot launch the Lean runtime.
- Add schema and migration consistency checks.
- Commit and review the first coherent governed foundation.

## Next highest-priority criteria

1. Close Phase 1 issue #4 with CI evidence.
2. Implement immutable canonical records, canonical JSON, and golden hash vectors.
3. Implement typed edges, artifacts, runs, hash-chained events, exact lookup, and FTS search.
4. Establish one shared application path for the CLI and MCP adapter.
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
- 9 Rust unit tests passed;
- 4 Rust CLI integration tests passed;
- manual initialization exited 0 with migration 1 and WAL mode;
- manual health exited 0 after an FTS5 probe defect was reproduced and repaired;
- manual doctor exited 1 only because Lean could not execute in the managed local sandbox.

- 39 legacy Python regression tests passed;
- patch whitespace validation passed.

## Release readiness

Not ready. The release checklist is overwhelmingly open, all four mandatory pilots are incomplete in the specified architecture, MCP is not implemented in Rust, and no authoritative Lean evidence has been produced.
