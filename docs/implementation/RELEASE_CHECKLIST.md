# MathOS 1.0.0 Release Checklist

Last updated: 2026-07-19

This checklist mirrors section 30 of [SPEC.md](../../SPEC.md). A checked item requires linked mechanical and manual evidence. Partial Phase 1 work is recorded below but does not check a release item whose complete wording is not yet satisfied.

## Installation and operation

- [ ] A clean checkout builds using documented commands on supported Windows and Linux CI environments.
- [ ] `mcl init` creates a working local instance.
- [ ] `mcl doctor` reports a healthy installation.
- [ ] MCP and CLI operate over the same canonical service layer.
- [ ] Process restart preserves committed state and resumes or safely requeues durable jobs.

Phase 2 evidence: `mcl init` and storage health work locally and in Windows and Linux CI. The Rust MCP system, query, source, claim, formalization, and research families call the same application service as CLI and survive process restart. Later verifier, pedagogy, and release capabilities and durable job recovery remain open, so the release criteria remain unchecked.

## Core lifecycle

- [ ] Sources, concepts, claims, formalizations, artifacts, evidence, edges, runs, learning units, environments, and releases are fully implemented.
- [ ] Multiple formalizations per claim work end to end.
- [ ] Fidelity review works end to end.
- [ ] Counterexample, repair, proof, disproof, and open outcomes work end to end.
- [ ] Truth status is derived and cannot be directly mutated.
- [ ] Verified intermediate results can be promoted and searched.

Phase 3 fidelity contract evidence: closed request and report types now distinguish all specified review levels, attestation, benchmark alignment, verified review, rejection, ambiguity disposition, definition mappings, and exact source, claim, formalization, run, artifact, and supersession references. The report contract rejects same-author verified review. No review persistence or derived fidelity status exists yet, so all core lifecycle criteria remain unchecked.

## Verification

- [ ] Lean verification uses pinned environments.
- [ ] Authoritative proof and refutation evidence is recorded against exact formalization versions.
- [ ] Hole, unsafe, and axiom policies are enforced.
- [ ] Replay works and reports its exact trust boundary.
- [ ] Publication CI produces retained evidence.

Phase 1 evidence: `lean-toolchain` pins Lean 4.32.0. No proof-authority item is complete.

Phase 3 environment evidence: a closed canonical manifest, exact hash, immutable persistence, CLI registration, restart retrieval, corruption detection, and formalization reference gate exist. No Lean artifact has been executed or accepted as evidence, so every verification item remains unchecked.

Phase 3 artifact evidence: Lean source bytes can be validated, atomically content-addressed, registered with immutable metadata, verified after restart, and materialized into a fresh contained workspace. Formalizations require the exact registered Lean source hash. This establishes artifact integrity only, so every verification item remains unchecked.

Phase 3 job evidence: exact verifier requests can be validated, durably queued, idempotently retried, transactionally leased, recovered after lease expiry, and inspected after restart. This scheduling layer grants no evidence authority, so every verification item remains unchecked.

Phase 3 execution evidence: a leased job can invoke only the allowlisted Lean executable with typed arguments in a fresh contained workspace. Source policy, toolchain matching, timeout, combined output bounds, private diagnostic artifacts, and canonical execution reports are enforced. Every report remains explicitly non-authoritative, the local profile reports absent memory and network isolation, and publication-profile execution is refused. Exact dependency closure, proof evidence, audits, and publication isolation remain open, so every verification item remains unchecked.

Phase 3 diagnostic evidence: the closed `evidence/1` contract names exact subject versions, all required evidence kinds, explicit authority, provenance, artifacts, environment, supersession, and staleness. Migration 0009 makes evidence rows immutable and rejects subject/version mismatch. The application can now promote only non-authoritative Lean elaboration diagnostics after matching an exact formalization, terminal job, environment, module, declaration, private verifier report, and CAS artifact closure. Retry, restart, mismatch, forged-provenance, and corruption tests pass locally. Proof closure, authoritative evidence, and mathematical-status derivation remain absent, so every verification item remains unchecked.

