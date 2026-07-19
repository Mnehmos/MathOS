# Canonical Artifacts

An artifact is immutable content plus validated metadata. Its identity is the SHA-256 hash of its raw bytes. The database records what the bytes are, where they came from, and whether they may be exported. It does not decide whether the bytes prove anything.

## Trust boundary

Artifact ingestion performs these steps:

1. decode the closed `artifact_metadata/1` object;
2. validate media type, size, license, restriction, and bounded semantic metadata;
3. validate the bytes for the declared media type;
4. compute the SHA-256 content identity;
5. write the bytes atomically into the content-addressed store;
6. register immutable metadata and actor attribution in SQLite;
7. retain an idempotency receipt for safe retries.

Semantic metadata cannot contain `proved`, `disproved`, `faithful`, `certified`, or `authoritative`. Those conclusions require typed evidence and remain outside the artifact record.

## CLI workflow

The input file must be a regular file inside the configured instance root. Symbolic links and paths outside that root fail closed.

```text
mcl artifact ingest \
  --input-file Formalization.lean \
  --metadata-json '{"schema_version":"artifact_metadata/1","media_type":"text/x-lean","creation_source":"user_ingest","license_expression":"PolyForm-Noncommercial-1.0.0","restriction":"restricted","semantic_metadata":{"declaration_name":"MathOS.Example"}}' \
  --actor operator-name \
  --idempotency-key artifact-formalization-1
```

Add `--dry-run` to validate bytes and metadata and predict the exact hash without writing the CAS or database.

```text
mcl artifact get --artifact-hash <sha256>
mcl artifact list --limit 20
mcl artifact verify --artifact-hash <sha256>
```

Verification rereads the bytes from CAS, recomputes their hash, validates their media representation, and compares their size with canonical metadata. It establishes storage integrity only.

## Formalization gate

A formalization may name a module artifact only when that exact hash is registered with media type `text/x-lean`. A missing artifact and an artifact of another media type both fail before the formalization becomes canonical state.

The later Lean worker will materialize verified bytes into a fresh temporary workspace using a verifier-selected plain file name. Materialization rejects path components, symbolic workspace roots, and overwrites.

## Orphan policy

CAS bytes are written before database metadata so a committed database row never points at missing bytes. A process failure after the atomic CAS write but before the SQLite commit can leave an unregistered content-addressed file. That file is an orphan, not canonical state, and is invisible to artifact lookup and formalization.

Orphans are safe to retain because their paths are determined only by content hashes and writes are idempotent. Release construction selects artifacts from canonical database references and never scans CAS as authority. Destructive garbage collection is intentionally absent until a reviewed reachability and backup policy exists. The initialization and doctor canary is also intentionally unregistered.

## Persistence and correction

Migration 0007 adds actor attribution and immutable update and delete triggers to artifact metadata. Corrections require a new artifact or new metadata policy operation. Existing rows are not silently rewritten.

Every metadata read validates the stored closed object and compares its indexed columns. Any disagreement reports `MCL_ARTIFACT_INTEGRITY_FAILED` and requires quarantine or verified restore.
