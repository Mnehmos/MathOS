# ADR 0001: Finite Verifier First

## Status

Accepted for v0.1.0.

## Context

The development environment does not currently contain Lean. MathOS still needs a real independent verifier for its first vertical slice. A mocked verifier would violate the product trust model.

## Decision

Implement a finite universal claim language with exhaustive independent verification. Proof Search and verification remain separate modules. The verifier recomputes every proof obligation or validates the exact counterexample witness.

## Consequences

- The first release can make rigorous claims within a deliberately bounded domain.
- The architecture is ready for Lean without pretending Lean is installed.
- Unbounded or unsupported claims remain unresolved.
- Finite enumeration limits must be explicit and tested adversarially.
