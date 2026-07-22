# ADR-0011: Portable releases are receipt-bound canonical directories

Date: 2026-07-21

Status: accepted

## Context

MathOS authority currently lives in SQLite projections plus content-addressed storage. That operational form is appropriate for compare-and-swap mutations and current-head queries, but it is not a portable result. A recipient must be able to copy a bounded release, remove access to the originating database, verify every byte and reference, and replay the exact Lean declaration without trusting caller-selected commands.

The protected publication closure already identifies the authoritative formalization, environment, module, evidence, report, attestation bundle, raw verification, and immutable receipt. Canonical pedagogy adds a reviewed exact-version curriculum path. Release construction must compose those roots without inventing a second authority model or treating an archive, manifest assertion, or successful replay as new mathematical evidence.

## Decision

MathOS defines a closed Rust-owned `release_manifest/1` contract and matching committed JSON Schema. A release is a directory containing `manifest.json` plus the required `objects/`, `edges/`, `evidence/`, `artifacts/`, `environments/`, `licenses/`, `replay/`, `reports/`, and `exports/` families. Every non-manifest file is a strictly sorted member with a safe relative path, kind, SHA-256 hash, exact byte size, license expression or null, explicit public/restricted/private classification, and registered artifact metadata when such metadata exists.

`manifest.json` is not a member of itself. Its identity is the SHA-256 of its exact RFC 8785 bytes, which avoids recursive self-hashing. Timestamps and output paths are excluded from the contract, so identical canonical inputs produce identical manifests. Builds require a new destination and never overwrite an existing path.

The application accepts only an immutable publication ingestion receipt hash and one exact reviewed pedagogy root plus bounded path parameters. Before exporting, it replays the same receipt, stage, report, retained closure, attestation output, current Store snapshots, and policy checks used by authority promotion. It requires the persisted authoritative evidence plus the current independently verified fidelity head for that exact formalization. It also revalidates every learning unit and then closes all exact record references, controlled repair and pedagogy edges, evidence artifacts, environments, formalization modules, counterexample package, and learning content artifacts. The release therefore starts from authority and pedagogy roots; it never scans CAS or accepts caller-authored members as authority.

Private releases preserve each member's resolved license and restriction. Publication-retained CAS bytes that have no registered artifact row remain explicitly private and unlicensed rather than receiving inferred metadata. Public release validation requires every member to be public and have a resolved license. Unknown, restricted, prohibited, private, or unlicensed content blocks the entire public build.

Offline verification is a static path that returns before MathOS configuration or database loading. It requires the trusted expected manifest hash and rejects symbolic links, non-files, unsafe paths, extra or missing files, whole-manifest substitution, hash or size changes, noncanonical JSON, schema-invalid records, unresolved exact references, missing artifacts or environments, authority/fidelity/repair or pedagogy binding changes, license-index drift, and any disagreement among the publication request, report, closure, stage, receipt, and manifest.

After structural verification, the verifier creates a fresh temporary workspace outside the bundle, scans the exact publication module for forbidden source tokens, appends only a verifier-controlled `#check` for the bound declaration, and invokes the allowlisted Lean executable with the exact pinned environment, cleared process environment, and bounded time and output. The CLI supplies no executable or argument surface. Release replay confirms portability; it does not promote or alter authority.

The protected Pilot A workflow first dry-runs and then builds a byte-identical private release from the repaired-proof receipt and five reviewed, privately eligible learning units. It copies the directory, hides `state.sqlite3`, invokes `mcl release verify` with a nonexistent instance root and the trusted build hash, and retains the bundle and playtest report.

## Consequences

- One manifest hash names the exact portable release independently of its location.
- An operational database, CAS layout, and idempotency tables are unnecessary for verification.
- Empty retained diagnostics remain hash-bound members instead of being silently dropped.
- Registered artifact policy is preserved; missing policy is never guessed into public eligibility.
- A copied release with one altered, missing, added, or symlinked member fails before Lean runs.
- Lean replay is typed and bounded, but remains an integrity check rather than a new authority source.
- Directory bundles are the canonical form. Transport archives may wrap them, but archive metadata is not release identity.
- MathCorpus, MCIP, RL/evaluation splits, leakage controls, and Comparator remain separate Phase 6 contracts.

## Rejected alternatives

### Export only the publication artifact

Rejected because the publication closure contains no reviewed curriculum, complete reference closure, release license index, or offline replay contract.

### Hash an archive file

Rejected because archive ordering, timestamps, permissions, and compression can change while the mathematical content does not. The canonical manifest binds directory members directly.

### List `manifest.json` inside itself

Rejected because a cryptographic member hash would be recursively defined.

### Trust the manifest without reopening members

Rejected because a manifest is a claim about bytes, not evidence that the copied files still match it.

### Accept a replay command from the caller

Rejected because it would turn release verification into an arbitrary process-execution surface and sever replay from the pinned environment contract.
