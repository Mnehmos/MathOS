# Local Proof-Closure and Axiom Audits

MathOS audits one exact formalization after an accepted diagnostic Lean elaboration. The audit records whether the submitted source contains a forbidden escape and which transitive axioms Lean reports for the exact declaration.

Local audit evidence is durable and useful, but it is not proof authority. The local worker does not enforce the publication network and memory boundary required by `SPEC.md`.

## Closed policy

The local policy is committed at `policies/lean-local-audit-1.json`. Its canonical SHA-256 identity is part of every audit request and report. A queued job fails closed if the committed policy changes before execution.

The source scan rejects explicit holes, custom axioms and constants, unsafe declarations, native evaluation, command elaborators, initialization hooks, file inclusion, and metaprogramming syntax on the submitted proof-authority path. Comments and strings do not trigger the identifier scan.

After that scan, the worker creates a verifier-controlled driver containing:

```text
#print axioms <exact-declaration-name>
```

The parser accepts exactly one declaration-specific Lean result. It canonicalizes a bounded, duplicate-free axiom list and rejects malformed, ambiguous, or excessive output.

For a Lake project, the elaboration worker's controlled driver already performs both `#check` and
`#print axioms` after the exact bound module build. The audit worker rematerializes and revalidates
the complete project/source closure, then reopens that immutable diagnostic report and its hashed
logs instead of treating a second build cache as independent evidence. The later protected
publication build remains the separate clean authority boundary.

The local allowlist contains only:

- `Classical.choice`
- `Quot.sound`
- `propext`

An unexpected axiom produces a rejected audit. A clean list produces a passed audit. Operational and parser failures remain distinct from mathematical rejection.

## Durable lifecycle

```text
accepted diagnostic elaboration
→ mcl verify audit
→ durable lean_audit job
→ mcl worker --job-kind audit
→ private CAS report and diagnostics
→ mcl verify promote-audit
→ diagnostic proof_closure_scan and axiom_audit evidence
```

Audit enqueue requires exact agreement among the formalization version, accepted elaboration evidence, environment, Lean module, declaration, and committed policy. Jobs are actor-attributed, idempotent, leased, restart-safe, and protected by the same legal state transitions as elaboration jobs.

Promotion re-reads and hashes the private verifier report, validates its closed schema, and matches every identity to the terminal job. It then creates exactly one `proof_closure_scan` and one `axiom_audit` evidence record in one transaction. A retry returns the same pair. Partial promotion is impossible.

## Trust boundary

Every report and promoted record produced by this path remains:

```text
authority_class = diagnostic
authoritative = false
```

A local audit reports `trust_profile = local` with both control flags false. A project audit derived
from a successful protected execution reports `trust_profile = publication` with both flags true
and reuses the exact immutable verifier streams; it does not rerun or reinterpret them.

Neither audit proves or disproves the source claim, establishes statement fidelity, or authorizes
publication. Authoritative proof evidence remains impossible until the protected attestation and
receipt-only gate revalidate the clean build, dependency closure, audit, exact retained evidence,
and source-bound request.

## Commands

```text
mcl verify audit \
  --formalization-object-id <uuidv7> \
  --formalization-version-hash <sha256> \
  --diagnostic-evidence-id <uuidv7> \
  --actor auditor-name \
  --idempotency-key audit-example-1

mcl worker --job-kind audit \
  --worker-id local-audit-worker \
  --lease-seconds 3660

mcl verify audit-status --job-id <uuidv7>
mcl verify audit-list --limit 20

mcl verify promote-audit \
  --formalization-object-id <uuidv7> \
  --formalization-version-hash <sha256> \
  --job-id <uuidv7> \
  --actor auditor-name \
  --idempotency-key audit-evidence-example-1
```

Use `--dry-run` on enqueue and promotion to validate and calculate identities without mutation.
