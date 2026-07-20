# Publication Proof Authority

MathOS publication authority is a staged boundary.

First, a protected clean-checkout workflow produces a candidate report. A quarantine path then registers the exact report, retained closure, and Sigstore bundle in content-addressed storage without granting canonical artifact provenance. A separate ingestion path verifies current canonical state and cryptographic provenance for those exact hashes. A final atomic evidence gate may then create authoritative proof or refutation evidence for that exact formalization.

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

`claim_polarity` is exact canonical intent, not proof that the Lean theorem faithfully represents the claim or its negation. Authority creation combines it with the fully replayed protected publication closure and derives the evidence kind internally; it never infers authority from the label alone. A later source-claim truth read must separately combine this formal proof authority with current reviewed statement-fidelity evidence.

The commit and tree values in this local request are proposed source identities. The protected clean-checkout workflow must derive and match the actual runtime commit and tree before producing a candidate. A prepared request is not evidence, is not a publication report, and grants no authority.

### Protected candidate and retained closure

The protected workflow refuses to start unless GitHub's immutable run context reports that `main` is actively protected. It then constructs one fresh, real canonical lifecycle for the no-import `MathOS.Publication.smoke` declaration, prepares its request through the same application service, and rebuilds the exact retained module twice inside Bubblewrap: once for elaboration and once with a verifier-controlled `#print axioms` driver. Both subprocesses use a read-only root, separate mount, PID, and network namespaces, one Lean worker thread, a 120-second timeout, a four-gibibyte Lean heap cap, and a six-gibibyte process address-space ceiling for runtime and thread overhead.

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

### Quarantine staging

The filesystem-facing staging surface is CLI-only:

```text
mcl verify stage-publication-candidate \
  --report-file <canonical-report> \
  --retained-closure-file <canonical-closure> \
  --retained-root <contained-candidate-root> \
  --attestation-bundle-file <sigstore-json-bundle> \
  --actor <identity> \
  --idempotency-key <stable-key>
```

Staging requires contained regular files, rejects symbolic-link and traversal substitutions, bounds every input, validates the canonical report and fixed 25-role closure, and atomically records one immutable `publication_stage/1`. The physical bytes use the existing content-addressed store, including exact zero-byte log members. Stage registration is publication-scoped quarantine metadata, not canonical `artifact` provenance, and is always `authoritative: false`. CAS writes that precede a failed database transaction remain harmless unregistered orphans; retry rehashes and reuses them.

Staging an archive in a fresh instance does not import its record, evidence, or job snapshots as canonical state. Ingestion still requires those exact identities to exist and be current in the Store. This prevents a downloaded archive from declaring its own currentness.

### Controlled ingestion

The shared CLI operation is:

```text
mcl verify ingest-publication \
  --report-artifact-hash <sha256> \
  --attestation-bundle-artifact-hash <sha256> \
  --actor <identity> \
  --idempotency-key <stable-key>
```

MCP exposes the same application operation as `verify.ingest_publication`. Neither interface accepts report JSON, closure paths, a verifier executable, verifier arguments, a verification record, or any authority field.

The application resolves the unique immutable stage, rehashes the report, closure, bundle, and every retained member, repeats all semantic candidate validation, and re-derives the publication request against the current canonical Store. It then resolves only the configured `gh`/`gh.exe` name, requires the committed version and executable SHA-256, copies those verified bytes into a fresh private execution workspace, rehashes the copy before and after execution, materializes controlled `.json` inputs, and invokes fixed typed arguments for repository, certificate identity, ref, source and signer digests, SLSA predicate, and self-hosted-runner denial. Standard input is null; time and combined output are bounded; a nonzero exit, timeout, output overflow, or unexpected standard error fails closed.

The current policy pins the official Linux amd64 GitHub CLI 2.96.0 executable. Production ingestion is therefore intentionally limited to the protected GitHub-hosted Ubuntu workflow. Windows can exercise staging and rejection behavior, but `gh.exe` cannot satisfy the Linux binary hash and must fail with `MCL_PUBLICATION_VERIFIER_PIN_MISMATCH`. Supporting another operating system or architecture requires a separately reviewed platform-specific executable pin and policy contract; command-name compatibility alone is insufficient.

The pinned verifier output must be exactly one result. Closed parsing requires the echoed registered bundle; certificate-bound workflow, repository and owner names plus their policy-pinned immutable GitHub numeric IDs, ref, commit, runner, trigger, and run URI; repeated verified identity; exactly one report subject and source dependency; matching SLSA workflow/build/run data; and one to eight verified timestamps including Rekor. Unknown trust-layer fields, key-signed results without the required certificate, multi-result output, missing optional verifier fields, `CurrentTime`, altered subjects, recreated repository or owner identities, or inconsistent predicate data are rejected.

