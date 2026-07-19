# MathOS 0-to-1 Product Specification

> Superseded: The binding product and implementation contract is now the repository-root `SPEC.md`. This earlier document describes only the legacy Python finite-domain slice and cannot define release completion.

## Product statement

MnehmosAI builds MathOS, a verifier-gated operating system for mathematical research and learning.

MathOS accepts a mathematical claim and carries it through formalization, proof or counterexample search, independent verification, pedagogy, provenance, and reinforcement-learning export. MCP is one interface into the platform, not the product boundary.

## 0-to-1 objective

The first release must demonstrate one complete trusted vertical slice. It must handle a proved claim, a disproved claim, and a claim that remains unresolved without converting uncertainty into success.

The first trusted formal domain is a finite universal claim language. This domain is intentionally small enough to verify exhaustively and strong enough to establish the architecture of generator and verifier separation.

## Required capabilities

1. Accept an informal statement and an optional finite formal specification.
2. Assign a content-derived stable claim identifier.
3. Preserve the original statement and exact formal specification.
4. Search for an enumeration proof or a counterexample witness.
5. Verify the candidate independently from the search engine.
6. Fail closed when the language, verifier, or search budget is insufficient.
7. Maintain explicit claim states and reject invalid transitions.
8. Record a tamper-evident provenance event chain in SQLite.
9. Generate pedagogy whose certainty matches the verified result.
10. Export a replayable JSON reinforcement-learning trajectory.
11. Expose the same application service through a CLI and MCP stdio server.
12. Pass deterministic unit, integration, end-to-end, replay, and adversarial tests.

## Trusted outcomes

| Outcome | Meaning |
| --- | --- |
| verified_proved | The independent verifier established the finite universal claim for every assignment. |
| verified_disproved | The independent verifier checked a valid assignment that violates the universal claim. |
| unresolved | No trusted conclusion was established. This includes budget exhaustion and unsupported formal input. |

Search output alone never changes a claim into a verified state.

## Initial non-goals

- General natural-language autoformalization
- Unbounded theorem proving
- Hosted multi-user deployment
- Commercial model training pipelines
- A graphical interface
- Treating a language model as a verifier

These are later layers. The first release establishes the trusted lifecycle they must use.

## Acceptance fixtures

- Proved: excluded middle over one Boolean variable
- Disproved: universal implication over two Boolean variables
- Unresolved: excluded middle over twelve Boolean variables with a search budget below the complete truth table

## Release condition

Release v1.0.0 only when every item in [ZERO_TO_ONE_DOD.md](ZERO_TO_ONE_DOD.md) has reproducible evidence on the release commit. In this specification, 0-to-1 means the completed path to MathOS 1.0, not a 0.1 preview release.
