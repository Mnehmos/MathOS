# Controlled Comparator authority

An official Comparator report is an evidence producer, not an authority decision. MathOS grants
Comparator authority only after the report passes three distinct application operations against
the current canonical ledger.

## Trust transition

1. `stage-comparator-authority` verifies the exact 20-file protected run bundle, independently
   reprojects its five-file package from the canonical plan and frozen release, rejects unsafe or
   extra filesystem entries, and copies every required byte into CAS. The resulting
   `comparator_authority_stage/1` is immutable and non-authoritative.
2. `ingest-comparator-authority` accepts only the staged report and attestation-bundle hashes. It
   replays staged CAS, invokes the hash- and version-pinned GitHub CLI attestation verifier with the
   committed repository, workflow, protected-ref, commit, predicate, subject, and hosted-runner
   constraints, retains its raw output, and creates an immutable non-authoritative
   `comparator_attestation_verification/1` receipt.
3. `promote-comparator-authority` accepts only the receipt hash and mutation attribution. It
   replays the complete chain, rechecks the current formalization, publication authority, fidelity,
   release, plan, package, policy, and CAS closure, then derives accepted authoritative
   `comparator_run` evidence under the closed `evidence/3` contract.

The report and both pre-promotion records keep `authoritative: false`. Callers cannot provide an
evidence subject, result, kind, authority class, environment, artifact set, or authority binding.

## CLI workflow

All staging paths must resolve inside the configured MathOS instance root.

```text
mcl --root <instance> --json verify stage-comparator-authority \
  --run-dir <instance/run> \
  --expected-report-hash <report-sha256> \
  --expected-package-verification-hash <verification-sha256> \
  --plan-file <instance/plan.json> \
  --release-dir <instance/release> \
  --expected-release-manifest-hash <manifest-sha256> \
  --attestation-bundle-file <instance/attestation.json> \
  --actor <actor> --idempotency-key <key>

mcl --root <instance> --json verify ingest-comparator-authority \
  --report-artifact-hash <report-sha256> \
  --attestation-bundle-artifact-hash <bundle-sha256> \
  --actor <actor> --idempotency-key <key>

mcl --root <instance> --json verify promote-comparator-authority \
  --comparator-receipt-hash <receipt-sha256> \
  --actor <actor> --idempotency-key <key>

mcl --root <instance> --json verify comparator-authority-status \
  --evidence-id <evidence-uuid>
```

Staging and promotion support `--dry-run`. Exact retries return the persisted canonical result.
The MCP `verify` family exposes the same four actions through the same application service and a
closed nested `comparator` request object.

## Currentness and failures

Evidence is immutable historical fact. A status read returns `current` only while every bound
input still replays. Otherwise it returns `stale` with a deterministic reason:

- `formalization_not_current`
- `publication_authority_not_current`
- `fidelity_not_current`
- `release_binding_changed`
- `plan_binding_changed`
- `package_binding_changed`
- `policy_changed`

Missing, altered, or corrupt CAS bytes and inconsistent database projections are integrity
failures, not ordinary staleness. They fail the read instead of producing a weaker status.

## Non-authority consequences

Comparator authority records that one exact publication package passed the reviewed protected
Comparator boundary. It does not create Lean proof or refutation evidence, replace statement
fidelity review, directly change a claim to `proved` or `disproved`, or complete a research pilot.
The claim-status derivation explicitly excludes `comparator_run` evidence from its proof and
refutation inputs.

The trust decision is recorded in
[ADR-0016](../decisions/ADR-0016-controlled-comparator-evidence-authority.md).
