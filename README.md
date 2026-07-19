# Mathematical Claim Engine

The Mathematical Claim Engine is a local-first mathematical knowledge-production and verification system built by MnehmosAI.

The binding product and implementation contract is [SPEC.md](SPEC.md). When this README, prior code, or an implementation assumption conflicts with that specification, the specification wins.

## Release status

**MathOS 1.0.0 is not complete or released.**

The repository currently contains two bodies of work:

1. A legacy Python finite-domain claim kernel. It preserves useful experiments in claim identity, verifier-gated state, provenance, CLI, MCP, and trajectory validation. It is not the specified product and carries no 1.0 release claim.
2. The in-progress Rust modular monolith named `mcl`, which is the canonical implementation required by the specification.

No remote `v1.0.0` tag exists. Release is prohibited until the complete Definition of Done in section 30 of the specification passes and `mcl acceptance --all --clean` produces a verified release candidate.

## Product boundary

MnehmosAI is the company. The Mathematical Claim Engine, exposed as MathOS, is the product.

Earlier proof-search episodes, claim records, MathCorpus packets, and MCIP bundles are migration inputs or export formats. They are not parallel applications. Their useful capabilities and histories must be absorbed without silently promoting trust.

The product contains one canonical service layer shared by:

- the `mcl` command-line interface;
- the Model Context Protocol adapter;
- the local SQLite store;
- the content-addressed artifact store;
- the Lean 4 verifier worker;
- release, pedagogy, migration, and learning-export modules.

## Current implementation phase

See:

- [Implementation status](docs/implementation/STATUS.md)
- [Real blockers](docs/implementation/BLOCKERS.md)
- [Release checklist](docs/implementation/RELEASE_CHECKLIST.md)
- [Architecture decisions](docs/decisions/)

The implementation agent must continue until the complete specification passes. A pilot, demo, schema, or partially working command is not completion.

## License

The repository is source-available under the [PolyForm Noncommercial License 1.0.0](LICENSE). Commercial use requires a separate license from MnehmosAI.
