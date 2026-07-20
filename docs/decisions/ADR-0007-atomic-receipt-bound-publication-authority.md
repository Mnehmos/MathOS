# ADR-0007: Publication authority is an atomic receipt-bound evidence gate

Date: 2026-07-20

Status: accepted

## Context

ADR-0006 established that a protected GitHub workflow and an independently verified OIDC attestation are required before publication evidence can become authoritative. The protected workflow nevertheless produces only a non-authoritative candidate report. Controlled ingestion likewise preserves an immutable `publication_stage/1`, exact CAS members, raw GitHub CLI verification output, a canonical `publication_attestation_verification/1` record, and a separate SQLite ingestion receipt, all with `authoritative: false`.

Those records establish provenance for exact bytes. Their existence does not establish that the candidate passed publication policy, that its complete retained closure still replays against current canonical state, or that the formalization named by the request is still current. Treating receipt-table presence as proof authority would collapse attestation, semantic validation, and mathematical evidence into one status bit.

The existing `evidence/1` contract is already part of canonical identity. Adding authority fields to every serialized v1 record would change old canonical bytes and hashes. At the same time, permitting callers or generic Store code to construct an authoritative payload would recreate the direct status-mutation path prohibited by the product specification.

The authority commit must therefore be narrow, compatible, atomic, and independently defended at the application, Store, database, and read boundaries.

## Decision

MathOS introduces `evidence/2` as a separate closed authoritative-evidence contract. Existing `evidence/1` values remain byte-compatible: the new publication binding is omitted when absent, and v1 explicitly rejects kernel-proof kinds, kernel-refutation kinds, the `authoritative` class, and publication-authority metadata. Version 2 accepts only an `accepted`, `authoritative` `lean_kernel_proof` or `lean_kernel_refutation`. It requires an exact environment, a complete sorted unique artifact closure, no local run or job identity, no supersession, no staleness assertion, and one closed `publication_authority_binding/1`.

The authority binding names exactly:

- the non-authoritative ingestion receipt;
- its immutable publication stage;
- the canonical report bytes;
- the retained-closure manifest;
- the Sigstore bundle;
- the raw constrained-verifier output;
- the protected publication request; and
- the publication policy.

The evidence payload separately names the exact formalization object and version, environment, and internally derived proof or refutation kind. Its artifact list is the sorted unique closure of every staged retained member together with the report, retained-closure manifest, bundle, raw verification output, and canonical receipt. A receipt may produce at most one authoritative evidence record.

The public application operation accepts only an ingestion-receipt hash, actor, idempotency key, and optional dry-run. It does not accept a subject, outcome, evidence kind, result, authority class, environment, artifact list, report, binding, or evidence payload. The application derives all of those values from the immutable receipt, stage, report, request, current formalization, and committed policy. The report outcome maps to proof or refutation only after it agrees with the formalization's typed `claim_polarity`.

Before opening the authority transaction, the application replays the complete evidence chain. It re-reads every named CAS object, checks its exact hash and bounded size, parses the canonical report and retained closure, replays every typed cross-reference, re-derives the publication request from current canonical evidence, validates the persisted attestation output against the exact report and bundle, requires a `passed` publication report, and confirms the committed publication policy. A stage or receipt row is never a substitute for this replay.

The expensive CAS and semantic replay occurs before the database transaction so authority creation does not hold a write lock while reading and validating the retained package. The application then passes one application-derived, non-deserializable `PublicationAuthorityCommit` to the crate-private Store gate. The Store does not accept an `EvidencePayload` from this path.

Inside one immediate SQLite transaction, the Store re-reads the receipt and stage, reproduces their projections, rechecks the exact current formalization head, decodes and rehashes that formalization, verifies its environment and claim polarity, checks the request and policy artifacts, checks the complete artifact closure, enforces receipt uniqueness and idempotency, inserts the authoritative evidence, reads it back through integrity validation, and records the idempotency result. A head change between application replay and transaction acquisition therefore fails closed. The evidence insertion and idempotency result either commit together or not at all.

The Store authority method and commit type remain crate-private. Generic diagnostic, audit, and fidelity evidence paths cannot create kernel or authoritative evidence. CLI and MCP expose the same application operation and no generic authority action exists.

Migration 0011 adds receipt and stage projection columns plus a direct-SQL insert trigger. Any insert involving a kernel evidence kind, the authoritative class, `evidence/2`, or publication projections must satisfy the complete closed gate: exact field counts, receipt and stage joins, current formalization subject, environment, claim polarity, request and policy hashes, verifier identity, and complete sorted artifact closure. Existing immutability triggers reject later evidence, stage, and receipt rewrites.

Database enforcement is defense in depth, not a replacement for application replay. Every evidence read revalidates canonical payload identity, column projections, receipt and stage relationships, and closure membership, and fails closed on disagreement. A later consumer of authority must additionally replay the referenced CAS package; a coherent database row alone is not sufficient authority.

This decision establishes formal proof or refutation authority only for the exact formalization version named by the publication request. `claim_polarity` binds whether that formalization is intended as the claim or its negation, but it does not establish that the formalization faithfully represents the source statement. Statement fidelity remains independent reviewed evidence.

Slice E will separately derive source-claim truth from current authoritative formal evidence plus current verified fidelity and the remaining derived-truth preconditions. This gate does not set `proved` or `disproved`, does not create a direct truth-status mutation, and does not complete Pilot A by itself.

## Consequences

- Existing `evidence/1` canonical bytes and identities remain compatible.
- A valid attestation receipt remains explicitly non-authoritative until its exact closure passes replay and the atomic gate.
- A caller cannot choose the subject, result, authority class, evidence kind, or artifact closure of authoritative evidence.
- One receipt cannot be reused to authorize competing payloads.
- A concurrent formalization-head change is detected inside the authority transaction.
- Direct SQL insertion and later projection tampering fail closed under separate database and read-time checks.
- Authority remains portable because the evidence names the complete CAS closure needed for later replay.
- The protected workflow identity replaces a fictitious local run or job identity; both local fields are null in `evidence/2`.
- Changing the formalization, environment, request, policy, report, bundle, receipt, or retained member requires a newly reproduced publication chain.
- Formal authority still does not establish statement fidelity, novelty, pedagogy quality, source-claim truth, or publication release status.

## Rejected alternatives

### Extend `evidence/1` in place

Rejected because adding serialized authority metadata would change existing canonical bytes and hashes while leaving one schema version with two incompatible trust meanings.

### Treat a successful ingestion receipt as authoritative evidence

Rejected because the receipt establishes constrained provenance for staged bytes, not a passed report, complete semantic replay, current subject, or mathematical authority.

### Accept a caller-authored authoritative evidence payload

Rejected because the caller could select a convenient subject, result, evidence kind, environment, or incomplete artifact set and thereby recreate a direct promotion API.

### Rely only on application validation

Rejected because internal defects or direct SQL writes could bypass a single software layer. The crate-private Store gate, database trigger, immutability rules, and read-time validation provide independent fail-closed controls.

### Rely only on the SQL trigger

Rejected because SQLite cannot replay the retained CAS bytes, Lean reports, canonical request derivation, or constrained attestation semantics. Database checks protect the commit shape; application replay establishes the evidence chain.

### Perform the entire replay inside one long write transaction

Rejected because bounded CAS and semantic replay should not hold the SQLite writer lock. Replay happens first, followed by a short transaction that rechecks every mutable-currentness condition needed to close the race.

### Promote the source claim directly

Rejected because kernel authority applies to an exact formalization. Source-claim truth additionally requires independently reviewed fidelity and must be derived in Slice E rather than directly mutated.
