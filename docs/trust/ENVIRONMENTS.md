# Lean Environment Identity

An environment manifest names the exact verifier context that a future Lean run intends to use. It is a reproducibility object, not proof evidence.

## Trust boundary

The environment hash is:

```text
SHA256(canonical_json(environment_manifest))
```

The identity includes:

- exact Lean release;
- pinned dependency revisions;
- sorted Lean imports;
- SHA-256 hashes of project configuration files;
- supported platform and trust profile;
- typed allowlisted verifier command;
- timeout, output, memory, and concurrency limits;
- disabled network access;
- temporary-workspace policy.

It excludes timestamps, database row numbers, local paths, machine names, and host-specific secrets.

Registering an environment does not execute Lean. It does not establish elaboration, kernel correctness, statement fidelity, acceptable axioms, clean rebuild, or publication readiness.

## Register from CLI

On a POSIX shell:

```text
manifest=$(jq -c . fixtures/environment/lean-4.32-local.json)
mcl environment register \
  --manifest-json "$manifest" \
  --actor operator-name \
  --idempotency-key environment-lean-4.32-local
```

On PowerShell:

```text
$manifest = Get-Content fixtures/environment/lean-4.32-local.json -Raw
mcl environment register `
  --manifest-json $manifest `
  --actor operator-name `
  --idempotency-key environment-lean-4.32-local
```

Add `--dry-run` to validate the manifest and compute its proposed hash without writing state.

Retrieve and list exact environments:

```text
mcl environment get --environment-hash <sha256>
mcl environment list --limit 20
```

All commands support global `--json` output.

## Failure policy

Registration fails closed when a manifest contains:

- a moving dependency target such as a branch name;
- an arbitrary executable or argument;
- a path-shaped import or configuration name;
- unknown fields such as a machine name;
- duplicate or noncanonical dependency and import ordering;
- network access;
- zero or excessive resource limits;
- malformed or non-SHA-256 project configuration hashes.

Formalization creation fails when its environment hash is not registered, even when the hash is syntactically valid.

## Persistence and migration

Migration 0006 adds actor attribution and immutability triggers to the existing `environments` table. Update and deletion are rejected. Corrections require a new manifest and therefore a new environment hash.

Every environment read decodes and validates the stored manifest, recomputes its hash, and compares its trust profile with the indexed column. A mismatch reports `MCL_ENVIRONMENT_INTEGRITY_FAILED` and requires quarantine or verified restore.

`mcl doctor` reports the number of registered environments. A zero count is not database corruption, and a positive count is not proof authority.
