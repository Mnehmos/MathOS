# ADR-0002: RFC 8785 canonical JSON with safe integers

Status: Accepted

Date: 2026-07-19

Issue: [#5](https://github.com/Mnehmos/MathOS/issues/5)

## Context

Record versions, environments, release manifests, and event chains require byte-identical identities across processes and machines. Ordinary JSON does not define object-key order, whitespace, or one numeric representation. Ad hoc recursive sorting would not fully define floating-point formatting or cross-language behavior.

RFC 8785 defines the JSON Canonicalization Scheme for cryptographic identity. Its number model follows ECMAScript and IEEE-754 binary64. Raw integers outside the exact binary64 range can silently lose mathematical precision during canonicalization.

## Decision

MathOS canonical JSON uses RFC 8785 through `serde_json_canonicalizer` 0.3.2.

Before canonicalization, the engine recursively rejects integer JSON numbers outside `[-9007199254740991, 9007199254740991]`. Exact larger integers must be represented as explicitly typed strings in their owning schema.

Rust strings provide valid UTF-8. Canonicalization preserves their Unicode scalar sequences without normalization. Canonically equivalent human text with different Unicode normalization forms therefore has different content identities unless an owning schema explicitly normalizes that field before submission.

Record versions use:

```text
SHA256(schema_version || NUL || RFC8785(payload))
```

Timestamps, actors, local paths, database rows, and machine information are not part of this identity.

## Consequences

- Other implementations can reproduce identities with a standard cross-language algorithm.
- Key insertion order, insignificant whitespace, and ordinary numeric spellings do not change identity.
- Schemas must distinguish exact large integers from JSON binary64 numbers.
- Unicode normalization is visible rather than silently changing mathematical or source text.
- Golden vectors and boundary tests are required before changing the canonicalization dependency.

## Rejected alternatives

### Serde JSON output alone

Rejected because stable Rust map behavior does not define the complete cross-language number and string contract.

### Normalize every Unicode string

Rejected because normalization can change source fidelity and identifiers. Normalization belongs to explicit field semantics, not the storage primitive.

### Accept arbitrary precision JSON numbers

Rejected because RFC 8785 canonicalizers convert through binary64 and could produce an identity for a rounded value rather than the submitted exact integer.