Phase 3 local audit evidence: the committed audit policy and closed request/report schemas bind an exact formalization, accepted elaboration diagnostic, environment, module, declaration, and policy identity. Durable audit jobs run source escape scans and verifier-controlled `#print axioms`, retain private diagnostics, and atomically promote diagnostic proof-closure and axiom-audit evidence. Policy mismatch, malformed output, duplicate axioms, retries, restart, partial promotion, and corruption fail closed. Local audits explicitly lack publication memory and network isolation and cannot become authoritative, so the complete proof-authority and publication criteria remain unchecked.

Remote evidence: GitHub Actions run `29704542965` passed the exact real Lean audit lifecycle and fresh Linux and Windows suites on tree `30d55d6e2ce7b0de2d921cff3e1368124fd9f66f`. This validates the local diagnostic audit capability only. The verification checklist remains open until publication-profile authority, replay, and retained publication evidence are complete.

## Search and context

- [ ] Exact, FTS, graph, declaration, and failure searches work.
- [ ] Context compilation is deterministic and provenance-bearing.
- [ ] Agents outside the originating campaign can locate and reuse verified results.

Phase 2 evidence: exact stable-ID lookup, exact version-hash lookup, current-head FTS5 search, and bounded typed graph traversal work through the application service, CLI, and initial MCP query surface. Declaration and prior-failure search remain unimplemented, so the release criterion is open.

## Pedagogy

- [ ] Hard and soft prerequisites are distinct.
- [ ] Learning units support explanations, examples, counterexamples, misconceptions, exercises, mastery checks, and frontier notes.
- [ ] Curriculum paths can be queried.
- [ ] External taxonomy crosswalks preserve source and license.

Phase 2 evidence: hard and soft prerequisite edge kinds are distinct and the database rejects hard-prerequisite cycles. Learning units, curriculum paths, and taxonomy crosswalks remain unimplemented, so all release criteria stay open.

## Releases and exports

- [ ] Release bundles are complete, hashed, licensed, and policy-checked.
- [ ] Releases verify without the operational database.
- [ ] MathCorpus and MCIP export works.
- [ ] RL and evaluation exports work with leakage-aware splits.
- [ ] Public exports fail closed on restricted or incomplete provenance.

## Migration

- [ ] Legacy proof-search evidence imports without silent trust promotion.
- [ ] Original IDs, hashes, histories, and negative attempts are preserved.
- [ ] The four pilot fixtures are represented in the new architecture.

Phase 1 evidence: the old Python implementation is explicitly classified as legacy input. No importer or pilot fixture is complete.

## Quality and operations

- [ ] All CI checks pass.
- [ ] All adversarial tests pass.
- [ ] Backup and restore is tested.
- [ ] Migrations are documented and tested.
- [ ] Structured errors and logs are implemented.
- [ ] No placeholder handlers exist on required paths.
- [ ] No critical-path TODO, FIXME, panic-only behavior, or undocumented manual database edit remains.
- [ ] User, operator, trust, data-format, and contributor documentation is complete.
- [ ] A release candidate is built, replayed, and tagged `1.0.0`.

Phase 1 evidence: migration 1 has idempotency and FTS5 tests; public CLI errors are structured; required unimplemented handlers have not been added as placeholders. The full quality and operations criteria remain open.

## Mandatory pilots

- [ ] Pilot A: elementary false statement.
- [ ] Pilot B: textbook theorem.
- [ ] Pilot C: BH research formalization.
- [ ] Pilot D: Erdős problem 647 open frontier campaign.

## Product acceptance command

- [ ] One clean command initializes a fresh instance, runs all checks and four pilots, builds database-independent releases, replays them, produces all exports, emits a hashed report, and exits nonzero on any unmet requirement.

## Release authorization

- [ ] Every item above is mechanically and manually reviewed.
- [ ] The release candidate was produced from a clean checkout.
- [ ] The verified commit is merged to protected `main`.
- [ ] The `v1.0.0` tag points to that exact verified commit.