Successful ingestion retains the exact raw verifier output and canonical `publication_attestation_verification/1` attestation-verification record in CAS. One final Store transaction then rechecks that the request's exact formalization is still the current head and writes both the separate immutable SQLite ingestion receipt (`PublicationIngestionReceiptSnapshot` in `publication_ingestion_receipts`) and the logical stage/actor idempotency result. Retry after restart reparses the stored raw output against the exact staged report and bundle, revalidates its CAS closure, repeats the atomic currentness check, and returns the stored ingestion receipt without depending on a new network challenge. The stage, attestation-verification record, and ingestion receipt all remain explicitly non-authoritative.

The canonical CAS attestation-verification record and its SQLite ingestion receipt prove only that the constrained verifier accepted the exact staged provenance. Neither means the report classification is `passed`, and neither establishes mathematical truth. The authority gate therefore uses the application-level replay path, separately requires a `passed` report and every publication control, and never infers authority from receipt-table presence or a Store shape check alone.

### Atomic authority promotion

The only public promotion surface is hash-only:

```text
mcl verify promote-publication-authority \
  --publication-receipt-hash <sha256> \
  --actor <identity> \
  --idempotency-key <stable-key>
```

MCP exposes the same application operation as `verify.promote_publication_authority`. Neither surface accepts a subject, outcome, evidence kind, result, authority class, report, receipt object, artifact list, verifier argument, fidelity verdict, or caller-authored evidence payload.

The receipt hash is only a locator. The application revalidates the immutable receipt and stage projections; rehashes the report, closure manifest, Sigstore bundle, all 25 staged members, raw verifier output, and canonical attestation-verification bytes; repeats the closed candidate, retained-semantic, canonical-Store, request-rederivation, and attestation-parser checks; and explicitly rejects every report classification except `passed`. It derives proof versus refutation from the request outcome already constrained by the current formalization's typed polarity.

`evidence/2` is a separate closed contract so existing `evidence/1` identities remain byte-compatible. It can encode only `accepted` plus `authoritative` `lean_kernel_proof` or `lean_kernel_refutation`. Its nested `publication_authority_binding/1` binds the receipt, stage, report, retained closure, bundle, raw verification, request, and policy hashes. Its artifact list is the sorted unique complete CAS closure. Local run and job UUIDs are null because the protected GitHub workflow identity is external and already content-bound inside the replayed report and receipt.

The Store accepts no evidence payload. Controlled ingestion first projects the exact formalization object and version onto the immutable receipt and proves that they match the registered private generated publication-request artifact retained by the stage. The authority gate then receives one non-deserializable application-derived commit, maps the outcome to the fixed evidence shape, and uses one immediate SQLite transaction to re-read the receipt and stage, recheck that receipt/request/subject relationship plus the current formalization head, environment, and polarity, enforce receipt uniqueness and idempotency, insert the evidence, read it back through integrity validation, and write the idempotency result. Migration 0011 projects the receipt, stage, and receipt-subject bindings and adds database triggers that reject an unbound receipt or any kernel kind, authoritative class, v2 schema, or publication projection outside this exact shape. CAS bytes are replayed before the short database transaction and must be replayed again by every later authority consumer.

Formalization authority and statement fidelity remain deliberately separate. This gate can certify that Lean checked the exact formalization under the protected policy. It does not establish that the formalization faithfully represents its source claim, and it does not derive a source-claim truth status.

## Current implementation state

The closed policy, request, retained-closure, candidate-report, quarantine-stage, attestation-verification, authority-binding, and authoritative-evidence contracts are implemented. The shared application, CLI, and MCP request-preparation paths derive and retain a non-authoritative request from exact current local evidence without accepting caller-authored JSON. The protected `main` workflow retains the earlier boundary smoke and additionally produces, application-validates, attests, independently challenges, stages, ingests, promotes, and retains a real `publication_report/1`, its complete exact closure, raw verifier output, canonical CAS attestation-verification record, separate immutable non-authoritative SQLite ingestion receipt, and receipt-bound authoritative evidence. Pull-request CI exercises the same candidate producer and quarantine persistence, but its deliberately synthetic bundle cannot cross the ingestion or authority boundary.

That workflow capability is now observed on merge `95bd8a1d2068612b5eca644c3d77754b5e4f49fd`. Protected run `29721420136` retained artifact `8452528096`; downloaded inspection recomputed the exact report, 25-role closure, candidate bundle, quarantine stage, raw verifier output, and canonical attestation-verification/receipt hashes. The closed result binds immutable repository ID `1305399818`, owner ID `193347153`, the protected workflow, `main`, merge commit, run and attempt, one report subject, one source dependency, GitHub-hosted execution, and Rekor timestamp `2026-07-20T06:22:33Z`. Every retained authority field remains false.

The attestation-verification record and SQLite ingestion receipt remain non-authoritative. Together they can establish that exact retained report bytes were signed by the constrained protected workflow and witnessed by the configured transparency system. The implemented atomic gate is the only path that can turn that fully replayed closure into authoritative evidence.

No protected merged-tree authority promotion has yet been observed for this implementation candidate. No mathematical claim status is derived.
