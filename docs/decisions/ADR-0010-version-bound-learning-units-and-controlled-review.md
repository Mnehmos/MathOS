# ADR-0010: Learning units are version-bound records with controlled review

Date: 2026-07-21

Status: accepted

## Context

Pedagogy in MathOS must be machine-readable, grounded in canonical mathematics, and safe to project into future training data. Free-form generated prose cannot establish its own source, license, review, or eligibility. A mutable lesson would also make it impossible to tell which explanation, exercise, or misconception a curriculum edge actually reviewed.

The existing canonical store already provides immutable record versions, exact graph endpoints, content-addressed artifacts, compare-and-swap heads, actor attribution, idempotency, deterministic traversal, and hard-prerequisite cycle rejection. Pedagogy should use those mechanisms instead of adding a second store or authority model.

## Decision

MathOS defines a closed Rust-owned `learning_unit/1` payload and matching committed JSON Schema. Every unit records one exact current claim or concept target, audience track, entry assumptions, objectives, distinct hard and soft prerequisites, exact grounded sources, one verified content artifact, typed related-unit references, optional exact formalizations, review data, a license field, and a closed training status. The thirteen unit kinds are the vocabulary required by the product specification.

Authored proposals and revisions can be only `draft` and `ineligible` or `quarantined`. The separate review operation creates another immutable version, binds the reviewer to the mutation actor, records nonempty notes, and may decide `reviewed` or `rejected` plus the training status. Rejected content cannot become eligible. A review is pedagogical quality metadata only; it creates no mathematical evidence and grants no proof, refutation, fidelity, or publication authority.

Validation resolves every reference to an exact current canonical version and checks the expected kind. Related-unit fields additionally require the referenced unit kind named by the field. Content bytes are rehashed from CAS and must be text or JSON marked `artifact_role=learning_unit_content`. The unit and artifact licenses must agree exactly. Training eligibility requires reviewed state and a resolved license. Public eligibility additionally requires public content, publicly redacted grounded sources, allowed redistribution, and resolved source licenses.

Pedagogy links reuse exact immutable graph edges and accept only a closed `{rationale}` payload. Hard and soft prerequisite links must be declared by the exact source learning-unit payload. Semantic links must target the unit's declared claim or concept. Recommended-next links join exact learning-unit versions. Generic application edge creation applies the same validation, so the dedicated interface is not a bypassable policy wrapper.

Review changes a unit's version hash. Consequently, prerequisite edges for a reviewed version are created after review and must match that reviewed payload before validation or curriculum use succeeds. This deliberately exposes incomplete multi-step authoring state while keeping export-facing validation fail closed. A future transactional authoring convenience may compose those steps without weakening exact-version identity.

Curriculum queries expose two deterministic bounded modes. Prerequisite mode follows outgoing hard prerequisites, optionally includes soft prerequisites, and orders deeper prerequisites before the requested unit. Recommended mode follows outgoing `recommended_next` edges from the requested unit. Every returned unit must be current, fully valid, and reviewed.

CLI and MCP expose propose, version, get, validate, review, link, and path through these same application methods.

## Consequences

- Pedagogy content, grounding, review, licensing, and training status are immutable and independently auditable.
- A moved source, target, formalization, related unit, or prerequisite head invalidates the dependent unit until it is intentionally rebased.
- Review cannot silently certify mathematical truth.
- Training eligibility fails closed against both canonical source policy and exact artifact metadata.
- Payload prerequisites and graph prerequisites cannot silently diverge on a valid unit.
- Hard-cycle rejection remains enforced in both the Store path and SQLite, while soft and recommended paths may branch.
- Authoring may temporarily produce a reviewed unit whose new exact-version edges have not yet been linked; validation and path queries reject it until complete.

## Rejected alternatives

### Store pedagogy as generated prose on a claim

Rejected because prose alone has no typed audience, objectives, prerequisites, review transition, licensing policy, or stable graph identity.

### Treat review as mathematical evidence

Rejected because pedagogical quality review is not proof, refutation, or statement fidelity.

### Keep mutable review fields outside canonical identity

Rejected because callers could no longer determine which exact content version received the decision.

### Infer prerequisites only from payload arrays or only from edges

Rejected because the required unit contract needs explicit machine-readable prerequisites, while curriculum traversal and cycle enforcement need graph edges. Validated equality keeps the two projections honest.
