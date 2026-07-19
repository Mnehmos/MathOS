# MathOS

**MnehmosAI builds MathOS, a verifier-gated operating system for mathematical research and learning.**

MathOS takes a mathematical claim from informal statement through formalization, proof or counterexample, independent verification, pedagogy, provenance, and reinforcement-learning export.

## Product identity

| Name | Role |
| --- | --- |
| MnehmosAI | Company and umbrella brand |
| MathOS | Flagship mathematical operating system |
| MathCorpus | Verified knowledge, provenance, and training layer |
| Proof Search | Research and verification engine |
| Mathematical Claim Engine | Core claim-lifecycle subsystem inside MathOS |

MathOS is the product. MCP is one interface into MathOS alongside future application, API, command-line, and research interfaces.

## Current vertical slice

MathOS v0.1 establishes the trusted claim lifecycle in a finite formal domain:

1. Submit an informal statement and versioned formal specification.
2. Produce an untrusted enumeration proof, counterexample, or unknown result.
3. Recompute the proof obligation or witness in an independent verifier.
4. Apply an explicit verified or unresolved claim state.
5. Record every step in a hash-chained SQLite provenance ledger.
6. Generate certainty-preserving pedagogy and a validated RL trajectory.
7. Access the same Claim Engine through the CLI or MCP stdio interface.

The initial verifier handles universal claims over finite Boolean, integer, string, or null domains. Unsupported and unbounded claims remain unresolved. A Lean subprocess adapter is present and fails closed when Lean is unavailable.

## Quick start

MathOS requires Python 3.12 and has no runtime dependencies outside the standard library.

```bash
make install
make check
make demo
```

The demo produces exactly one proved claim, one disproved claim, and one unresolved claim. It also writes validated RL trajectories under `.mathos-demo/exports/`.

Individual CLI operations:

```bash
mathos init --db .mathos/mathos.db
mathos submit --db .mathos/mathos.db \
  --statement "For every Boolean p, p or not p." \
  --formal-file examples/finite/excluded_middle.json
mathos replay --db .mathos/mathos.db
mathos validate-export --input trajectory.json
```

## MCP stdio

```json
{
  "mcpServers": {
    "mathos": {
      "command": ".venv/bin/mathos-mcp",
      "args": ["--db", ".mathos/mathos.db"]
    }
  }
}
```

The server implements initialization, `tools/list`, and `tools/call` over newline-delimited JSON-RPC stdio. Protocol output is isolated on stdout.

## Administrative controls

Development is governed by:

- [Dynamic Gitflow Policy](docs/administrative-controls/GITFLOW.md)
- [Verifier-Gated TDD Policy](docs/administrative-controls/TDD.md)
- [Contribution Guide](CONTRIBUTING.md)
- [Security Policy](SECURITY.md)
- [0-to-1 Product Specification](docs/product/SPEC.md)
- [0-to-1 Definition of Done](docs/product/ZERO_TO_ONE_DOD.md)
- [System Architecture](docs/architecture/SYSTEM_OVERVIEW.md)
- [Trust Model](docs/architecture/TRUST_MODEL.md)
- [Implementation Evidence](docs/implementation/0001-zero-to-one-evidence.md)
- [GPT-5.6 Sol Dev Diary](docs/dev-diary/README.md)

## License

MathOS is source-available under the [PolyForm Noncommercial License 1.0.0](LICENSE). Commercial use requires a separate license from MnehmosAI.
