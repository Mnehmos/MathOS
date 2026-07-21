# ADR-0008: Claim research status is a live polarity-aware derivation

Date: 2026-07-20

Status: accepted

## Context

ADR-0007 established protected, receipt-bound proof or refutation authority for one exact formalization version. That authority answers whether the pinned formal declaration passed the committed kernel and publication controls. It does not answer whether the declaration faithfully states an informal source claim. MathOS keeps those questions on independent evidence axes.

The existing `fidelity_review_request/1` and `fidelity_review_report/1` contracts bind exact source, claim, and formalization versions, but they do not record whether the reviewer compared the declaration with the source claim or with its logical negation. Publication authority now maps immutable formalization polarity `claim` to `lean_kernel_proof` and polarity `negation` to `lean_kernel_refutation`. Treating an old v1 fidelity record as a review of either relation would silently change the meaning of already hashed canonical bytes.

Research status is also current-state dependent. A valid historical proof can stop applying when its claim, formalization, fidelity head, policy, environment, dependency, artifact, receipt, or authority chain is superseded or becomes unavailable. Persisting `proved` or `disproved` as a caller-set field would conceal those dependencies and create a direct truth-mutation path.

## Decision

MathOS derives `claim_research_status/1` live and read-only from one caller-supplied exact claim identity. The caller cannot supply a proposed status, formalization selector, fidelity selector, authority selector, actor, idempotency key, or dry-run flag. CLI and MCP call the same application service. Derivation creates no canonical state.

The response uses the complete SPEC research-status vocabulary, but this service initially derives only `superseded`, `not_started`, `open`, `ambiguous`, `proved`, and `disproved`. It does not infer `active`, `conditionally_resolved`, or `malformed` until closed evidence semantics for those states exist. Terminal truth is represented by exact qualifying witness identities, never by an independent Boolean.

### Polarity and compatibility

The v1 fidelity request and report contracts, their JSON schemas, serialized bytes, and hashes remain unchanged. A current verified v1 review may qualify only a `claim`-polarity formalization paired with authoritative `lean_kernel_proof` evidence. It can never qualify a refutation.

Closed `fidelity_review_request/2` and `fidelity_review_report/2` contracts add required `reviewed_source_relation: claim | logical_negation`. The report repeats the relation and must equal its embedded request. To qualify, the reviewer-authored relation must also equal the immutable formalization polarity and agree with authority kind:

- `claim` + formalization polarity `claim` + `lean_kernel_proof` may witness `proved`;
- `logical_negation` + formalization polarity `negation` + `lean_kernel_refutation` may witness `disproved`.

No inversion, contrapositive, repaired statement, weakened theorem, or changed hypothesis is inferred. A changed declaration or hypothesis is a new formalization version whose evidence must be established independently.

### Currentness and replay

The service first schema-validates and canonically rehashes the exact claim and source records. If the requested claim version is not its object's current head, it returns `superseded` without allowing historical witnesses to affect current truth.

For a current claim, the Store enumerates every bounded current formalization head that names that exact claim version in deterministic order. The caller cannot select a convenient variant. No current formalization yields `not_started`.

For every current formalization, the service revalidates its canonical identity and source lineage. It considers only the current, unique, unsuperseded, non-stale, accepted, role-separated, verified fidelity head. The fidelity report and every supporting artifact are loaded and rehashed from CAS. Stored database projections are locators, not authority.

Likewise, stored authoritative evidence qualifies only after the service replays the full ADR-0007 chain: exact `evidence/2`, receipt, stage, passed report, protected request, committed policy, pinned environment and dependencies, complete 25-role retained closure, bundle, raw verifier output, and every referenced CAS member. The formalization, environment, declaration, outcome, evidence kind, receipt, and artifact identities must all agree with current Store state.

The initial Store basis is one SQLite read snapshot and includes the exact source reference plus its current head, the claim head, the complete formalization-head set, relevant fidelity identities, and authority identities. A superseded claim does not enumerate historical dependent evidence, so unrelated historical volume or corruption cannot override the required `superseded` precedence. After bounded CAS replay, the service takes and compares a fresh basis. A concurrent relevant change returns a retriable conflict rather than a result computed from mixed snapshots.

Missing or corrupt canonical projections, CAS artifacts, report members, or authority links are integrity failures. They fail the read closed and never degrade silently to `open` or a nonqualification reason.

### Aggregation, ambiguity, and history

Status precedence is deterministic:

1. a non-current requested claim version is `superseded`;
2. a current claim with no current formalization is `not_started`;
3. a current formalization whose fidelity preserves source variants or leaves source ambiguity unresolved makes the claim `ambiguous`;
4. qualifying proof and refutation witnesses on distinct current formalizations together are `ambiguous` and both witness sets are returned;
5. one or more qualifying proofs and no qualifying refutation yields `proved`;
6. one or more qualifying refutations and no qualifying proof yields `disproved`;
7. otherwise the claim is `open`, with deterministic explicit nonqualification reasons.

`resolved_from_source` may qualify because it is an explicit role-separated review of the exact source/claim/formalization bridge. `preserved_variants` and `unresolved` may not. Claim ambiguity notes alone do not override `not_started` when no formalization exists.

Proof and refutation authority for the same exact formalization is an integrity contradiction and fails closed; it is not reported as ordinary ambiguity. Distinct current formalization variants can legitimately conflict, so the response reports `ambiguous` with exact proof and refutation witnesses.

Conditional results remain attached to their own exact explicitly assumed claims. A result for a repaired, strengthened, weakened, or conditional claim never changes the original claim. Historical claim/formalization/fidelity/authority versions remain auditable but cannot qualify the current derivation.

The response sorts and deduplicates witnesses and nonqualifications by stable exact identities. Each witness names the formalization version, proof/refutation kind, reviewed relation, fidelity schema version, fidelity evidence UUID and hash, fidelity report artifact hash, authority evidence UUID and hash, and publication receipt hash.

## Consequences

- No database status column, cache, trigger verdict, mutation receipt, `mark_proved`, or caller-authored truth payload exists.
- Existing fidelity v1 canonical identities keep their original meaning and can support only source-claim proof.
- Source disproof requires a new explicit v2 logical-negation review in addition to protected refutation authority.
- Every status read can be more expensive than a projection lookup because it replays all current qualifying chains and their CAS closures.
- Results remain deterministic across restart and expose enough exact evidence identity for an auditor to reproduce the derivation.
- Supersession or loss/corruption of a required current artifact is visible immediately instead of leaving stale terminal truth.
- Empirical search, raw model output, local diagnostics, failed proof attempts, workflow success, and unverifiable replay never become truth authority.

## Rejected alternatives

### Reinterpret fidelity v1 as polarity-neutral

Rejected because it would retroactively change the semantics of existing canonical bytes and could turn a source-claim review into support for source disproof.

### Infer logical negation from authority kind alone

Rejected because kernel refutation authority says what the exact formal declaration establishes, not whether that declaration faithfully represents the source claim's logical negation.

### Let callers select a formalization or evidence witness

Rejected because selecting only a convenient variant could hide a current contradiction, stale head, or disqualifying ambiguity.

### Persist the derived status

Rejected because truth applicability depends on multiple independently current chains. A stored verdict would become stale and would create a direct mutation surface.

### Treat missing or corrupt evidence as merely open

Rejected because absence of a qualifying witness is different from failure to verify the evidence universe being queried. Silent degradation would turn integrity loss into a false mathematical conclusion.

### Let a conditional or repaired result update the original claim

Rejected because assumptions and statement identity are part of the proposition. Such a result belongs to a new exact claim or formalization lineage.
