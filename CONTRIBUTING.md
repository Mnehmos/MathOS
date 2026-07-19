# Contributing to MathOS

MnehmosAI builds MathOS, a verifier-gated operating system for mathematical research and learning.

MathOS is the product. MCP is one interface into the platform, not the platform itself. Contributions should preserve the boundaries among MathOS, MathCorpus, Proof Search, and the Mathematical Claim Engine.

## Before contributing

Read the administrative controls:

- [Dynamic Gitflow Policy](docs/administrative-controls/GITFLOW.md)
- [Verifier-Gated TDD Policy](docs/administrative-controls/TDD.md)

## Required workflow

1. Classify the change by risk.
2. Create a short-lived branch from main.
3. For a Class 0 documentation-only change, define acceptance criteria and run appropriate documentation checks. Executable Red and Green evidence is required only when executable examples or generated behavior change.
4. For a Class 1 through Class 3 change, define acceptance criteria and a failing test or formal obligation.
5. Implement the smallest passing change.
6. Refactor while tests remain green.
7. Run the independent verifier required by the trust boundary.
8. Record reproducible evidence in the pull request.
9. Obtain the required review before merge.

## Pull requests

Complete the repository pull-request template. A pull request must identify:

- What changed and why
- Its change class and affected trust boundary
- Red and Green test evidence, or Class 0 documentation-check evidence
- Verification commands and results
- Claim, theorem, corpus, or artifact identifiers
- Data and code provenance
- Third-party licenses and new dependencies
- Known limitations and follow-up work

Generated proof text, model output, solver output, or a successful search trace is not accepted as verified merely because it was produced successfully.

## Licensing and provenance

MathOS code is source-available under the PolyForm Noncommercial License 1.0.0 unless a path states different terms.

Do not remove upstream notices or relicense third-party code. Do not submit datasets, papers, examples, model outputs, or training records without documented rights and provenance. Commercial use requires a separate agreement from MnehmosAI.

By submitting a contribution, you represent that you have the right to submit it under the applicable repository terms. Contributor terms may be added before external contributions are accepted at scale.

## Security and private data

Never commit credentials, private keys, tokens, personal data, unpublished confidential material, or access-controlled research data.

Report security-sensitive defects through [GitHub private vulnerability reporting](https://github.com/Mnehmos/MathOS/security/advisories/new) and follow the [Security Policy](SECURITY.md). Do not disclose vulnerability details in a public issue.
