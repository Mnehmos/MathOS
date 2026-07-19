# Implementation Blockers

Last updated: 2026-07-19

Only irreducible external blockers belong here. Ordinary unfinished work belongs in `STATUS.md` or an issue.

There are currently no irreducible external blockers.

The managed local sandbox still cannot launch Lean, but the exact pinned toolchain is executable on a fresh GitHub-hosted Linux runner. The contained worker integration runs there after each verifier change. This is a documented local environment limitation, not a project-wide blocker. The GitHub connector publishes every controlled commit to the durable remote branch.
