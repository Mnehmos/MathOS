# ADR-0009: Counterexample repair is an atomic package-and-lineage commit

Date: 2026-07-21

Status: accepted

## Context

A derived `disproved` result establishes that a current exact source claim has a qualifying refutation formalization, role-separated fidelity review, and receipt-bound publication authority. It does not identify the repaired proposition. Quietly editing the false claim would erase the proposition that the refutation actually addressed, while copying its authority to a changed statement would assert mathematics that was never checked.

A useful repair must preserve the counterexample itself, the exact evidence that made it consequential, the search trace that found or selected it, and the exact change proposed for the statement. These values span CAS, canonical records, graph lineage, run history, and current truth inputs. Independent writes would permit a package without a claim, a claim without its repair edge, or an edge referring to bytes that were never registered.

## Decision

MathOS defines closed `counterexample_repair_request/1`, `counterexample_package/1`, and `claim_repair_edge/1` contracts. The public request may select the exact disproved claim and one of its current refutation formalizations, provide a typed canonical JSON witness, optional minimization notes and supporting artifacts, explain the failure, select one SPEC repair operation, propose a complete repaired `claim/1` payload, and bind the exact current head of a `counterexample_search` run.

The caller cannot provide research status, checker identity, fidelity or authority evidence, receipt identity, package bytes or hash, repaired object ID or version hash, artifact metadata, or a graph edge. The application derives those fields after replaying the existing live claim-status service. A repair is admitted only when the exact current claim is `disproved` and the selected formalization is exactly one current refutation witness.

The application rehashes the source, claim, and negation formalization; verifies the pinned environment and Lean module; verifies minimization artifacts; and replays the complete counterexample-search event chain at the supplied current head. The proposed claim must retain the exact source, satisfy `claim/1`, and have canonical content distinct from the original. Its version hash and the checker binding are derived, not accepted.

The resulting canonical package contains the source and original claim versions, typed witness, checker, complete qualifying refutation witness, minimization, failure explanation, repair operation, proposed repaired claim and hash, and search-run provenance. It is stored as private generated canonical JSON. The package records already revalidated evidence; it creates no new proof or refutation authority.

After CAS staging, one crate-private Store operation opens an immediate SQLite transaction. It rechecks the captured claim/source/formalization/fidelity/authority basis and exact search-run head, then atomically registers the package artifact, creates a new claim object with no predecessor, creates one `research.repairs` edge from the new claim version to the original claim version, and records the idempotency result. Any failure rolls back all logical writes. CAS bytes written before a failed transaction are non-semantic orphans.

Generic edge creation rejects `research.repairs`. CLI and MCP expose repair and retrieval through the same application methods. Retrieval rehashes the package bytes, controlled metadata, canonical lineage, search-chain prefix, fidelity report, publication authority, repaired claim, and unique repair edge.

The original claim, version, evidence, and derived `disproved` status remain untouched. The repaired claim starts independently as `not_started`; it inherits neither fidelity nor proof. Proving the repaired proposition requires a new formalization and its own evidence chain.

## Consequences

- A correction never rewrites the proposition that failed.
- Counterexample, proposed statement, exact checker, evidence locators, run provenance, claim identity, and repair edge can be audited as one closure.
- Dry-run computes the exact package and repaired-version hashes without registering CAS metadata or canonical state.
- Idempotent retry returns the original artifact, claim, and edge; rebinding the key fails.
- A relevant truth-basis or search-head race loses with a retryable conflict.
- Transaction failure can leave only unregistered content-addressed bytes, never a partial logical repair.
- The repaired statement remains unproved until its own formal lifecycle succeeds.

## Rejected alternatives

### Version the disproved claim in place

Rejected because the new proposition is not the proposition addressed by the retained refutation.

### Copy refutation or fidelity evidence to the repaired claim

Rejected because evidence is exact-version and exact-formalization bound. Changed hypotheses or conclusions require new evidence.

### Let callers create repair edges or ingest package JSON

Rejected because caller-selected lineage could bind unrelated evidence, bytes, statements, or search runs.

### Write artifact, claim, and edge independently

Rejected because retries and failures could expose semantically incomplete repairs.
