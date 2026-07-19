# Verifier-Gated TDD Policy

## Purpose

MathOS uses test-driven development with an additional independent verification gate. Ordinary software tests protect behavior. Formal checkers, counterexample validators, schema validation, and provenance checks protect mathematical trust.

This policy applies to production code, formal artifacts, schemas, data pipelines, MCP interfaces, proof search, claim-state transitions, and RL export.

## The development cycle

### 1. Specify

State the behavior, invariant, mathematical obligation, or failure mode in observable terms. Identify the affected trust boundary and change class.

### 2. Red

Add a focused test, fixture, formal obligation, or counterexample case that fails for the expected reason before implementation changes.

A test that fails because of broken setup, missing dependencies, or an unrelated error does not satisfy the Red step.

### 3. Green

Implement the smallest coherent change that makes the new test pass. Avoid broad refactors during this step.

### 4. Refactor

Improve structure, naming, interfaces, and duplication while keeping the new test and affected suite green.

### 5. Verify

Run a verifier independent from the system that produced the candidate result whenever the change affects mathematical correctness.

Examples include:

- Lean kernel checking for Lean proofs
- Direct substitution and domain checking for symbolic results
- Recomputed witnesses for counterexamples
- Schema and hash verification for provenance records
- Replay of proof-search traces against pinned tools
- Independent evaluators for generated training trajectories

### 6. Record

Attach the commands, versions, identifiers, and results needed to reproduce the evidence. A pull request is incomplete if reviewers cannot determine what was tested and what established trust.

## Test layers

| Layer | Purpose |
| --- | --- |
| Unit | Local behavior and edge cases |
| Property | Invariants across generated inputs |
| Integration | Boundaries between MathOS subsystems and interfaces |
| Contract | Stable API, MCP, schema, and serialization behavior |
| Golden | Intentional comparison against reviewed canonical output |
| Regression | A specific previously observed failure |
| Formal | Kernel-checked proof obligations and trusted checker results |
| Provenance | Hashes, identities, toolchains, sources, and lineage |
| Adversarial | Malformed, ambiguous, hostile, or misleading inputs |
| End-to-end | Claim intake through final verified state and export |

Tests should be deterministic by default. Nondeterministic research systems must record seeds, configurations, model identifiers, tool versions, and acceptance criteria.

## Mathematical trust rules

1. Generation and verification must be separate logical steps.
2. A model, tactic, solver, or search engine may propose a result but may not certify its own output.
3. A claim may enter a verified state only when its exact formal artifact or witness passes the designated verifier.
4. Failed and unknown results must remain first-class outcomes. The system must not coerce them into success.
5. Verification must include required assumptions, domains, imports, toolchain pins, and artifact hashes.
6. Pedagogical explanations inherit no more certainty than the verified artifact they explain.
7. RL exports must preserve outcome labels, verifier evidence, provenance, and failed attempts when policy permits.

## Minimum evidence by change class

### Class 0

Documentation or administrative changes require accurate references, formatting checks when available, and review for consistency with current behavior.

### Class 1

Standard implementation requires a recorded Red test, the new passing test, affected unit and integration suites, and a concise statement of observed behavior.

### Class 2

Verifier-critical changes require:

- A regression or specification test that fails before the change
- Boundary and failure-path coverage
- Property or adversarial tests when input space matters
- Independent verifier output
- Pinned toolchain and dependencies
- Provenance identifiers and artifact hashes
- Replay or reproduction instructions

### Class 3

Release, migration, and security-sensitive work requires all Class 2 evidence plus the full repository suite, recovery planning, and release-level validation.

## Formalization and proof changes

A theorem or proof pull request should include:

- The informal claim and its assumptions
- The formal statement and namespace
- The expected status, such as proved, disproved, or unresolved
- The failing obligation or missing proof state before implementation
- The final kernel-check result
- Toolchain and dependency pins
- Any reused upstream theorem and its license
- Provenance linking the claim, formal artifact, proof attempt, and verifier result

Changes that merely weaken a theorem, add assumptions, or alter a definition to obtain a passing proof must be called out explicitly.

## Counterexamples

A counterexample must be checked against the exact quantified claim and domain. Tests must establish that the witness satisfies all premises and violates the conclusion. Numeric approximations require stated tolerances and, where correctness depends on exactness, an exact or formally justified representation.

## Bug fixes

Every bug fix begins with a regression test that demonstrates the defect. The test should remain after the fix to prevent recurrence.

## Refactors

A refactor must preserve behavior. Existing tests should pass before and after the change. Add characterization tests first when behavior is insufficiently specified.

## Documentation-only and exploratory exceptions

Documentation-only changes do not require an executable Red test unless they alter executable examples.

Research and experiment branches may explore without full TDD. Their code cannot become trusted production behavior until the result is restated as a specification and passes the complete cycle.

Emergency exceptions must be documented in the pull request with scope, reason, immediate evidence, owner, and a tracked follow-up. Mathematical verification requirements are not waived.

## Definition of done

A change is done only when:

- Acceptance criteria are explicit
- The relevant test failed first for the expected reason
- The implementation passes the new test
- Affected suites pass
- Required independent verification passes
- Failure paths and boundary cases are covered
- Documentation matches behavior
- Provenance and licensing are recorded
- No flaky or ignored test is used to claim success
- Review threads are resolved
- The pull request contains reproducible evidence
