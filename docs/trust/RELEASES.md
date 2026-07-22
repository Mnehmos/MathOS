# Portable releases

Portable release construction is an export of already canonical state. It grants no proof, refutation, fidelity, review, or publication authority.

## Build

The build path requires an exact publication ingestion receipt and an exact reviewed learning-unit root:

```text
mcl --root <instance> --json release build \
  --publication-receipt-hash <sha256> \
  --pedagogy-root-object-id <uuidv7> \
  --pedagogy-root-version-hash <sha256> \
  --mode prerequisites \
  --max-depth 8 \
  --limit 100 \
  --profile private \
  --output-dir <new-directory>
```

`--mode` is `prerequisites` or `recommended`; only prerequisite mode accepts `--include-soft`. The destination's parent must exist, and the destination itself must not exist. Builds do not overwrite or merge directories.

The application fully revalidates the receipt-bound protected publication chain and the current reviewed pedagogy path before writing. A stale source, claim, formalization, unit, edge, artifact, environment, stage, receipt, or policy fails closed.

Add `--dry-run` to execute the same derivation and validation without creating the output directory. The returned `dry_run: true` manifest hash must equal a later build from unchanged state, including after reopening the instance.

Private releases retain explicit member restrictions. A public profile additionally requires every member to be public and licensed. Selecting `public` never downgrades a restriction or invents a license.

## Verify without an instance

Verification does not load `mcl.toml`, create an instance, or open SQLite:

```text
mcl --root <missing-path-is-allowed> --json release verify \
  --bundle-dir <copied-release-directory> \
  --expected-manifest-hash <trusted-sha256>
```

The expected hash is required out of band so a coherent replacement manifest cannot silently redefine the release. The verifier checks the exact file inventory, rejects symbolic links and unsafe paths, recomputes every member hash and size, parses exact canonical JSON, validates record hashes and schemas, resolves all object and edge references, verifies the persisted authority and current fidelity witnesses, artifacts, environment identities, and controlled repair graph, reproduces the publication report/closure/stage/receipt bindings, compares report copies with their CAS members and the license index, and checks the replay and pedagogy exports against the manifest.

Only after those checks pass does it replay `replay/Submission.lean`. The executable is fixed to `lean` (or `lean.exe` on Windows), the only argument is the verifier-controlled module path, and the declaration comes from the receipt-bound manifest. The pinned environment controls toolchain, platform, network flag, timeout, and output limit. A platform or Lean-version mismatch fails closed.

The returned `manifest_hash` is the SHA-256 of the exact canonical `manifest.json` bytes. It must remain identical after copying the bundle.

## Project into MathCorpus and MCIP

`mcl release export` is a deterministic child projection of a verified portable release. It
requires the trusted source-release manifest hash plus explicit packet ID, domain, level, and
difficulty. It does not open SQLite, contact MathCorpus, infer a leakage split, or grant proof,
training, or publication authority.

Private releases produce only `private_audit_only` packets and `private_only` MCIP records. Public
releases remain `quarantined` and require explicit public redistribution, redaction, and license
authority. `mcl release verify-export` requires a separately trusted export-manifest hash and an
independently supplied frozen release, then checks the closed tree and pinned offline schemas and
requires byte-identical deterministic reprojection.

The full interface and output contract are documented in
[MathCorpus and MCIP export](../implementation/CORPUS_EXPORT.md), with the trust decision in
[ADR-0012](../decisions/ADR-0012-deterministic-offline-mathcorpus-mcip-projection.md).

## Trust boundary

- A successful build means the exported directory matched canonical state at build time.
- A successful offline verification means the copied directory is internally complete and its Lean artifact replays under the declared toolchain.
- Neither result creates authority. Authority remains the exact protected receipt-bound `evidence/2` chain included in the release.
- Transport ZIP or tar hashes may be retained operationally, but they are not the canonical release identity.
