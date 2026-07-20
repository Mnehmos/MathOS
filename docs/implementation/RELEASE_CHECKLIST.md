# MathOS 1.0.0 Release Checklist

Last updated: 2026-07-20

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

Phase 3 fidelity evidence: closed request and report types distinguish all specified review levels, attestation, benchmark alignment, verified review, rejection, ambiguity disposition, definition mappings, and exact source, claim, formalization, run, artifact, and supersession references. Reviews persist through one shared CLI and MCP application path. Controlled private reports bind human-review provenance, derived status follows one compare-and-swap evidence head, and superseded reviews remain visible. Self-verification, stale heads, missing artifacts, substituted lineage, erased ambiguity, report corruption, restart, and retry paths are tested. Fidelity is only one part of the full claim lifecycle, so the complete core lifecycle criteria remain unchecked.

Remote fidelity evidence: GitHub Actions run `29706138708` passed fresh Linux, Windows, storage, Python, and real pinned Lean jobs on exact tree `80b1d2e92e81192a2863bb445a7bef872fc21b72`. Issue #20 is closed. Proof authority and derived mathematical status remain absent, so the complete lifecycle criteria remain unchecked.

## Verification

- [ ] Lean verification uses pinned environments.
- [ ] Authoritative proof and refutation evidence is recorded against exact formalization versions.
- [ ] Hole, unsafe, and axiom policies are enforced.
- [ ] Replay works and reports its exact trust boundary.
- [x] Publication CI produces retained evidence.

Phase 1 evidence: `lean-toolchain` pins Lean 4.32.0. No proof-authority item is complete.

Phase 3 environment evidence: a closed canonical manifest, exact hash, immutable persistence, CLI registration, restart retrieval, corruption detection, and formalization reference gate exist. No Lean artifact has been executed or accepted as evidence, so every verification item remains unchecked.

Phase 3 artifact evidence: Lean source bytes can be validated, atomically content-addressed, registered with immutable metadata, verified after restart, and materialized into a fresh contained workspace. Formalizations require the exact registered Lean source hash. This establishes artifact integrity only, so every verification item remains unchecked.

Phase 3 job evidence: exact verifier requests can be validated, durably queued, idempotently retried, transactionally leased, recovered after lease expiry, and inspected after restart. This scheduling layer grants no evidence authority, so every verification item remains unchecked.

Phase 3 execution evidence: a leased job can invoke only the allowlisted Lean executable with typed arguments in a fresh contained workspace. Source policy, toolchain matching, timeout, combined output bounds, private diagnostic artifacts, and canonical execution reports are enforced. Every report remains explicitly non-authoritative, the local profile reports absent memory and network isolation, and publication-profile execution is refused. Exact dependency closure, proof evidence, audits, and publication isolation remain open, so every verification item remains unchecked.

Phase 3 diagnostic evidence: the closed `evidence/1` contract names exact subject versions, all required evidence kinds, explicit authority, provenance, artifacts, environment, supersession, and staleness. Migration 0009 makes evidence rows immutable and rejects subject/version mismatch. The application can now promote only non-authoritative Lean elaboration diagnostics after matching an exact formalization, terminal job, environment, module, declaration, private verifier report, and CAS artifact closure. Retry, restart, mismatch, forged-provenance, and corruption tests pass locally. Proof closure, authoritative evidence, and mathematical-status derivation remain absent, so every verification item remains unchecked.

Phase 3 local audit evidence: the committed audit policy and closed request/report schemas bind an exact formalization, accepted elaboration diagnostic, environment, module, declaration, and policy identity. Durable audit jobs run source escape scans and verifier-controlled `#print axioms`, retain private diagnostics, and atomically promote diagnostic proof-closure and axiom-audit evidence. Policy mismatch, malformed output, duplicate axioms, retries, restart, partial promotion, and corruption fail closed. Local audits explicitly lack publication memory and network isolation and cannot become authoritative, so the complete proof-authority and publication criteria remain unchecked.

