# Implementation Blockers

Last updated: 2026-07-19

Only irreducible external blockers belong here. Ordinary unfinished work belongs in `STATUS.md` or an issue.

## Local Lean execution in the managed work environment

### Description

The repository pins Lean to `leanprover/lean4:v4.32.0`. Elan 4.2.3 downloaded and installed that exact official toolchain, but the Lean executable exits with `error: failed to locate application` inside the managed process sandbox.

### Impact

`mcl doctor` correctly remains unhealthy in this local environment. No local Lean elaboration, kernel, hole, unsafe, or axiom evidence can be claimed here.

This does not block persistence, domain, policy, schema, CLI, MCP, release-structure, or test-harness work. It does block local completion of verifier acceptance criteria.

### Attempts

1. Installed the official Elan distribution in the ignored repository toolchain directory.
2. Installed the pinned Lean 4.32.0 distribution through Elan.
3. Diagnosed safety-munged shared-library symlinks in the workspace and replaced them only inside the ignored toolchain with their exact versioned files.
4. Installed the same official Elan and Lean toolchain under `/tmp`, where those workspace symlink transformations do not apply.
5. Ran the exact Lean binary directly from both installations. Both failed with the same application-location error.
6. Requested a bounded unsandboxed `lean --version` check. The managed execution policy rejected it as unavailable under the active permission profile.

### Smallest required resolution

Run the pinned toolchain on a fresh GitHub-hosted Linux runner through CI. If it succeeds there, retain the local limitation as an environment-specific diagnostic. If it also fails there, investigate the pinned distribution or choose another explicitly reviewed pin through an ADR.

No user decision is currently required because unrelated implementation continues and CI is the next safe resolution path.

## GitHub publication workflow lacks authenticated Git transport

### Description

The connected GitHub app can manage issues and pull-request metadata, but the local Git checkout has no authenticated transport for pushing its existing commit graph. The environment also reports `gh: command not found`.

### Impact

Eight controlled local commits exist beyond `origin/main`, but the branch cannot yet be published without recreating a divergent commit history through the connector. GitHub Actions therefore cannot validate Windows, fresh Linux, or pinned Lean behavior yet.

### Attempts

1. Confirmed the connected GitHub app can create and update issues.
2. Ran `gh --version` and `gh auth status`; execution stopped at the missing binary.
3. Preserved the local branch and commit sequence without using a second remote-commit path that could desynchronize the checkout.

### Smallest required resolution

From the authenticated home workstation, push `feat/spec-driven-rust-rebuild` and open a draft pull request against `main`. The connected GitHub app can then manage reviews, comments, checks, and merge metadata without rewriting the local commit graph.

No user decision is currently required while unrelated implementation work remains available.
