# ADR-0013: RL and evaluation splits are component-locked frozen-release projections

Date: 2026-07-22

Status: accepted

## Context

Row-random dataset splitting can put a theorem, an equivalent statement, its source paper, a
certificate sibling, or a proof variant on both sides of an evaluation boundary. A frozen
MathOS release contains exact evidence for some of those relationships, but semantic
equivalence, generated families, benchmark identity, and publication chronology cannot all be
reliably inferred from proof text.

RL data is also derived data. It must not become canonical state, turn diagnostic traces into
proof, weaken release licensing, or make a private release public or trainable.

## Decision

MathOS accepts a closed `rl_export_plan/1` cohort plan. Every named source release is bound by
its expected manifest SHA-256, one of `train`, `validation`, `public_test`, or
`held_out_evaluation`, a benchmark identity, and explicit nonempty labels for theorem-dependency,
equivalent-formalization, shared-source, certificate-family, and proof-variant relationships.
The signed publication receipt, not caller text, supplies the exact publication date.

The exporter first structurally verifies every release. The protected producer separately runs
the release's platform-bound Lean replay immediately before projection while SQLite is hidden.
The exporter then adds derived keys for shared exact records, source locators and content,
claim/formalization identity, proof modules and
declarations, typed dependency/equivalence/repair edges, and counterexample run/package lineage.
Any releases sharing a declared or derived key are unioned into one leakage component. Every
component must have exactly one split. Training dates must be at or before the plan cutoff;
validation, public test, and held-out dates must be later.

Private releases are permitted only in `held_out_evaluation`. Train, validation, and public test
therefore require a release that already passed the complete public-release member and license
policy. Task and evidence members retain explicit licenses and restrictions.

`mcl release export-rl` emits only tasks backed by exact current fidelity plus receipt-bound
kernel authority. Version 1 projects formalization, fidelity selection, counterexample, statement
repair, declaration retrieval, and proof generation. Reviewed split-eligible pedagogy can also
produce explanation and curriculum-ordering tasks. All twelve SPEC task families appear in the
family audit; missing evidence and the four not-yet-projected families have explicit skip reasons.
Private chain-of-thought fields are rejected and never treated as proof evidence.

`mcl release verify-rl-export` requires the trusted export-manifest hash, an independent plan, and
the named frozen releases. It revalidates their closed semantic inventories, recomputes the
complete projection, and requires byte-identical output without SQLite or network access. Lean
replay remains the preceding platform-bound `mcl release verify` gate, not a hidden portability
requirement of the derived export.

## Consequences

- A row cannot be moved independently of its dependency/equivalence/source/certificate/proof/
  benchmark component.
- Caller-declared relations supplement exact derived identities but cannot replace them.
- Publication chronology is receipt-bound and mechanically enforced.
- Private Pilot A can exercise the evaluation path without becoming training data.
- The export is portable derived evidence; the canonical store and frozen releases remain truth.
- Decomposition, proof repair, generalization, and frontier selection remain explicit future
  projections until their required source evidence exists.

## Rejected alternatives

### Random or task-ID-based splitting

Rejected because cryptographically distinct rows can still disclose the same mathematics.

### Infer every relation from theorem text

Rejected because semantic equivalence, benchmark membership, and certificate family are review
decisions, not reliable string properties.

### Allow private training with a plan flag

Rejected because a split label cannot manufacture redistribution, redaction, or training rights.

### Verify only internal export hashes

Rejected because a coherent substituted export can redefine its own manifest. Verification also
requires a trusted hash and exact offline reprojection from independently supplied sources.
