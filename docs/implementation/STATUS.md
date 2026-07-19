# Implementation Status

Last updated: 2026-07-19

## Release truth

MathOS 1.0.0 is not complete and has not been released.

The binding contract is the root [SPEC.md](../../SPEC.md). The former Python finite-domain implementation is retained as legacy migration input. It is not the canonical product implementation and does not satisfy the 1.0 Definition of Done.

## Current phase

Transition from Phase 2 canonical interfaces to Phase 3 formalization and Lean authority.

Active implementation issue: [#15, implement immutable Lean environment manifests](https://github.com/Mnehmos/MathOS/issues/15).

Closure gate still monitored: [#14, add a thin MCP stdio adapter](https://github.com/Mnehmos/MathOS/issues/14) was reopened for one literal invalid-action test and will close after that exact branch state passes remote CI.

Active branch: `feat/spec-driven-rust-rebuild`.

## Completed criteria with evidence

- Root normative specification exists and includes sections 0 through 37.
- The package and legacy Python adapter report version `0.1.0`, removing the false local 1.0 assertion.
- The Rust `mcl` binary compiles from `Cargo.lock` and exposes only implemented Phase 1 commands.
- `mcl init` creates a real SQLite database in WAL mode, applies all committed migrations, and writes a SHA-256 content-addressed canary.
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
- All 11 specified run kinds and a closed execution-event vocabulary are exhaustive Rust variants.
- Run creation atomically records actor, canonical budget, UUIDv7 identity, and a hash-chained origin event.
- Event append uses expected-head compare-and-swap and immutable idempotency receipts; concurrent writers produce one winner and one structured conflict.
- SQLite anchors and triggers reject missing predecessors, gaps, rewrites, deletion, and run-origin mutation.
- Chain verification detects forged payloads, reordered events, and final-event truncation, including after restart.
- Run history remains explicitly non-authoritative for proof, fidelity, and novelty.
- Graph traversal begins from an exact object and version pair and preserves exact version-bound edges in every result.
- Incoming, outgoing, and bidirectional traversal support typed edge-kind filters without accepting raw query text.
- Depth, result count, and scanned edges are bounded; cycles terminate without duplicate edge results.
- Traversal ordering is deterministic across restart and remains read-only and non-authoritative.
- Source and claim payloads have separate closed Rust types and committed JSON Schemas for `source/1` and `claim/1`.
- Source records explicitly preserve original text, locator, licensing, redistribution, citations, redaction, and provenance.
- Claim records explicitly preserve exact source reference, normalized statement, kind, assumptions, variables, concept links, citations, and ambiguity.
- Canonical create and version paths reject unknown fields, unsupported schema versions, malformed hashes, empty required text, and excessive collections before persistence.
- Schema rejection leaves no record or idempotency receipt, while valid original source text survives restart byte-for-byte.
- Concept payloads have a closed Rust type and committed `concept/1` JSON Schema covering aliases, domains, formal declarations, licensed taxonomy crosswalks, pedagogy references, and provenance.
- Formalization payloads have a closed Rust type and committed `formalization/1` JSON Schema covering one exact claim version, Lean environment, module artifact, declaration identity, theorem type, imports, notes, and separate evidence references.
- Formalization payloads reject embedded `proved`, `disproved`, `faithful`, and `certified` verdicts. These conclusions remain outside the formalization record.
- One claim can retain multiple formalization objects, and changes to theorem type, environment, module artifact, or imports produce different canonical hashes.
- A formalization must reference an exact existing claim object and version. Missing references and references to other record kinds fail before persistence.
- GitHub Actions run `29696708243` passed Rust tests and warnings-denied lint on fresh Linux and Windows runners, the real-storage smoke test, and all legacy Python regression tests.
- The fresh Linux runner installed the exact pinned Lean 4.32.0 toolchain from a SHA-256-verified Elan installer and executed `lean --version` successfully. This establishes toolchain availability only, not proof authority.
- Sources, concepts, claims, and formalizations now use one typed application service for CLI create, version, exact retrieval, and dry-run validation.
- CLI entity mutations bind the committed schema version, require actor and idempotency attribution, and preserve compare-and-swap versioning.
- Canonical FTS search is available through that same application service, and CLI integration covers dry-run non-mutation, create, version, current and historical reads, restart, search, and wrong-family rejection.
- Version-bound edge creation, exact edge retrieval, and bounded typed graph traversal now use the same application service and CLI path.
- Research run creation, retrieval, event listing, event append, and hash-chain verification now use that shared path while remaining explicitly non-authoritative.
- Edge, run, and run-event dry runs validate without mutation. Real mutations preserve store-level idempotency before evaluating changed current state.
- CLI adversarial coverage caught and fixed an application-layer retry-order defect, then verified identical event retries, stale-head conflicts, graph bounds, restart persistence, and chain validity.
- Golden fixtures pin representative record-mutation, edge-mutation, and run-chain JSON response shapes after normalizing only dynamic identities and timestamps.
- The issue #13 CLI surface contains no proof, disproof, fidelity, novelty, certification, raw SQL, arbitrary shell, or unrestricted executable action. Its only process launch remains the allowlisted Lean availability check in `doctor`.
- CLI integration rejects stale canonical version writers without changing the accepted head.
- ADR-0004 pins the MCP `2025-11-25` stable protocol, stdio transport, exact official Rust SDK release, one-way application-service dependency, and disabled inference and network capabilities.
- `mcl serve` now runs a real MCP `2025-11-25` server over newline-delimited stdio through the exactly pinned official Rust SDK.
- The initial MCP surface exposes only closed `system` and `query` families. It provides identity, health, capability, policy, exact record, FTS5 search, and bounded graph actions without direct storage access.
- MCP tool schemas have an object root, reject unknown fields, bound search and graph work, and return stable application errors as structured tool failures.
- Real subprocess tests exercise initialization, tool discovery, tool calls, invalid parameters, forbidden tool names, stdout purity, clean EOF shutdown, restart, and persisted-state recovery.
- CLI-created canonical state produces the same serialized search and exact-record results when read through MCP, establishing parity for the implemented read surface.
- The MCP surface now includes all six issue #14 families: `system`, `query`, `source`, `claim`, `formalization`, and `research`.
- Source, claim, and formalization proposals and versions require explicit actor and idempotency attribution; versions additionally require compare-and-swap object and head identities.
- Research start and submit actions require the same attribution controls, while observe remains read-only. Every recorded run remains explicitly non-authoritative.
- MCP dry runs validate without mutation, exact retries return the original result, stale writers fail with structured conflicts, and irrelevant mutation fields fail closed.
- Adversarial MCP coverage rejects embedded proof verdicts in formalizations and confirms that no `mark_proved`, raw shell, raw SQL, model-routing, or publication action exists.
- The environment domain now has a closed `environment/1` Rust manifest, committed JSON Schema, and golden canonical SHA-256 fixture.
- Environment identity includes exact Lean release, dependency revisions, imports, project configuration hashes, platform, trust profile, typed verifier command, explicit resource limits, network policy, and working-directory policy.
- Environment validation rejects unpinned dependencies, unknown and machine-specific fields, path-shaped imports, duplicates, noncanonical ordering, arbitrary commands, network-enabled verification, unsafe hashes, and zero or excessive limits before hashing.
- Changing an environment-relevant field changes the canonical environment hash. This establishes context identity only and does not establish a proof.
- Migration 0006 adds immutable environment attribution and database triggers that reject environment update and deletion.
- Environment registration is content-addressed, actor-attributed, idempotent, durable across restart, exactly retrievable, and deterministically listable through the shared application service and CLI.
- Environment dry runs validate and predict the exact hash without mutation. Idempotency-key reuse for a different manifest fails closed.
- Environment reads recompute manifest identity and trust profile, detecting corrupted stored JSON even if database mutation guards are bypassed.
- `mcl doctor` reports registered environment count while explicitly stating that environment identity does not establish proof authority.
- New formalizations must reference an exact registered environment hash. A syntactically valid but unresolved hash fails before canonical record persistence.

These items establish only part of the product foundation and Phase 2 trace model. They do not establish any mathematical claim, Lean proof authority, complete MCP mutation surface, pilot, portable release, or 1.0 acceptance result.

## Active work

- Require fresh Linux and Windows CI evidence containing the explicit unknown-action MCP test before closing issue #14 again.
- Publish the completed issue #15 persistence and formalization-binding slice and require fresh Linux and Windows CI.
- Design the narrow Lean subprocess boundary only after the exact environment can be resolved from canonical state.
- Keep the local Lean launch limitation visible without misclassifying it as a project-wide blocker.

## Next highest-priority criteria

1. Obtain remote CI evidence for environment persistence, forward migration, CLI, corruption detection, and formalization binding.
2. Implement the narrow Lean elaboration boundary using registered environments.
3. Implement evidence records and derived truth rules before any proof-status surface exists.
4. Complete Pilot A through the real interfaces only after those authority controls exist.

## Exact last validation commands

Run from the repository root. The explicit local toolchain path is required only in this managed workspace:

```text
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo fmt --check
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo clippy --workspace --all-targets --all-features -- -D warnings
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo test --workspace
PYTHONPATH=src PYTHONWARNINGS=error::ResourceWarning python -m unittest discover -s tests -v
git diff --check
```

Observed validation evidence for this update:

- formatting passed;
- warnings-denied Clippy passed;
- 52 Rust unit tests passed;
- 7 Rust CLI integration tests and 3 Rust MCP subprocess integration tests passed;
- 39 legacy Python regression tests passed;
- patch whitespace validation passed.
- GitHub Actions run `29698741558` passed all five jobs for the completed MCP mutation surface, including exact pinned Lean availability and both Rust operating-system targets. The subsequent explicit invalid-action test awaits a branch CI run.

## Release readiness

Not ready. The release checklist is overwhelmingly open, all four mandatory pilots are incomplete in the specified architecture, later verifier, pedagogy, and release MCP families do not exist because their application capabilities do not yet exist, and no authoritative Lean evidence has been produced.
