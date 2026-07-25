# Trust Model

MathOS does not use one Boolean as mathematical trust. Trust is derived for one exact
formalization version across five independent dimensions.

## Trust dimensions

| Dimension | Question answered | Authority source | Conservative default |
| --- | --- | --- | --- |
| kernel | Did controlled Lean checking accept the exact formal statement? | Lean diagnostic evidence and receipt-bound proof/refutation evidence | `unverified` |
| fidelity | Does the formal statement match the reviewed source relationship? | Role-separated fidelity review history | `unreviewed` |
| definition | Are the formal definitions grounded and approved for the intended project? | Immutable reviewed trust transitions | `ungrounded` |
| reuse | How ready is the artifact for use beyond its current campaign? | Immutable reviewed trust transitions | `experimental` |
| coverage | What portion of the advertised mathematical result is formalized? | Immutable reviewed trust transitions | `unknown` |

The complete state vocabularies and promotion policy are fixed by
[`trust_status/1`](../../schemas/trust/trust-status-1.schema.json) and
[ADR-0018](../decisions/ADR-0018-multidimensional-trust-and-promotion-profiles.md).

Kernel and fidelity are projections of their existing evidence histories. They cannot be set
through the trust-transition API. Definition, reuse, and coverage transitions require an exact
formalization, current predecessor, reviewer, reason, timestamp, and verified artifact evidence.
Changing one dimension never changes another.

Claim research status remains a separate live derivation over current source, fidelity, and
receipt-bound proof/refutation authority. Comparator acceptance remains a separate authority class.
Pedagogy, release construction, corpus/RL projection, and workflow success cannot create
mathematical trust.

## Promotion profiles

| Profile | Required state |
| --- | --- |
| experimental | kernel is `lean_checked` or `kernel_verified`; fidelity is not `rejected` |
| publication | kernel is `kernel_verified`; fidelity is at least `reviewer_checked`; definition is at least `source_grounded`; reuse is at least `candidate`; coverage is declared |
| upstream | kernel is `kernel_verified`; fidelity is `expert_approved`; definition is at least `project_approved`; reuse is at least `review_ready`; coverage is declared |

Each `trust_status/1` response includes all three deterministic evaluations and explicit blockers.
New portable releases bind the relevant eligible assessment in `release_manifest/2`. Private
release maps to `experimental`; public release maps to `publication`. A public legacy manifest
without this binding fails offline verification.

## Interfaces

Read all five dimensions and promotion blockers:

```text
mcl --root <instance> --json verify trust-status \
  --formalization-object-id <uuidv7> \
  --formalization-version-hash <sha256>
```

Record one definition, reuse, or coverage decision:

```text
mcl --root <instance> --json verify transition-trust \
  --request-json <trust_transition/1-json> \
  --actor <actor> \
  --idempotency-key <key>
```

The request's `reviewer_identity` must exactly equal `--actor`; delegated review recording is not
accepted. Definition, reuse, and coverage histories are each capped at 256 immutable decisions.

MCP exposes the equivalent closed `trust_status` and `transition_trust` verify actions. Both
interfaces return the same canonical representation.

## Fail-closed rules

- Missing, corrupt, stale, contradictory, or substituted authority evidence is an integrity
  failure or a promotion blocker, never an implicit promotion.
- Migration creates no synthetic definition, reuse, or coverage promotion.
- Definition and reuse promotions are adjacent and predecessor-guarded; explicit demotions remain
  auditable. Coverage changes are reviewed classifications.
- Release construction rejects an incomplete profile before writing a directory.
- Offline verification recomputes the assessment and checks its subject, profile, statuses,
  evidence heads, artifact closure, and hash binding.
- A `verified` execution predicate in a specialized report describes only that check; it is never
  the sole canonical trust representation.

## Residual risks

SHA-256 histories are tamper-evident rather than externally authenticated unless a protected
attestation or trusted manifest hash anchors them. Definition semantics and upstream suitability
still require human review; issues #68 and #69 define the deeper workflows. User-provided text
remains data and must be escaped by any future graphical client.