Phase 3 publication contract evidence: the closed publication policy, request, retained closure, candidate report, quarantine stage, canonical CAS attestation-verification record (`publication_attestation_verification/1`), and separate immutable SQLite ingestion receipt (`PublicationIngestionReceiptSnapshot` in `publication_ingestion_receipts`) bind exact proof or refutation intent, diagnostic and audit evidence, formalization, environment, module, declaration, Git commit and tree, protected workflow identity, runner class, required isolation controls, retained artifacts, and SHA-pinned attestation actions and verifier. The application derives canonical private request artifacts from the current formalization head and one exact accepted elaboration/audit chain without accepting request JSON or authority fields. Typed formalization polarity prevents one exact version/evidence closure from being requested as both proof and refutation; legacy records without polarity cannot publish until versioned and reverified. The workflow-facing validator and staging path require 25 exact fixed-path closure roles, recompute every member hash, and replay the source, claim, formalization, evidence, job, report, policy, environment, module, parser-derived protected dependency output, and local/protected axiom relationships. Hash-only CLI and MCP ingestion resolve that immutable stage, run an isolated copy of the Linux-amd64 SHA-pinned GitHub CLI with fixed arguments, parse one closed attestation result, retain the raw output and canonical CAS attestation-verification record, then register the separate immutable SQLite ingestion receipt plus the logical stage/actor idempotency result in one final transaction that rechecks currentness. Missing or altered CAS, stale state, key rebinding, parser ambiguity, and unpinned or misbehaving verifiers fail closed. The protected producer fails unless GitHub reports an actively protected source ref, then builds a real no-import candidate under clean-checkout, Bubblewrap network/process isolation, timeout, memory, and output-size controls; pull-request CI runs it without attestation, while protected `main` attests the exact candidate report. Every request, report, stage, validation result, attestation-verification record, and ingestion receipt remains structurally non-authoritative. The attestation-verification record and its ingestion receipt prove provenance only and may not be treated as a passed report or proof. Merged-tree controlled-ingestion evidence is complete and recorded below.

Phase 3 authority-gate candidate: closed `evidence/2` and nested `publication_authority_binding/1` contracts preserve `evidence/1` identities while allowing only accepted authoritative Lean kernel proof/refutation evidence with a complete receipt-bound CAS closure. Hash-only CLI and MCP promotion accept only an ingestion receipt hash plus attribution. The application replays the report, closure, all 25 members, bundle, raw output, canonical receipt, current Store snapshots, request derivation, and attestation parser; explicitly requires a `passed` report; and derives evidence kind from typed polarity. Controlled ingestion immutably binds the receipt's exact subject to the registered private generated publication-request artifact. The Store accepts only a non-deserializable derived commit and uses one immediate transaction to recheck that receipt/request/subject relationship, stage projections, current head, environment, polarity, uniqueness, and idempotency. Migration 0011 rejects every kernel/authoritative/v2 row outside the fixed shape. Statement fidelity is intentionally not an authority input; a later truth read must combine current fidelity with current proof authority. The verification checkbox remains open until the exact candidate is merged and protected-main promotion evidence is independently audited.

Remote controlled-ingestion evidence: PR #34 merged as `95bd8a1d2068612b5eca644c3d77754b5e4f49fd`, tree `4027d03fe05bd997108c86250adcf2d920adda48`; main CI run `29721420110` passed all five jobs. Protected publication run `29721420136`, job `88285050682`, passed exact candidate construction, attestation, independent challenge, staging, controlled ingestion, and retention. Artifact `8452528096` has GitHub archive digest `66cd47753a77f90bf216b874d4d7c99f7ba561a4586e29c524e094c82eb3206c`. Downloaded inspection recomputed the report (`5f02696eac1308f48eae9f085f003e750f009dbe496d055543792b088e3b2aa6`), 25-role closure (`6440836d612d91dfe43af952d6ac83ca1445bf75c86b8b52a8c79f6121427b38`), candidate bundle (`c02cde0384cd015bbb085d2f98e69c245888dd7cfbc10973b0c9a9e722ad326c`), stage (`e4b307416358ef657c57d81d07c6b21df0381befbf5600982442bc435d98baa5`), raw verifier output (`bdfe54e9e889c52c7dc32941dc8fb16d50b1e2b8071a5b69be04eb1040d45932`), and canonical CAS attestation-verification/SQLite receipt key (`659b789a41a14ac59ca9253c2c71d73c4717acc58dcc2187809e92c710814402`) hashes. All 71 retained files, eight stderr files, immutable repository/owner identities, one subject, one source dependency, Rekor timestamp `2026-07-20T06:22:33Z`, and 18 false authority fields passed audit. This is exact provenance evidence only.

Remote evidence: GitHub Actions run `29704542965` passed the exact real Lean audit lifecycle and fresh Linux and Windows suites on tree `30d55d6e2ce7b0de2d921cff3e1368124fd9f66f`. This validates the local diagnostic audit capability only. The remaining verification checklist items stay open until publication-profile authority and replay are complete.

Remote request-preparation evidence: PR #30 merged as `da33431a1061bb3f05db7a7d2473f1fb5b8059f2`, tree `620f2a3060290ffe37fb3051ff604fa4433679af`. Main CI run `29711916515` passed every required platform/toolchain job; publication run `29711916501` retained and independently verified the exact non-authoritative smoke artifact. That request-preparation capability is now incorporated into the merged candidate and controlled-ingestion evidence above.

Repository protection evidence: live GitHub API reads now report `main` as protected. Pull requests, strict passage of the five CI jobs, resolved conversations, stale-review dismissal, and admin enforcement are required; force-pushes and deletions are disabled. The protected candidate also checks GitHub's immutable protected-ref context at runtime. This administrative control does not itself grant proof authority.

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
