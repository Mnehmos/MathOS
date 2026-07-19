# Durable Verifier Jobs

A verifier job is durable intent to check one exact Lean source artifact in one exact environment. It is not proof evidence and it cannot change a claim's mathematical status.

## Canonical request

The closed `verifier_request/1` object contains only:

- an exact registered environment hash;
- an exact registered Lean source artifact hash;
- a bounded dotted Lean declaration name.

There is no executable, shell fragment, working directory, environment variable, provider key, model route, status verdict, or arbitrary file path in the request. The future worker derives its entire process command from the registered environment template.

## Queue from CLI

```text
mcl verify check \
  --environment-hash <sha256> \
  --module-artifact-hash <sha256> \
  --declaration-name MathOS.Example.theoremName \
  --priority 0 \
  --actor operator-name \
  --idempotency-key verify-example-1
```

Add `--dry-run` to validate exact references and predict the canonical input hash without creating a job.

```text
mcl verify status --job-id <uuidv7>
mcl verify list --limit 20
```

These commands enqueue and inspect work. They do not execute Lean yet.

## State and recovery

```text
queued -> leased -> running -> succeeded | failed
   |          |         |
   +----------+---------+-> cancelled | blocked
              |
              +-> queued after lease expiry
```

The database rejects state jumps, identity rewrites, and deletion. A lease records a bounded worker identity and expiry. Leasing an item increments its attempt count. An expired leased or running job returns to `queued` before the next worker selects work, preserving its history and input identity.

Only one transaction can lease the highest-priority eligible job. A worker cannot start a job leased by another worker or one whose lease expired.

## Trust boundary

Durability solves scheduling ambiguity, not mathematics. A queued, running, failed, or even eventually succeeded job is not authoritative evidence by itself. Authority will require a separate evidence record tied to exact verifier artifacts, environment, audits, and policy.

The current slice deliberately stops before process execution. Issue #17 remains open until contained execution, bounded diagnostic artifacts, real Lean testing, and cross-platform CI satisfy its complete acceptance criteria.
