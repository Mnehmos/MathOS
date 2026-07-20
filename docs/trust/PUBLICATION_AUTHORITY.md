# Publication Proof Authority

MathOS publication authority is a two-stage boundary.

First, a protected clean-checkout workflow produces a candidate report. Second, a separate ingestion path verifies cryptographic provenance for the exact report bytes before it may create authoritative proof or refutation evidence.

## Candidate stage

The committed `publication_policy/1` binds:

- repository and protected workflow identity;
- required `main` source ref;
- GitHub-hosted runner requirement;
- exact Lean toolchain;
- allowed axiom surface;
- clean-checkout, dependency-closure, network-isolation, and memory-limit requirements;
- SLSA provenance predicate;
- exact action commit identities for attestation and artifact retention.
- GitHub CLI attestation verifier version, archive digest, and executable digest.

The `publication_request/1` binds one exact formalization, intended proof or refutation outcome, diagnostic elaboration evidence, proof-closure evidence, axiom-audit evidence, environment, Lean module, declaration, policy, Git commit, and Git tree.

The `publication_report/1` records the observed controls, axiom surface, workflow identity, run identity, and retained artifact closure. A passed report is valid only when every required control is true and every observed axiom is allowed.

Every candidate report must contain:

```text
authoritative = false
```

A report that says otherwise is invalid.

### Request preparation

The safe local preparation surface is:

```text
mcl verify prepare-publication \
  --formalization-object-id <uuidv7> \
  --formalization-version-hash <sha256> \
  --outcome proof \
  --diagnostic-evidence-id <uuidv7> \
  --proof-closure-evidence-id <uuidv7> \
  --axiom-audit-evidence-id <uuidv7> \
  --source-commit-sha <git-sha1> \
  --source-tree-sha <git-sha1> \
  --actor <identity> \
  --idempotency-key <stable-key>
```

Use `--outcome refutation` for a refutation request and `--dry-run` to validate without writing. MCP exposes the same application operation as `verify.prepare_publication`.

The caller selects exact IDs and publication intent but cannot submit request JSON, evidence hashes, environment, module, declaration, policy, report, or authority fields. A publishable formalization must carry typed `claim_polarity`: `claim` binds a proof request and `negation` binds a refutation request. Omitting this field remains valid only for compatibility with earlier `formalization/1` records; publication preparation then fails closed, so a legacy formalization must be versioned and fully reverified. Changing polarity changes the formalization version identity and invalidates the old evidence chain.

The application requires the current formalization head; rereads immutable diagnostic and audit evidence; verifies that the accepted audit pair came from one job and audited the selected elaboration diagnostic; reopens the controlled job reports; recomputes every referenced CAS object; and derives the remaining request fields from canonical state. A Store transaction rechecks the current head while recording the request and its idempotency receipt, including when identical request bytes already exist. The canonical bytes are retained as a private `generated` JSON artifact whose SHA-256 equals the request hash.

`claim_polarity` is exact canonical intent, not proof that the Lean theorem faithfully represents the claim or its negation. Later authority creation must combine it with current reviewed statement-fidelity evidence and the protected publication closure; it must never infer authoritative proof/refutation kind from the label alone.

The commit and tree values in this local request are proposed source identities. The protected clean-checkout workflow must derive and match the actual runtime commit and tree before producing a candidate. A prepared request is not evidence, is not a publication report, and grants no authority.

## Attestation stage

The protected workflow attests the exact candidate report digest using GitHub OIDC and Sigstore. Authority ingestion must verify at least:

- repository `Mnehmos/MathOS`;
- signer `.github/workflows/publication.yml`;
- source ref `refs/heads/main`;
- exact source commit digest;
- SLSA provenance v1 predicate;
- GitHub-hosted runner;
- exact report subject digest.

MathOS must parse the verifier output and match it to the report and committed policy. Process success by itself is insufficient.

## Current implementation state

The closed policy, request, candidate-report, and attestation-verification contracts are implemented. The shared application, CLI, and MCP paths can now derive and retain a non-authoritative publication request from exact current local evidence without accepting caller-authored JSON. A clean-checkout boundary smoke runs a pinned trivial Lean theorem with one Lean worker thread inside Linux mount, PID, and network namespaces with a four-gibibyte address-space limit. The protected `main` workflow attests the exact smoke-report bytes, challenges that bundle with the independently pinned GitHub CLI verifier, and retains the report, raw verification result, constrained verification record, diagnostics, and Sigstore bundle. Pull-request CI exercises the isolation script without granting authority.

The attestation-verification record is still non-authoritative. It can establish that exact retained bytes were signed by the constrained protected workflow and witnessed by the configured transparency system. Canonical proof authority additionally requires a real `publication_report/1`, complete exact evidence closure, controlled ingestion, and a new authoritative evidence record produced by the application rather than accepted from caller JSON.

No authoritative proof or refutation evidence exists yet. No mathematical claim status is derived.
