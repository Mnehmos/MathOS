# MathCorpus and MCIP export

`mcl release export` creates one deterministic MathCorpus packet and one MCIP v1 evidence
bundle from an already frozen MathOS release. It is an offline projection: it does not open
the operational SQLite database, contact MathCorpus, or grant new mathematical authority.
The governing trust decision is
[ADR-0012](../decisions/ADR-0012-deterministic-offline-mathcorpus-mcip-projection.md).

## Trust boundary

The caller must provide the expected SHA-256 identity of the source release manifest. The
exporter first verifies that exact release directory, then follows its bound formalization to
the exact claim and source records. Packet and MCIP evidence are derived from those records,
the publication receipt, current authority and fidelity bindings, replay environment, and
normalized Lean module.

MathCorpus is not a runtime dependency. MathOS vendors the minimum Apache-2.0 schema set from
`Mnehmos/mathcorpus` commit
`a0d08c9ace0dcc70a8bc281dcf29c560242075d3`, tree
`62bc32fac877a82958ffcbe86402f8e793295f99`. Every schema hash is fixed in the export manifest,
and validation uses only the embedded copies. Unpinned schema retrieval is denied.

## Build

Choose the packet classification explicitly. MathOS does not infer domain, level, or
difficulty from proof text.

```text
mcl --root does-not-need-to-exist --json release export \
  --bundle-dir /path/to/frozen-release \
  --expected-manifest-hash <source-release-manifest-sha256> \
  --packet-id mathos.number_theory.pilot_a_repair.v1 \
  --domain number_theory \
  --level L1_proof_basics \
  --difficulty-bin D1 \
  --output-dir /new/path/corpus-export \
  --dry-run
```

The dry run resolves and validates every output byte but does not create the destination.
Remove `--dry-run` to publish the directory atomically. Existing destinations are never
overwritten.

## Verify

Verification requires the trusted export-manifest hash and an independently supplied copy of
the frozen source release:

```text
mcl --root does-not-need-to-exist --json release verify-export \
  --export-dir /path/to/copied-corpus-export \
  --expected-manifest-hash <corpus-export-manifest-sha256> \
  --source-bundle-dir /path/to/copied-frozen-release
```

The verifier checks the closed inventory, regular-file and path policy, canonical JSON,
member hashes and sizes, exact vendored schemas, MathCorpus packet and MCIP record hashes,
source/publication/environment/module bindings, and the fail-closed export policy. It then
reprojects the supplied frozen release and requires every manifest field and member byte to be
identical. No database or network access is required.

## Output

The v1 export has exactly eleven manifest members:

```text
lean/Submission.lean
licenses/mathcorpus-apache-2.0.txt
mathcorpus/packet.json
mcip/bundle.json
schemas/mathcorpus/packet.schema.json
schemas/mcip/v1/_defs.schema.json
schemas/mcip/v1/bundle.schema.json
schemas/mcip/v1/dependency_manifest.schema.json
schemas/mcip/v1/packet_identity.schema.json
schemas/mcip/v1/proof_variant.schema.json
source-release/manifest.json
```

The root `manifest.json` uses the Rust-owned `corpus_export_manifest/1` contract committed at
`schemas/release/corpus-export-manifest-1.schema.json`. The inventory excludes the root
manifest itself so its identity is not recursive.

MathCorpus file hashes follow upstream rules: canonical sorted compact UTF-8 JSON, and
BOM-free LF-normalized source bytes. `hashes.formal_statement_sha256` is the upstream
canonical hash of theorem name, pretty-printed statement, and toolchain. MCIP
`formal_statement_sha256` retains the raw exact-theorem-type bridge required by MCIP. These
identities are intentionally distinct and both are verified.

## Export policy

| Frozen release | Packet training policy | MCIP eligibility | Sensitive members |
| --- | --- | --- | --- |
| `private` | `private_audit_only` | `private_only` | `private` |
| `public` | `quarantined` | `quarantined` | `public`, with explicit source and module licenses |

A private release is never promoted to a training split. A public release remains quarantined in
this corpus packet because the corpus exporter does not assign leakage-aware splits. The separate
`rl_export_plan/1` projection must bind a verified public release to an eligible component-locked
split; it does not retroactively change this packet's curation policy. Public projection also
requires source redistribution `allowed`, redaction `public`, and an explicit source and module
license. Missing authority fails closed.

MCIP records are child evidence. They mirror a frozen kernel-verification result but cannot
promote packet trust or replace the MathOS release, receipt, authority, or fidelity records.

## Current limits

The exporter emits the evidence MathOS can substantiate without guessing: `PacketIdentity`, a
canonical `ProofVariant`, and a `DependencyManifest`. It does not invent proof-search
episodes, tactic counts, model identity, retrieval candidates, literature review, novelty,
RL transitions, empirical difficulty, or leakage-aware train/validation/test assignments.
Split assignment belongs to the separate RL/evaluation export; the other absent records remain
tracked 1.0 work.
