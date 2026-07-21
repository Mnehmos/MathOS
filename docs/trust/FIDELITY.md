# Statement Fidelity Reviews

Lean can establish that a theorem follows in a formal environment. It cannot establish that the theorem faithfully represents the source statement that motivated it. MathOS therefore records statement fidelity as a separate reviewed evidence axis.

## Review contract

`fidelity_review_request/1` binds one exact source version, claim version, formalization version, producing run, reviewer, review level, verdict, findings, ambiguity disposition, definition mappings, supporting artifacts, and prior evidence head. Its bytes and hashes remain immutable.

`fidelity_review_request/2` adds the reviewer-authored `reviewed_source_relation` field. `claim` means the declaration was compared with the source claim; `logical_negation` means it was compared with that claim's logical negation. The relation must match the formalization's immutable polarity. Version 1 can qualify only a claim-polarity proof and can never qualify source disproof.

The supported review levels are:

- surface syntax;
- mathematical statement;
- definition mapping;
- source paper correspondence;
- benchmark hash alignment;
- expert domain review.

Surface review and benchmark alignment cannot produce a `verified` fidelity verdict. A review with unresolved ambiguity cannot produce `verified`. Benchmark alignment requires a content-hashed benchmark source, and paper correspondence requires a paper source.

## Role separation

The actor submitting the review must be the named reviewer. A reviewer may attest to their own formalization, but may not verify it. A `verified` report requires the reviewer to differ from the formalization author.

The source, claim, and formalization must form one exact stored lineage. Recorded claim ambiguity cannot be silently discarded by the review.

## Controlled evidence

The application creates the canonical review report. A caller cannot upload a report and present it as controlled review provenance. The generated report is canonical JSON stored privately in the content-addressed artifact store with `human_review` provenance.

The durable evidence record has:

```text
evidence_kind = statement_fidelity_review
authority_class = reviewed
environment_hash = null
producing_job_id = null
producing_run_id = <exact run>
```

The report preserves the exact theorem type, declaration hash, formalization author, reviewer, findings, and request hash. Status reads revalidate the database projections, report bytes, report schema, request identity, artifact metadata, reviewer identity, producing run, and supersession chain.

## Supersession and status

The first review must name no predecessor. Every later review must compare against and supersede the one current evidence head. A stale reviewer receives a structured conflict instead of overwriting another review.

The current head derives one of:

```text
attested
benchmark_aligned
verified
rejected
```

An exact formalization with no review derives `unreviewed`. Earlier reviews remain in history as `superseded`. Nothing is deleted or rewritten.

## Trust boundary

Statement fidelity evidence is reviewed evidence, not proof evidence. It cannot prove or disprove a claim, approve axioms, establish novelty, or authorize publication by itself.

The read-only `claim_research_status/1` service combines the two independent axes. It rehashes the exact source, claim, formalization, fidelity report, supporting artifacts, and full receipt-bound publication closure; enumerates every current formalization automatically; and rechecks the captured Store basis after replay. Only a polarity-consistent pair of current verified fidelity and current protected authority can yield `proved` or `disproved`. If the claim's exact source version is no longer that source object's live head, the result is `open/source_version_not_current` and historical witnesses do not qualify. Missing or corrupt inputs fail closed. No status field, mutation command, evidence selector, or caller-authored verdict exists.

## Commands

```text
mcl verify review-fidelity \
  --request-json '<fidelity_review_request/1-or-2>' \
  --actor reviewer-name \
  --idempotency-key review-example-1

mcl verify fidelity-status \
  --formalization-object-id <uuidv7> \
  --formalization-version-hash <sha256>

mcl verify claim-status \
  --claim-object-id <uuidv7> \
  --claim-version-hash <sha256>
```

The same actions are available through the MCP `verify` family as `review_fidelity`, `fidelity_status`, and `claim_status`. CLI and MCP call the same application read and return the same closed snapshot.

A `disproved` snapshot is only the eligibility gate for a correction. It does not mutate the false claim or prove a proposed replacement. The separate [counterexample repair boundary](COUNTEREXAMPLE_REPAIR.md) packages one exact current refutation and creates a new independently unproved claim.
