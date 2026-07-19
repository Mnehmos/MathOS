# MathOS 0-to-1 Definition of Done

This is the implementation ledger for v0.1.0. An item is complete only when its evidence exists on the release commit.

| Requirement | Status | Evidence |
| --- | --- | --- |
| Administrative Gitflow and TDD controls | Complete | docs/administrative-controls |
| Executable product specification | Complete | docs/product/SPEC.md and canonical fixtures |
| Stable claim identity and state machine | Complete | `tests/test_claim_engine.py` and CLI demonstration |
| Independent search and verification boundary | Complete | `tests/test_finite_verifier.py` and forged-evidence tests |
| Proved, disproved, and unresolved outcomes | Complete | Canonical fixture and demo tests |
| Tamper-evident provenance ledger | Complete | Replay and database-tamper tests |
| Certainty-preserving pedagogy | Complete | Outcome-specific engine tests |
| RL trajectory export | Complete | Export validation and tamper tests |
| CLI interface | Complete | `tests/test_cli.py` subprocess tests |
| MCP stdio interface | Complete | `tests/test_mcp.py` protocol and lifecycle tests |
| Adversarial playtesting | Complete | Nine scenarios plus 200 deterministic randomized claims |
| Clean environment installation | Complete | Wheel build, isolated install, demo, export validation, and `pip check` |
| All review threads and checks pass | Pending | Release pull request |
| Main tagged v0.1.0 | Pending | Release tag |

## Terminal release gate

- All table rows are Complete.
- The full test suite passes from a clean virtual environment.
- The canonical demo produces exactly one proved, one disproved, and one unresolved claim.
- Provenance replay succeeds before tampering and fails after tampering.
- No test, verifier, or review failure is ignored.

Detailed local evidence is recorded in `docs/implementation/0001-zero-to-one-evidence.md`. Rows that depend on GitHub remain pending until the release pull request supplies remote evidence.
