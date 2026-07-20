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

### Protected candidate and retained closure

The protected workflow refuses to start unless GitHub's immutable run context reports that `main` is actively protected. It then constructs one fresh, real canonical lifecycle for the no-import `MathOS.Publication.smoke` declaration, prepares its request through the same application service, and rebuilds the exact retained module twice inside Bubblewrap: once for elaboration and once with a verifier-controlled `#print axioms` driver. Both subprocesses use a read-only root, separate mount, PID, and network namespaces, one Lean worker thread, a 120-second timeout, and a four-gibibyte address-space limit.

The request-bound environment remains truthfully marked `local` because it describes the diagnostic and audit evidence created by the ordinary worker. Its manifest contains the exact Lean toolchain, no dependencies, no imports, and the checked-in `lean-toolchain` hash. The candidate report separately records the stronger GitHub-hosted runner, clean checkout, dependency closure, network isolation, and memory controls actually observed by the protected workflow. A local trust-profile label is never upgraded in place.

`publication_retained_closure/1` has exactly 25 sorted roles with fixed lowercase paths. It retains the request; source, claim, and formalization snapshots; environment; Lean module; publication and audit policies; diagnostic and audit evidence; terminal jobs, reports, and local logs; and protected rebuild, Lean parser-derived dependency, and axiom-audit logs. Every entry binds a semantic identity and the SHA-256 of its exact bytes. The candidate report contains the sorted unique set of all member hashes plus the canonical closure-manifest hash; it cannot include its own hash without creating a recursive identity.

Before attestation, the workflow invokes:

```text
mcl verify validate-publication-candidate \
  --report-file <canonical-report> \
  --retained-closure-file <canonical-closure> \
  --retained-root <contained-output-root>
```

The application requires exact canonical JSON, bounded contained regular files, fixed paths with no symbolic-link components, and byte hashes for every retained member. It replays the source-to-claim-to-formalization references, environment and policy hashes, all evidence identities, terminal job/report/log closures, the pinned Lean `--deps` output, and both local and protected axiom outputs. For this no-import contract, the environment and formalization manifests must both be empty and protected discovery may contain only Lean's implicit pinned `Init.olean`. It then dry-run re-derives the request from current Store and CAS state and verifies the registered request artifact. The validator creates no canonical record or artifact, performs no promotion, and always returns `authoritative: false`; opening an instance may create ordinary operational directories or SQLite WAL files.

The sandbox clears the inherited process environment and applies wall-clock, address-space, and output-file limits. A bounded structured attempt summary, per-execution classifications, and available CAS bytes are retained outside the authoritative closure on both success and failure so a rejected build is not erased.

Pull-request CI exercises this producer in an explicitly `simulated-main`, non-attested context. Only the protected `main` workflow may attest the resulting candidate report.

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

The closed policy, request, retained-closure, candidate-report, and attestation-verification contracts are implemented. The shared application, CLI, and MCP request-preparation paths derive and retain a non-authoritative request from exact current local evidence without accepting caller-authored JSON. The protected `main` workflow retains the earlier boundary smoke and additionally produces, application-validates, attests, independently challenges, and retains a real `publication_report/1` and its complete exact closure. Pull-request CI exercises the same candidate producer without attestation or authority.

The attestation-verification record is still non-authoritative. It can establish that exact retained report bytes were signed by the constrained protected workflow and witnessed by the configured transparency system. Canonical proof authority still requires controlled ingestion that revalidates the downloaded closure and attestation together, followed by a new authoritative evidence record produced only by that application path rather than accepted from caller JSON.

No authoritative proof or refutation evidence exists yet. No mathematical claim status is derived.
