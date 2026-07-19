# Diagnostic Evidence

Diagnostic evidence records what one exact contained verifier attempt observed about one exact formalization version. It is durable and reproducible context. It is not proof authority and cannot change mathematical status.

## Promotion boundary

The public creation path is:

```text
mcl verify promote-diagnostic \
  --formalization-object-id <uuidv7> \
  --formalization-version-hash <sha256> \
  --job-id <uuidv7> \
  --actor reviewer-name \
  --idempotency-key diagnostic-example-1
```

Add `--dry-run` to resolve and validate the complete evidence proposal without mutation.

The caller does not submit an evidence payload. The application derives it from canonical state and requires exact agreement among:

- formalization object and immutable version;
- environment hash;
- Lean module artifact hash;
- declaration name;
- completed verifier job;
- canonical execution report;
- bounded stdout and stderr artifacts where present.

The report is read through the content-addressed store, its hash is verified, its closed schema is validated, and its job and target identities are compared before persistence. A mismatch or corrupted artifact fails closed.

## Identity and durability

Evidence identity is the SHA-256 hash of its canonical `evidence/1` payload. Artifact hashes are sorted and unique. The record retains actor attribution and exact producing-job provenance.

```text
mcl verify evidence --evidence-id <uuidv7>
mcl verify evidence-list --limit 20
```

Exact retries return the original record. A changed retry under the same idempotency key fails. Evidence rows cannot be updated or deleted. Reads recompute the payload hash and compare every database projection, so bypassed database guards become detectable corruption.

## Authority boundary

This path can create only:

```text
evidence_kind = lean_elaboration
authority_class = diagnostic
```

An elaborated diagnostic means the contained worker observed Lean accept the submitted driver. A rejected diagnostic means Lean or the source policy rejected that exact attempt. A failed diagnostic describes an operational failure such as timeout, output exhaustion, version mismatch, or launch failure.

None of those results proves, disproves, or establishes fidelity for the source claim. No direct mathematical-status mutation exists. Authoritative proof or refutation evidence remains blocked until proof-closure, hole, unsafe, dependency, and axiom audits are implemented and reviewed.
