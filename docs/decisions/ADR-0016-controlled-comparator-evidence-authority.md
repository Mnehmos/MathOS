# ADR-0016: Comparator authority is a receipt-bound replayed evidence gate

Date: 2026-07-22

Status: accepted

## Context

ADR-0015 produces an exact protected official Comparator report and authenticates its bytes with
GitHub OIDC provenance. The report is intentionally `authoritative: false`. Attestation proves
where exact bytes came from; it does not establish that the named formalization, publication
authority, fidelity witness, release, plan, package, tools, policy, and retained artifacts are
still the current canonical closure.

The existing `evidence/1` contract excludes all authoritative values. `evidence/2` has a narrower
published meaning: an accepted Lean kernel proof or refutation bound to a publication receipt.
Reinterpreting either version to admit Comparator authority would change the semantics of existing
canonical hashes.

## Decision

MathOS adds `evidence/3` as a closed accepted authoritative `comparator_run` contract. Existing
`evidence/1` and `evidence/2` serialized bytes, schemas, and validation remain unchanged. Version 3
requires one `comparator_authority_binding/1`, an exact formalization and environment, a sorted
complete artifact closure, no local run or job identity, no supersession, and no caller-authored
staleness assertion.

Authority uses three separate application operations:

1. staging verifies the exact 20-file run bundle, independently reprojects the five-file package
   from a supplied canonical plan and frozen release, rejects unsafe filesystem entries, and
   places the exact run, release, plan, policy, and Sigstore bundle bytes in CAS under an immutable
   non-authoritative `comparator_authority_stage/1`;
2. ingestion re-reads only staged CAS bytes, repeats structural and semantic verification, invokes
   only the hash- and version-pinned GitHub CLI verifier with typed repository, workflow, protected
   ref, source and signer commit, predicate, subject, and hosted-runner constraints, parses the
   verifier result, and creates an immutable non-authoritative
   `comparator_attestation_verification/1` receipt; and
3. promotion accepts only receipt hash, actor, idempotency key, and dry-run. It replays every CAS
   member and current canonical binding, derives the evidence subject, kind, result, authority,
   environment, verifier identity, and complete closure, and passes a non-deserializable commit to
   a crate-private Store gate.

The expensive CAS, release, package, attestation, publication-authority, and fidelity replay occurs
before the write transaction. The short Store transaction re-reads the immutable stage and
receipt, rechecks the current formalization head and database projections, enforces one evidence
per receipt and idempotency, inserts the evidence, reads it back through integrity validation, and
commits the idempotency result atomically. Immutable tables, a closed SQL insert trigger, and
read-time projection checks provide separate defense in depth.

Comparator evidence currentness is derived live. The evidence remains immutable historical fact;
it is `current` only while the exact formalization, publication authority and fidelity witness,
release, plan, package fingerprint, policy and tool pins, report, attestation, and every required
CAS member replay. A changed canonical input returns a deterministic stale reason. Missing or
corrupt bytes are integrity failures rather than ordinary staleness.

Comparator authority records that an exact publication package passed the reviewed official
Comparator boundary. It does not create Lean proof or refutation evidence, replace statement
fidelity review, directly set `proved` or `disproved`, or complete Pilot C.

## Consequences

- A protected report, a successful workflow, or caller-selected hashes cannot self-promote.
- Prior evidence versions remain portable and byte-compatible.
- Attestation receipt creation and mathematical authority remain distinct transitions.
- Retry, restart, direct SQL, incomplete closure, receipt reuse, current-head races, and later
  projection tampering fail closed under separate controls.
- Historical Comparator acceptance remains auditable after it becomes stale for current use.
- Promotion is intentionally more expensive than a projection insert because it replays the exact
  portable evidence chain.

## Rejected alternatives

### Extend `evidence/2`

Rejected because version 2 is already the closed publication-bound Lean proof/refutation contract.

### Trust the report's `comparator_verified` field

Rejected because the report generator and workflow are evidence producers, not the canonical
authority gate.

### Treat attestation verification as promotion

Rejected because authentic bytes may still name stale, substituted, or incomplete canonical
inputs.

### Persist a mutable stale flag

Rejected because currentness depends on independently versioned live chains. It is derived by
replay and never rewritten into historical evidence.

### Let callers construct `evidence/3`

Rejected because caller selection of the subject, environment, result, binding, or artifact set
would recreate a generic authority mutation API.
