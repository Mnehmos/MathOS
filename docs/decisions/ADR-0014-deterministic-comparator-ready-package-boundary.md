# ADR-0014: Comparator-ready packages are deterministic non-authoritative projections

Date: 2026-07-22

Status: accepted

## Context

Comparator protects a different boundary from MathOS publication replay. It compares declarations
from a trusted challenge environment with a solution, enforces an explicit axiom allowlist, and
replays the solution through the Lean kernel. Its security argument also depends on a real Linux
sandbox, exact tool builds, and avoiding prior compilation of potentially adversarial solution
code.

A five-file package must therefore be portable before it can be verified, but package integrity is
not Comparator verification. Allowing `verification.json`, a local build, fake-landrun, or an
ordinary program exit code to claim success would collapse the distinction between prepared input
and protected verifier evidence.

## Decision

MathOS accepts one closed canonical `comparator_package_plan/1`. The plan binds an exact portable
release manifest and its headline formalization, contains explicit reviewer-controlled
`Challenge.lean` source, selects exactly the release declaration, fixes the permitted axioms and
nanoda setting, records bounded formalization metadata, and pins the fixed Comparator,
lean4export, and landrun repositories to full commits.

`Solution.lean` is never supplied by the plan. It is copied byte-for-byte from the verified frozen
release's `replay/Submission.lean` and must match the publication module artifact. The selected
formalization record, environment/dependency manifest, replay declaration, theorem source, and
publication binding must all identify that same exact theorem.

The projection contains exactly:

```text
Challenge.lean
Solution.lean
config.json
formalization.yaml
verification.json
```

`config.json` uses the official Comparator module, theorem, axiom, and nanoda fields.
`formalization.yaml` is deterministic UTF-8 YAML with the frozen identities, reviewed metadata,
and exact repository/commit pins. The closed `comparator_package_verification/1` record binds the
four other files, the source-release members, plan hash, and an input fingerprint covering every
SPEC staleness input.

The verification record has one possible status in this contract: `ready`. Both
`comparator_verified` and `authoritative` are constant `false`. Construction and offline
verification create no evidence record and cannot affect claim research status.

`mcl release verify-comparator-package` requires an independently trusted verification hash, the
canonical plan, the independently supplied frozen release, and its trusted manifest hash. It
rejects links, reparse points, missing or extra members, noncanonical JSON, altered bytes, changed
bindings, and any failure of byte-identical deterministic reprojection. It does not open SQLite,
use the network, or execute Comparator.

## Consequences

- A challenge, solution, theorem source, dependency manifest, configuration, formalization
  record, plan, or tool-pin change produces a different input fingerprint and package identity.
- A caller can prepare a challenge but cannot assert that it matches the solution.
- The package can move independently of the operational database while retaining exact source
  lineage.
- Real sandboxed Comparator execution and evidence ingestion remain a separate protected trust
  transition.
- Ordinary Lean elaboration of the two package files is only a readiness diagnostic.

## Rejected alternatives

### Copy the solved module as both challenge and solution

Rejected because it does not preserve the independent trusted-challenge boundary and weakens the
meaning of statement comparison.

### Generate a challenge by rewriting arbitrary Lean proof text

Rejected because a text rewrite is not a Lean parser and could silently alter scope, declarations,
macros, or the theorem statement.

### Store `Comparator-verified` in the package plan

Rejected because caller-authored metadata and package construction are not verifier evidence.

### Run fake-landrun on Windows and retain the result as publication evidence

Rejected because the official Comparator security assumptions require the real production sandbox;
fake-landrun is a development aid only.
