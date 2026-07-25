# ADR-0017: Non-null source content is a verified portable binding

Date: 2026-07-25

Status: accepted

## Context

The closed `source/1` schema already permits an optional SHA-256 `content_hash`. Syntax validation
previously established only that a non-null value looked like a hash. Source create and version
operations could therefore name missing bytes, an unrelated artifact, generated output, or content
whose license and redistribution policy contradicted the source record. Portable releases included
the source record but did not derive an artifact member from that field.

That gap matters before BH Pilot C. A provenance record that survives while its named source bytes
do not is not a database-independent research source boundary. At the same time, changing
`source/1` would alter existing canonical identities, and treating source availability as fidelity
or mathematical authority would collapse separate trust roles.

## Decision

`source/1` bytes and hashes remain unchanged. `content_hash: null` continues to mean that exact
source bytes are unavailable.

When `content_hash` is non-null, the application requires a registered CAS artifact whose bytes
rehash to that identity. Its immutable metadata must carry the reviewed `source_content` role, use
user-ingest/import/migration provenance, and be no less restrictive than the source's license,
redaction, and redistribution declarations. The check runs before create/version dry runs,
idempotency lookup, and persistence so an idempotent retry cannot conceal later CAS loss.

Migration 0013 adds a separate SQL insert trigger over the relational metadata projections and
fails closed if existing source versions violate the new closure. It cannot replace CAS
verification; it prevents direct SQL from bypassing the registered-role and policy contract.

Release construction derives every non-null source hash from the exact record closure, includes its
bytes and metadata at `artifacts/<sha256>`, and fails if CAS or metadata no longer verifies.
Database-free release verification enforces exact artifact paths and identities and reruns the same
source-content policy from manifest-retained metadata.

The initial conformance fixture is the Apache-2.0 `SOURCE-LOCK.md` file from the exact pinned
`Mnehmos/BHFormalization` commit. It exercises source portability only.

## Consequences

- Existing null source bindings remain compatible and do not invent unavailable bytes.
- A non-null binding is meaningful across CLI, MCP, retry, restart, direct SQL defense, and copied
  releases.
- Public redistribution cannot be inferred from a source record when the exact artifact is private,
  unlicensed, or differently licensed.
- An incompatible legacy non-null binding blocks migration until an operator supplies a reviewed
  exact artifact instead of receiving silent trust promotion.
- Release verification remains independent of SQLite while retaining the policy inputs it checks.
- The decision creates no statement fidelity, Lean proof/refutation, Comparator authority, or
  derived source truth.

## Rejected alternatives

### Keep syntax-only hashes

Rejected because a well-formed digest without retained verified bytes is not a source-content
closure.

### Embed artifact bytes in `source/2`

Rejected because the CAS and release manifest already provide immutable byte identity and policy,
while a schema revision would unnecessarily change canonical source hashes.

### Trust any registered artifact

Rejected because generated reports, verifier outputs, and review documents can be registered
artifacts but are not thereby source content.

### Check only during release construction

Rejected because canonical source state and idempotent retries would remain able to bind missing or
misclassified content.

### Infer fidelity or authority from exact bytes

Rejected because byte integrity and provenance do not establish that a formalization matches the
source or that a mathematical claim is true.
