# ADR-0006: Publication authority requires external OIDC-attested provenance

Date: 2026-07-19

Status: accepted

## Context

The local verifier and audit workers deliberately produce diagnostic evidence only. A successful Lean process, a clean source scan, and an acceptable axiom surface still cannot prove that the result came from a protected clean checkout or that required isolation and retention controls were applied.

A local flag such as `GITHUB_ACTIONS=true`, a caller-authored JSON report, or a repository secret checked by the same process would be forgeable by the party seeking promotion. None is a sufficient authority boundary.

GitHub artifact attestations bind an artifact digest to a workflow identity using an OIDC token and a short-lived Sigstore signing certificate. GitHub documents repository, signer-workflow, source-ref, source-digest, predicate-type, and GitHub-hosted-runner constraints in `gh attestation verify`.

## Decision

The protected `.github/workflows/publication.yml` workflow will create a canonical `publication_report/1` candidate after clean-checkout verification. The candidate always carries `authoritative: false`. Its contract rejects a report that attempts to promote itself.

The workflow will attest the exact report bytes with the SHA-pinned `actions/attest` action and retain the report, full logs, hashes, and serialized Sigstore bundle. The attestation uses the SLSA provenance v1 predicate.

Authority ingestion will invoke only the allowlisted GitHub CLI attestation verifier with typed arguments equivalent to:

```text
gh attestation verify <report> \
  --repo Mnehmos/MathOS \
  --signer-workflow Mnehmos/MathOS/.github/workflows/publication.yml \
  --source-ref refs/heads/main \
  --source-digest <exact-commit-sha> \
  --predicate-type https://slsa.dev/provenance/v1 \
  --deny-self-hosted-runners \
  --format json
```

MathOS will then parse and constrain the verified certificate and statement rather than trusting command success alone. The report digest, repository, workflow, ref, commit, runner environment, policy hash, formalization, environment, module, evidence closure, and retained artifacts must all agree.

The publication workflow lives on protected `main`. Pull-request CI may test candidate generation and rejection paths, but cannot produce authoritative evidence.

The implementation follows GitHub's current [artifact attestation guidance](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations) and the [GitHub CLI attestation verification contract](https://cli.github.com/manual/gh_attestation_verify).

## Consequences

- A local worker, model, caller, or altered report cannot cross the authority boundary without a valid repository-scoped workflow attestation.
- No long-lived private signing key is required in the repository or engine.
- Promotion requires GitHub attestation verification and is not fully offline by default. A future offline mode may use a securely acquired trusted root and retained bundle.
- The workflow predicate remains partly workflow-controlled, so authority consumes only fields independently constrained by the certificate plus exact content identities revalidated by MathOS.
- A compromised protected publication workflow can produce bad candidates. Branch protection, pinned actions, minimal permissions, reviewed workflow changes, and exact signer constraints remain administrative and engineering controls around that boundary.
- Publication proof authority still does not establish source-statement fidelity, novelty, pedagogy quality, or mathematical importance.

## Rejected alternatives

### Trust a CI environment variable

Rejected because any local caller can set it.

### Sign with a repository secret

Rejected as the initial design because long-lived key custody, rotation, leakage, and verifier separation add avoidable operational risk.

### Let the publication report mark itself authoritative

Rejected because the generator and claimant would be the same authority.

### Treat GitHub workflow success as evidence

Rejected because a green check does not bind the exact retained report bytes or provide portable cryptographic provenance.
