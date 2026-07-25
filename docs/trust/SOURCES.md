# Exact source content

A canonical `source/1` record describes provenance and may optionally bind exact retained bytes
through `content_hash`. That hash is an integrity and portability reference only. It does not prove
that the bytes are faithful to a paper, correct, complete, or mathematically authoritative.

## Null and non-null bindings

`content_hash: null` remains valid. It means MathOS has provenance text but no exact source-content
artifact to verify or carry into a release.

A non-null `content_hash` must resolve to the exact SHA-256 bytes in the local content-addressed
store. The registered immutable `artifact_metadata/1` must satisfy all of these conditions:

- `semantic_metadata.source_role` is exactly `source_content`;
- `creation_source` is `user_ingest`, `import`, or `migration`, never generated, verifier-produced,
  or human-review output;
- a source license, when present, exactly matches the artifact license; and
- a public source with allowed redistribution has a public artifact and the same resolved license.

The application re-reads and hashes the CAS bytes before every source create or version proposal,
including dry runs and idempotent retries. A missing, changed, unregistered, wrongly classified, or
policy-incompatible artifact returns `MCL_SOURCE_CONTENT_INVALID` before state changes.

The `original_text` field remains canonical source context; it is not silently treated as the
artifact bytes. Callers that need byte identity must use the non-null `content_hash`.

## Store defense and migration

Migration `0013_source_content_closure.sql` independently rejects direct `record_versions` inserts
whose non-null source hash does not resolve to compatible immutable artifact metadata. It also
checks every existing source version while upgrading. An incompatible legacy binding blocks the
migration instead of being silently promoted to reviewed source content.

SQLite cannot prove that the CAS file still exists or matches its name. The application performs
that byte check before mutation, and portable release construction repeats it while materializing
the closure.

## Portable releases

Release construction adds every non-null source content hash in the exact object-reference closure
to `artifacts/<sha256>`. The manifest retains its artifact metadata and policy. Database-free
verification requires the exact artifact path and hash, requires retained metadata, and reruns the
same role, provenance, license, and redistribution policy checks.

Removing the operational database therefore does not remove the source bytes or the evidence
needed to decide whether they were eligible for the release. A source with `content_hash: null`
adds no invented artifact member.

## BH Pilot C fixture

`fixtures/bh_formalization/SOURCE-LOCK.md` is copied byte-for-byte from
`Mnehmos/BHFormalization` commit `8875bef812375d7e6fd00bff95764cd5c057c7fe`, tree
`600f3e1230ca88f9784837933e2417c319aaad3e`. The GitHub content object is blob
`c7e3805ab78327f1cd561421e2c0168a009eb161` with 1,809 bytes. Its SHA-256 is
`249f62a4e2780235239cf9f4e781f274f8dadba7f3aeba3aab51f46b0a6fc5c5`.
The earlier
`c82096766a9df350fc2584cdabd0e18334348a1a8adbd4fc7ec24d75ff2994ee`
discovery digest was a CRLF Windows worktree rendering, not the pinned Git object, and is
deliberately not used as the portable identity.
The adjacent Apache-2.0 `LICENSE` and `NOTICE` preserve upstream terms and attribution.

This fixture tests exact source ingestion and database-independent release closure. It is not the
BH paper, the paper's reproducibility certificate, or a fidelity, Lean-kernel, Comparator, or
mathematical-status witness.
