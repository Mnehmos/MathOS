# RL and evaluation export

`mcl release export-rl` creates a deterministic task dataset from one or more frozen MathOS
releases. It never opens the operational SQLite database or contacts a network service. The
split and trust decision is defined by
[ADR-0013](../decisions/ADR-0013-component-locked-rl-evaluation-projection.md).

## Cohort plan

The plan uses `rl_export_plan/1`. Releases must be sorted by `release_id`, live directly beneath
the supplied source root, and bind their expected `release_manifest/1` hash. Every leakage label
array is mandatory, sorted, unique, and nonempty.

```json
{
  "schema_version": "rl_export_plan/1",
  "publication_cutoff": "2026-07-21",
  "releases": [{
    "release_id": "pilot-a-release",
    "expected_manifest_hash": "<sha256>",
    "split": "held_out_evaluation",
    "published_on": "2026-07-22",
    "benchmark_identity": "mathos-pilot-a-prime-parity",
    "leakage_labels": {
      "theorem_dependency_components": ["pilot-a-prime-parity"],
      "equivalent_formalizations": ["pilot-a-refutation-repair"],
      "shared_sources": ["pilot-a-protected-source"],
      "certificate_families": ["pilot-a-certificate-family"],
      "proof_variants": ["pilot-a-proof-family"]
    }
  }]
}
```

The `published_on` value must equal the UTC date in the release's signed publication receipt.
Train releases must be dated on or before the cutoff. Validation, public-test, and held-out
releases must be later. A private release may only be held out.

## Build and verify

```text
mcl --root does-not-need-to-exist --json release export-rl \
  --plan /path/to/plan.json \
  --source-root /path/to/releases \
  --output-dir /new/path/rl-export \
  --dry-run
```

Dry run verifies every source's exact inventory, canonical schemas, references, and receipt-bound
semantics, resolves every task and output byte, and writes nothing. Remove `--dry-run` to
atomically create the new directory. Existing destinations are never overwritten. The protected
producer runs the separate platform-bound `mcl release verify` Lean replay immediately before
projection; derived export verification remains portable across workers.

Verification needs the trusted output hash plus independent copies of both policy and sources:

```text
mcl --root does-not-need-to-exist --json release verify-rl-export \
  --export-dir /path/to/copied-rl-export \
  --expected-manifest-hash <rl-export-manifest-sha256> \
  --plan /path/to/plan.json \
  --source-root /path/to/copied-releases
```

Verification checks the closed inventory, paths and regular files, canonical JSON, hashes,
committed schemas, task identities, evidence references, license/restriction policy, receipt
dates, component isolation, and family audit. It then structurally reverifies all releases and
byte-compares a fresh deterministic reprojection. It does not silently rerun a source release's
platform-bound Lean replay under a different verifier environment.

## Output contracts

The root `manifest.json` is `rl_export_manifest/1`. It binds the plan hash, publication cutoff,
source release/profile/split/component identities, leakage report hash, counts, and every member
path/hash/size/license/restriction. The manifest itself is excluded from its member inventory to
avoid recursive identity.

Members include:

- the canonical copied plan and `rl_leakage_report/1`;
- one canonical `rl_task/1` file per task;
- the exact source manifest and task-referenced records, evidence, edges, environments, and
  content-addressed artifacts;
- the four committed RL schemas.

Task IDs hash every field except the ID itself. Evidence paths and hashes are exact. Trust binds
the source release, publication receipt, accepted authority evidence, and reviewed fidelity
evidence. Input and target objects reject `chain_of_thought`, `private_chain_of_thought`, and
`reasoning_trace` fields.

## Leakage components

MathOS hashes declared labels by dimension and adds exact derived keys, including normalized
nonempty import manifests. Sharing any key joins releases transitively. A component crossing
split assignments is rejected; tasks are never reassigned row-by-row. The report lists every
component, release, split, leakage key, and task ID, with zero accepted cross-split overlap.

## Task families and current limits

Version 1 emits six families when current reviewed fidelity and authoritative kernel evidence are
present: formalization, fidelity selection, counterexample, statement repair, declaration
retrieval, and proof generation. Reviewed learning units with a split-compatible training status
can additionally emit explanation and curriculum ordering.

Decomposition, proof repair, generalization, and frontier selection remain unimplemented
projections. Any family with no emitted record has a machine-readable skip reason; absence is
never silently represented as success. Comparator packages, remaining pilots, and full 1.0
acceptance are separate unfinished work.
