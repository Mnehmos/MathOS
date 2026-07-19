# Dynamic Gitflow Policy

## Purpose

MathOS uses a short-lived, risk-aware branch workflow. The process becomes stricter as a change approaches the mathematical trust boundary. The main branch must remain releasable, reproducible, and suitable for downstream use.

This policy is an administrative control. It applies to maintainers, contributors, automation, and AI-assisted development.

## Core rules

1. Main is the only permanent branch.
2. Production changes enter main through a pull request.
3. Every pull request declares its change class before review.
4. Tests, verifier evidence, provenance, and review requirements scale with risk.
5. Research and experiment branches may move quickly, but their output is not trusted until it passes the normal merge gates.
6. Force pushes, branch deletion, and direct commits to main are prohibited after repository bootstrap.
7. A passing generator, model, or proof search process is not its own verifier.

## Branch naming

Use lowercase, short-lived branches with one of these prefixes:

| Prefix | Purpose |
| --- | --- |
| feat/ | New user-facing or platform behavior |
| fix/ | Bug or correctness repair |
| docs/ | Documentation and administrative controls |
| refactor/ | Behavior-preserving implementation change |
| test/ | Test infrastructure or coverage |
| chore/ | Maintenance, dependencies, or tooling |
| research/ | Reproducible mathematical or systems research |
| experiment/ | Disposable exploration that is not yet production-ready |
| release/ | Release preparation |
| hotfix/ | Urgent repair based on the current release |

Use an issue or claim identifier when one exists, followed by a short description.

Examples:

- feat/claim-ingestion
- fix/provenance-hash
- research/bh-dependence
- docs/administrative-controls

## Change classes

### Class 0: Administrative and documentation

Examples include prose, templates, licensing metadata, and non-executable documentation.

Required gates:

- Accurate links and formatting
- No contradiction with code, licensing, or current policy
- Owner or code-owner review
- Documentation checks when available

### Class 1: Standard implementation

Examples include UI behavior, adapters, MCP interfaces, noncritical utilities, and ordinary refactors.

Required gates:

- Test-driven development evidence
- Relevant unit and integration tests
- Full affected test suite
- One maintainer approval
- Resolved review threads
- Dependency and license review when applicable

### Class 2: Verifier-critical

Examples include claim-state transitions, formalization, proof checking, counterexample checking, provenance, corpus schemas, proof-search acceptance, scoring, and RL export.

Required gates:

- A failing regression or specification test recorded before the fix
- Unit, integration, property, and adversarial tests as applicable
- Independent verifier output
- Pinned toolchain and dependency versions
- Provenance and artifact identifiers
- Code-owner review
- Independent approval when another qualified maintainer is available
- No self-merge when an independent reviewer is available

### Class 3: Release, migration, or security-sensitive

Examples include public releases, destructive migrations, authentication changes, secret handling, and trust-boundary changes spanning multiple subsystems.

Required gates:

- All Class 2 gates
- Full repository test and verification suite
- Migration, rollback, or recovery plan
- Security and licensing review
- Release notes
- Tagged release created from main only

## Workflow

1. Start from the latest main.
2. Classify the change from Class 0 through Class 3.
3. Create a correctly named branch.
4. Define acceptance criteria and tests before implementation.
5. Follow the verifier-gated TDD policy.
6. Open a pull request early when design review would reduce risk.
7. Complete the pull-request evidence checklist.
8. Run all required automated and mathematical gates.
9. Resolve every review thread.
10. Squash merge by default, then delete the branch.
11. Tag releases only from a verified commit on main.

A rebase merge may preserve a meaningful, reviewed commit series. Merge commits are reserved for release integration or cases where topology is materially useful.

## Research and experiment branches

Research and experiment branches may contain exploratory notebooks, scripts, candidate proofs, generated trajectories, or temporary instrumentation. They must clearly label unverified results.

Before production behavior moves from research or experiment work into main:

1. Restate the behavior as a specification.
2. Add a failing test or formal obligation.
3. implement the smallest passing change in a normal feature or fix branch.
4. Run independent verification.
5. attach provenance for imported data, generated artifacts, and external results.

Exploratory success is evidence for a hypothesis, not proof of correctness.

## Hotfixes

A hotfix branches from the released commit or current main. The pull request must include a regression test that fails without the repair. If an active security incident makes that impossible, the pull request must document the exception, the immediate verification performed, and the tracked follow-up test. The exception does not waive independent verification of mathematical claims.

## Commit and pull-request conventions

Use Conventional Commit prefixes:

- feat:
- fix:
- docs:
- refactor:
- test:
- chore:
- research:
- build:
- ci:

Each pull request should address one coherent concern. Large changes should be divided at stable interfaces so each merged unit remains usable and verified.

## Provenance requirements

A pull request must identify any affected:

- Claim, theorem, or counterexample identifiers
- Lean modules or other formal artifacts
- MathCorpus records or schemas
- Proof-search runs and toolchain versions
- Generated datasets or RL trajectories
- Third-party code, data, licenses, and citations
- Verification logs or reproducible commands

No pull request may convert an unverified output into a trusted result solely because the producing system reported success.

## Repository settings

Main should require pull requests, resolved conversations, required status checks, and code-owner review. Force pushes and deletion should remain disabled. Administrator bypass is reserved for repository recovery and documented emergencies.
