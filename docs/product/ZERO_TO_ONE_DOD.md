# MathOS 0-to-1 Definition of Done

This is the implementation ledger for v1.0.0. An item is complete only when its evidence exists on the release commit. The 0-to-1 milestone terminates at the 1.0 release.

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
| Adversarial playtesting | Complete | Thirteen scenarios plus 200 deterministic randomized claims |
| Clean environment installation | Complete | Wheel build, isolated install, demo, export validation, and `pip check` |
| All review threads and checks pass | Pending | v1.0 release-correction pull request |
| Main tagged v1.0.0 | Ready after merge | Release tag |

## Terminal release gate

- All table rows are Complete.
- The full test suite passes from a clean virtual environment.
- The canonical demo produces exactly one proved, one disproved, and one unresolved claim.
- Provenance replay succeeds before tampering and fails after tampering.
- No test, verifier, or review failure is ignored.

Detailed local evidence is recorded in `docs/implementation/0001-zero-to-one-evidence.md`. The final row becomes complete only after the v1.0 release-correction pull request is merged and its exact `main` commit is tagged.
