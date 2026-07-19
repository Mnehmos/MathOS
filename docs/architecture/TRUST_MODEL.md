# Trust Model

## What MathOS may claim

MathOS v0.1 may authorize certainty only for a formal claim accepted by the finite verifier. A proved result means exhaustive evaluation succeeded for every assignment in the declared finite domain. A disproved result means the verifier checked a concrete in-domain assignment that makes the claim false. Every other result is unresolved.

The informal statement is descriptive context. The v0.1 system does not prove that the formal specification faithfully expresses the author's natural-language intent.

## Authority boundary

| Component | Trusted for | Not trusted for |
| --- | --- | --- |
| Proof Search | Proposing a candidate | Authorizing proof or disproof |
| Finite Verifier | Checking supported finite claims | Unbounded mathematics or intent alignment |
| Claim Engine | Enforcing lifecycle transitions | Creating mathematical evidence |
| Provenance Ledger | Detecting ordinary corruption and broken chains | Resisting an attacker who can rewrite and rehash the database |
| Pedagogy | Restating verified status and evidence | Increasing the certainty of a result |
| RL Export | Preserving linked trajectory evidence | Establishing truth independently of the verifier |
| Lean Adapter | Failing closed when Lean is absent | Claiming Lean verification in v0.1 |

## Residual risks

1. Search and verification are separate components, and verification recomputes instead of trusting the candidate. They currently share one expression semantics implementation, so an implementation bug could affect both. A deterministic randomized test compares 200 generated Boolean expressions with a separate reference oracle. A different verifier implementation or Lean kernel should become the stronger long-term boundary.
2. SHA-256 event chaining is tamper-evident, not authenticated. Full database write access permits rewriting and rehashing history. Signed checkpoints or external digest anchoring are required before making adversarial provenance guarantees.
3. Resource limits intentionally turn large, deep, unsupported, or unbounded work into unresolved results. They are safety boundaries, not mathematical conclusions.
4. The hand-written MCP adapter covers the declared protocol surface and is tested as a subprocess. Broader client interoperability remains a release-following integration task.
5. Outputs and trajectories can contain user-provided text. Consumers must continue treating those fields as data.

## Fail-closed rules

- Unknown operators, malformed values, mixed domain types, invalid branches, and exhausted budgets cannot authorize certainty.
- Missing external verifier tooling returns unresolved.
- A candidate is persisted before verification and can never certify itself.
- A verified claim cannot transition back to a weaker state.
- Invalid provenance or trajectory evidence causes validation failure.
