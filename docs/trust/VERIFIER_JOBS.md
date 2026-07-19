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

These commands enqueue and inspect work. A separate worker command leases and executes at most one eligible job:

```text
mcl worker --worker-id local-worker --lease-seconds 3660
```

The lease must cover the registered timeout plus a cleanup margin. An empty queue returns a successful structured response without launching a process.

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

## Local execution boundary

The local worker:

- accepts only the configured `lean` or `lean.exe` executable;
- constructs arguments from typed state rather than request text;
- verifies source bytes from CAS and materializes them below the instance root;
- creates a controlled driver that checks the requested declaration;
- clears the child environment and restores only a narrow runtime allowlist;
- supplies null stdin;
- bounds wall-clock time and retained stdout plus stderr;
- preserves bounded diagnostics and a canonical execution report as private artifacts;
- rejects explicit holes, custom source axioms, unsafe declarations, native evaluation, command elaborators, initialization hooks, and file inclusion before launch.

The lexical rejection policy is intentionally conservative. It is defense in depth, not kernel proof-closure analysis.

The local profile does not enforce a memory limit or network namespace, and its report says so. It is not a hardened virtualization boundary. Publication-profile environments are refused by this worker.

## Trust boundary

Durability and controlled execution solve scheduling and process ambiguity, not mathematics. A queued, running, failed, or succeeded job is not authoritative evidence. An `elaborated` report says only that the observed Lean binary accepted the controlled driver. A `rejected` report does not disprove a source claim. Operational job success says only that an attempt completed and its immutable report was committed.

Every local execution report is permanently marked `authoritative: false`. Authority still requires exact proof evidence, dependency closure, hole and unsafe scans, axiom audit, fidelity review, and publication policy. Issue #17 therefore remains open after contained execution.
