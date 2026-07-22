# ADR-0012: MathCorpus and MCIP are deterministic offline release projections

Date: 2026-07-22

Status: accepted

## Context

Portable MathOS releases contain enough frozen evidence to participate in external corpus and
interchange workflows, but those formats have different schemas and trust vocabularies. Treating
MathCorpus or MCIP as runtime authorities would couple canonical MathOS state to an external
repository, invite network-dependent validation, and risk turning derived evidence into proof,
training, or publication authority.

The projection also needs caller-supplied curation. Domain, level, difficulty, and leakage-aware
split placement cannot be inferred safely from a theorem or proof body.

## Decision

MathOS projects only a structurally verified, receipt-bound `release_manifest/1` directory whose
trusted SHA-256 identity is supplied out of band. `mcl release export` accepts an explicit packet
ID, domain, level, and difficulty bin, then deterministically emits one MathCorpus packet, one
MCIP 1.0.0 bundle containing exactly PacketIdentity, ProofVariant, and DependencyManifest, the
normalized Lean module, the copied source-release manifest, and the exact schemas and license.
The destination must be new, and dry run resolves the same bytes without writing.

The minimum Apache-2.0 upstream contract is vendored byte-for-byte from
`Mnehmos/mathcorpus` commit `a0d08c9ace0dcc70a8bc281dcf29c560242075d3`, tree
`62bc32fac877a82958ffcbe86402f8e793295f99`. Runtime schema retrieval is denied. A closed
Rust-owned `corpus_export_manifest/1` binds the source release and authority/fidelity identities,
curation, upstream commit/tree/schema hashes, every member path/hash/size/license/restriction,
and the packet, MCIP, and module identities.

Private releases map only to `private_audit_only` packets, `private_only` MCIP records, and
private sensitive members. Public releases remain `quarantined`; they additionally require public
source redistribution and redaction plus explicit source and module licenses. Split assignment is
not inferred.

`mcl release verify-export` returns before configuration or database loading. It requires the
trusted export-manifest hash and an independently supplied frozen source release, rejects any
path, symlink, inventory, canonical-JSON, hash, schema, policy, or reference mismatch, then
reprojects the source and requires the complete manifest and all member bytes to be identical.
Packet and MCIP records remain subordinate evidence and grant no MathOS authority.

## Consequences

- MathCorpus is an output contract, not a state store or runtime dependency.
- One trusted export-manifest hash identifies the exact projection independently of its location.
- A coherent replacement manifest is insufficient without the trusted hash and exact source
  release.
- Schema validation is reproducible offline and cannot silently follow upstream changes.
- Private evidence cannot become trainable or public through this projection.
- Public evidence remains quarantined until a separate leakage-aware split decision exists.
- Adding other MCIP records, RL/evaluation data, or Comparator packages requires a later reviewed
  contract.

## Rejected alternatives

### Resolve schemas from the network

Rejected because availability or upstream drift could change validation of the same frozen bytes.

### Infer curation or split metadata

Rejected because proof text does not establish reviewed domain, difficulty, or contamination
policy.

### Treat MCIP verification status as MathOS authority

Rejected because MCIP is derived child evidence and cannot replace the receipt-bound authority
and fidelity chain.

### Verify only the export's internal hashes

Rejected because a coherent substituted directory could redefine its own manifest. Verification
therefore requires both an out-of-band export hash and byte-identical reprojection from the exact
source release.
