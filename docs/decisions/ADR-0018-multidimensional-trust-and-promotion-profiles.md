# ADR-0018: Trust is multidimensional and promotion is profile-specific

Date: 2026-07-25

Status: accepted

## Context

Lean kernel verification establishes that a proof term has the recorded formal type in an exact
environment. It does not establish that the formal type faithfully represents a source, that its
definitions are appropriate, that it covers the advertised theorem, or that the artifact is ready
for reuse. A single `verified` flag would collapse these independent judgments and could promote a
formally correct trivialization as a faithful theorem.

MathOS already has stronger authority paths for kernel results and statement fidelity. Creating a
second mutable status path for those dimensions would introduce conflicting authorities.

## Decision

MathOS derives one `trust_status/1` snapshot for an exact formalization version with five axes:

- kernel: `unverified`, `lean_checked`, `kernel_verified`, or `failed`;
- fidelity: `unreviewed`, `source_mapped`, `reviewer_checked`, `expert_approved`, or `rejected`;
- definition: `ungrounded`, `source_grounded`, `project_approved`, or `upstream_accepted`;
- reuse: `experimental`, `campaign_specific`, `candidate`, `review_ready`, or `upstreamed`; and
- coverage: `unknown`, `statement_only`, `analytical_core`, `supporting_lemma`,
  `finite_specialization`, `asymptotic_component`, or `full_theorem`.

Kernel status is projected from controlled Lean evidence and receipt-bound proof/refutation
authority. Fidelity status is projected from the existing role-separated fidelity history. Neither
axis has a generic status mutation API.

Definition, reuse, and coverage use the immutable `trust_transition/1` ledger. Every transition
names one exact formalization, its prior and next state, the current predecessor, a reviewer, a
reason, sorted registered evidence-artifact hashes, actor, and timestamp. Definition and reuse
promotions are adjacent; explicit evidence-backed demotions are allowed. Coverage is a reviewed
classification rather than an ordered promotion ladder. SQL constraints, insert triggers, unique
predecessor rules, immutable-row triggers, canonical readback, and application validation enforce
the same contract.

Migration 0014 creates no synthetic transition. Existing formalizations therefore begin at
`ungrounded`, `experimental`, and `unknown`; existing valid kernel and fidelity evidence continues
to project only onto its own axis.

Promotion is a deterministic assessment, not another stored status:

| Profile | Kernel | Fidelity | Definition | Reuse | Coverage |
| --- | --- | --- | --- | --- | --- |
| experimental | `lean_checked` or `kernel_verified` | not `rejected` | unrestricted | unrestricted | unrestricted |
| publication | `kernel_verified` | `reviewer_checked` or `expert_approved` | at least `source_grounded` | at least `candidate` | declared, not `unknown` |
| upstream | `kernel_verified` | `expert_approved` | at least `project_approved` | at least `review_ready` | declared, not `unknown` |

Portable release construction maps private releases to `experimental` and public releases to
`publication`. New builds emit `release_manifest/2` with a hash-bound
`promotion_assessment/1`, exact trust-axis values, and kernel/fidelity evidence heads.
`release_manifest/1` remains byte-compatible for legacy private bundles; offline verification
rejects a legacy public bundle because it has no multidimensional assessment.

CLI and MCP expose the same closed `transition_trust` mutation and read-only `trust_status`
representation. Existing booleans that report a particular execution check, such as Comparator
sandbox verification, remain local predicates and are not canonical mathematical trust.

## Consequences

- Kernel verification cannot alter fidelity, definition, reuse, or coverage.
- Each axis retains its own reviewer, reason, evidence, timestamp, and immutable history.
- A formally correct but fidelity-unreviewed or definition-ungrounded full-theorem claim is
  represented honestly and fails publication and upstream promotion.
- Public release and offline verification fail closed when any required axis, evidence head,
  assessment hash, or retained review artifact is absent or inconsistent.
- Detailed definition-review and upstream-collaboration workflows remain separate follow-up work
  in issues #68 and #69.

## Rejected alternatives

### Add columns and a single aggregate `verified` value

Rejected because an aggregate hides which independent judgment is missing and invites automatic
cross-axis promotion.

### Permit callers to set kernel or fidelity status

Rejected because those dimensions already have receipt-bound and role-separated authority
histories that must remain the only source of their status.

### Backfill optimistic migration states

Rejected because the absence of a definition, reuse, or coverage review is evidence of no review,
not evidence for a promoted state.

### Let public `release_manifest/1` retain its old interpretation

Rejected because a legacy manifest cannot prove that the new independent promotion policy was
evaluated.
