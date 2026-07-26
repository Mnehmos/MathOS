# Durable Verifier Jobs

A verifier job is durable intent to check one exact Lean source artifact in one exact environment. It is not proof evidence and it cannot change a claim's mathematical status.

## Canonical request

The closed `verifier_request/1` object contains only:

- an exact registered environment hash;
- an exact registered Lean source artifact hash;
- optionally, an exact registered project archive hash, fixed archive root, and validated relative
  module path;
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

For a Lake project, add all three closed project selectors:

```text
  --project-archive-artifact-hash <sha256> \
  --project-archive-root project \
  --project-module-path Final.lean
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

## Worker execution boundary

Every worker:

- accepts only the configured `lean`, `lean.exe`, or `lake` executable;
- constructs arguments from typed state rather than request text;
- verifies source bytes from CAS and materializes them in an exclusive temporary workspace;
- on Windows, uses the bounded operating-system temporary root so deep Lake output paths do not
  cross the legacy path-length boundary;
- for a project request, rejects traversal, links, special entries, hidden build caches, changed
  configuration hashes, unlocked dependencies, omitted Lean sources, and a module whose bytes do
  not match the separately registered source artifact;
- prepares an allowlisted Mathlib cache when the environment requires it, builds only the bound
  module's `olean` facet with the manifest's concurrency limit, and then runs the controlled driver
  through Lake;
- creates a controlled driver that checks the requested declaration;
- clears the child environment, restores only a narrow runtime allowlist, and selects the validated exact toolchain through worker-controlled `ELAN_TOOLCHAIN`;
- supplies null stdin;
- bounds wall-clock time and retained stdout plus stderr;
- preserves bounded diagnostics and a canonical execution report as private artifacts;
- rejects explicit holes, custom source axioms, unsafe declarations, native evaluation, command elaborators, initialization hooks, and file inclusion before launch.

The lexical rejection policy is intentionally conservative. It is defense in depth, not kernel proof-closure analysis.

The `local` profile does not enforce a memory limit or network namespace, and its report says so.
It is not a hardened virtualization boundary.

The Linux `publication` profile requires an exact memory bound and the protected runner controls
`sudo`, `/usr/bin/bwrap`, and `/usr/bin/prlimit`. Fixed cache acquisition runs first in a
capability-free Bubblewrap filesystem boundary with host homes and runtime directories masked and
network deliberately shared so pinned dependency objects can be fetched. The module build and
controlled driver then run with all namespaces unshared, no network namespace connectivity, no
capabilities, a cleared environment, a writable mount containing only the exact materialized
project, the exact pinned toolchain mounted read-only, the manifest's `LEAN_NUM_THREADS`, and the manifest's
address-space limit. A successful inner execution records both control flags as true; a launch or
rejected execution cannot claim a successful protected result. Windows workers fail closed if
asked to run this profile.

## Trust boundary

Durability and controlled execution solve scheduling and process ambiguity, not mathematics. A queued, running, failed, or succeeded job is not authoritative evidence. An `elaborated` report says only that the observed Lean binary accepted the controlled driver. A `rejected` report does not disprove a source claim. Operational job success says only that an attempt completed and its immutable report was committed.

Every execution report is permanently marked `authoritative: false`, including a successful
publication-profile report. Authority still requires exact proof evidence, dependency closure,
hole and unsafe scans, axiom audit, fidelity review, protected attestation, and publication policy.
Contained execution closed issue #17 without claiming any of those later controls.
