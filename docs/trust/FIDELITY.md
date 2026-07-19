# Statement Fidelity Reviews

Lean can establish that a theorem follows in a formal environment. It cannot establish that the theorem faithfully represents the source statement that motivated it. MathOS therefore records statement fidelity as a separate reviewed evidence axis.

## Review contract

A `fidelity_review_request/1` binds one exact source version, claim version, formalization version, producing run, reviewer, review level, verdict, findings, ambiguity disposition, definition mappings, supporting artifacts, and prior evidence head.

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

Statement fidelity evidence is reviewed evidence, not proof evidence. It cannot prove or disprove a claim, approve axioms, establish novelty, or authorize publication. Mathematical status remains unavailable until an exact fidelity-verified formalization also has current authoritative proof or refutation evidence under the publication trust profile.

## Commands

```text
mcl verify review-fidelity \
  --request-json '<fidelity_review_request/1>' \
  --actor reviewer-name \
  --idempotency-key review-example-1

mcl verify fidelity-status \
  --formalization-object-id <uuidv7> \
  --formalization-version-hash <sha256>
```

The same actions are available through the MCP `verify` family as `review_fidelity` and `fidelity_status`.
