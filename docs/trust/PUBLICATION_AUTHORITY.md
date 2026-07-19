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

The closed policy, request, candidate-report, and attestation-verification contracts are implemented. A clean-checkout boundary smoke now runs a pinned trivial Lean theorem with one Lean worker thread inside Linux mount, PID, and network namespaces with a four-gibibyte address-space limit. The protected `main` workflow attests the exact smoke-report bytes, challenges that bundle with the independently pinned GitHub CLI verifier, and retains the report, raw verification result, constrained verification record, diagnostics, and Sigstore bundle. Pull-request CI exercises the isolation script without granting authority.

The attestation-verification record is still non-authoritative. It can establish that exact retained bytes were signed by the constrained protected workflow and witnessed by the configured transparency system. Canonical proof authority additionally requires a real `publication_report/1`, complete exact evidence closure, controlled ingestion, and a new authoritative evidence record produced by the application rather than accepted from caller JSON.

No authoritative proof or refutation evidence exists yet. No mathematical claim status is derived.
