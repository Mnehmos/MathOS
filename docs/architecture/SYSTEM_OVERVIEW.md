# System Overview

## Architectural rule

MathOS has one application core with multiple interfaces. CLI and MCP calls must use the same Claim Engine service and must not duplicate trust logic.

## Components

| Component | Responsibility |
| --- | --- |
| Mathematical Claim Engine | Claim identity, lifecycle, orchestration, and state transitions |
| Proof Search | Produces candidate enumeration proofs, counterexamples, or unknown results |
| Independent Verifier | Recomputes proof obligations or validates witnesses without trusting search output |
| MathCorpus layer | Stores claim records, provenance events, pedagogy, and trajectory material |
| Provenance Ledger | Hash-chained SQLite event log and replay verification |
| Pedagogy | Explains only what the verifier established |
| RL Export | Emits versioned, machine-readable claim trajectories with verifier evidence |
| Interfaces | CLI and MCP stdio adapters over the application core |

## Trust boundary

Proof Search is untrusted. It may be heuristic, model-driven, or adversarial. Only the verifier may authorize a verified claim state.

The initial finite verifier exhaustively evaluates a versioned JSON expression language. It does not trust search certificates, but search and verification currently share the same expression semantics implementation. An independent test oracle reduces common-mode risk without eliminating it. A later Lean adapter invokes an external Lean toolchain and fails closed when Lean is unavailable.

The provenance hash chain detects accidental corruption and incomplete rewriting. It is not a digital signature. An attacker with unrestricted database write access can replace events and recompute the chain. Authenticated provenance requires signed checkpoints or an externally anchored digest.

## Data flow

1. Canonicalize the submitted claim and derive its identifier.
2. Persist the claim and submission event.
3. Produce an untrusted search candidate.
4. Independently verify the exact candidate against the exact formal specification.
5. Atomically persist the candidate, verification, allowed state transition, and pedagogy in that event order.
6. Export a trajectory containing verifier evidence and a compact link path through the complete global event chain.

## Dependency policy

The 0-to-1 core uses only the Python standard library. This minimizes installation cost, supply-chain exposure, and token spent debugging framework integration. External verifier and interface SDKs can be added only when they materially reduce risk or maintenance.

The complete authority boundary and residual risks are documented in [Trust Model](TRUST_MODEL.md).
