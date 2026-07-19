# Cycle 0001: Zero-to-One Evidence

Date: July 18, 2026

## Scope

This cycle implements the complete MathOS 1.0 claim lifecycle defined by the 0-to-1 product specification: ingestion, finite formalization, untrusted search, verifier-gated outcome, pedagogy, provenance, CLI and MCP access, and RL trajectory export.

## TDD record

The first test run failed during collection with `ModuleNotFoundError: mathos`. This established the expected Red state before the package existed. The Green loop then exposed and corrected reproducibility, resource cleanup, and static-validation defects.

One important adversarial discovery was that short-circuit evaluation could hide an invalid expression branch. Static type inference now validates the full expression tree before evaluation.

## Local verification

Run from the repository root:

```bash
make install
make check
make adversarial
make demo
make validate-demo
.venv/bin/python -m pip check
git diff --check
```

Observed evidence:

- 39 total tests pass with `ResourceWarning` promoted to an error.
- Thirteen dedicated adversarial scenarios pass.
- A deterministic property test compares 200 Boolean expressions with an independent reference oracle.
- The canonical demo produces one `verified_proved`, one `verified_disproved`, and one `unresolved` claim.
- All three exported RL trajectories validate.
- Provenance replay accepts the intact ledger and rejects mutated event data.
- Package dependency inspection reports no broken requirements.
- Patch whitespace validation passes.
- A wheel builds without dependency resolution, installs into a new virtual environment, runs the canonical demo, validates all exports, and passes `pip check`.

## Adversarial inventory

- Forged proof digest
- False counterexample
- Search budget exhaustion presented as proof
- Unsupported operator
- Out-of-domain typed value
- Excessive expression depth
- Invalid expression hidden in an unreachable branch
- Mixed domain types
- SQL metacharacters in user text
- Missing Lean toolchain
- Database event mutation
- RL event mutation with recomputed outer hash
- MCP use before initialization
- Malformed MCP JSON followed by successful recovery
- Verified-state downgrade attempt
- Missing formalization
- Materialized claim-state mutation
- Missing RL lifecycle event
- Excessive integer growth
- Oversized claim statement
- Invalid CLI export validation process status
- Self-consistent RL outcome relabeling
- Excessive assignment budget
- Rehashed pedagogy certainty corruption
- Terminal lifecycle write failure and rollback
- Concurrent content-addressed submission
- External Lean process timeout or operating-system failure
- Incomplete global provenance path in a claim-specific export

## Release evidence still required

- Pass GitHub Actions on the release pull request.
- Resolve every actionable review thread.
- Merge the reviewed release commit to `main`.
- Create the `v1.0.0` tag on that exact commit.
