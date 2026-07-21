# MathOS 1.0.0 Release Checklist

Last updated: 2026-07-21

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
- [x] Truth status is derived and cannot be directly mutated.
- [ ] Verified intermediate results can be promoted and searched.

Phase 3 fidelity evidence: closed request and report types distinguish all specified review levels, attestation, benchmark alignment, verified review, rejection, ambiguity disposition, definition mappings, and exact source, claim, formalization, run, artifact, and supersession references. Reviews persist through one shared CLI and MCP application path. Controlled private reports bind human-review provenance, derived status follows one compare-and-swap evidence head, and superseded reviews remain visible. Self-verification, stale heads, missing artifacts, substituted lineage, erased ambiguity, report corruption, restart, and retry paths are tested. Fidelity is only one part of the full claim lifecycle, so the complete core lifecycle criteria remain unchecked.

Remote fidelity evidence: GitHub Actions run `29706138708` passed fresh Linux, Windows, storage, Python, and real pinned Lean jobs on exact tree `80b1d2e92e81192a2863bb445a7bef872fc21b72`. Issue #20 is closed. At that checkpoint proof authority and derived mathematical status were absent; the authority evidence below now supplies only the former, so the complete lifecycle criteria remain unchecked.

Phase 3 derived-status evidence: issue #38 preserves fidelity v1 canonical identity, adds closed polarity-aware fidelity v2, and derives one exact claim's live status through the same CLI/MCP application service. Callers provide only exact claim identity. The Store automatically captures every current formalization plus source/claim/formalization/fidelity/authority heads in one read snapshot; the application rehashes canonical records and all fidelity CAS, fully replays every receipt-bound protected authority chain, then rejects mixed-time reads on basis change. Proof/refutation fixtures, v1-negation refusal, v2 relation mismatch, superseded claim and formalization heads, moved-source invalidation, restart determinism, and missing-CAS failure pass. The protected publication workflow retains `open` after authority and `proved` only after role-separated verified v2 fidelity. Exact merged-tree CI and the protected artifact have been independently audited, satisfying the truth-derivation item; the broader counterexample, repair, pedagogy, replay, and release lifecycle remains incomplete.

Pilot A refutation candidate evidence: issue #41 commits an exact no-import Lean module for “Every prime number is odd,” including a named checked witness that `2` is prime and not odd and a negation theorem that uses it. The protected producer creates the exact source, normalized universal natural-number claim, negation-polarity formalization, local evidence, and refutation request through `mcl`; its staging, ingestion, authority, v2 logical-negation fidelity, and derived-status path remains unchanged. A Windows CLI playtest completed elaboration and axiom-free audit, prepared the refutation request, and rejected a proof request for the same formalization. The broad lifecycle and Pilot A boxes remain open until protected merged-tree refutation evidence is independently audited and counterexample packaging, repair, repaired proof, pedagogy, and export are implemented.

## Verification

- [x] Lean verification uses pinned environments.
- [ ] Authoritative proof and refutation evidence is recorded against exact formalization versions.
- [x] Hole, unsafe, and axiom policies are enforced.
- [ ] Replay works and reports its exact trust boundary.
- [x] Publication CI produces retained evidence.

Phase 1 evidence: `lean-toolchain` pins Lean 4.32.0. The protected authority evidence below now demonstrates that pin in the receipt-bound publication boundary. This is semantic/CAS revalidation, not the complete typed-action and event-chain replay required by SPEC section 16.6.

Phase 3 environment evidence: a closed canonical manifest, exact hash, immutable persistence, CLI registration, restart retrieval, corruption detection, and formalization reference gate exist. No Lean artifact has been executed or accepted as evidence, so every verification item remains unchecked.

Phase 3 artifact evidence: Lean source bytes can be validated, atomically content-addressed, registered with immutable metadata, verified after restart, and materialized into a fresh contained workspace. Formalizations require the exact registered Lean source hash. This establishes artifact integrity only, so every verification item remains unchecked.

Phase 3 job evidence: exact verifier requests can be validated, durably queued, idempotently retried, transactionally leased, recovered after lease expiry, and inspected after restart. This scheduling layer grants no evidence authority, so every verification item remains unchecked.

Phase 3 execution evidence: a leased job can invoke only the allowlisted Lean executable with typed arguments in a fresh contained workspace. Source policy, toolchain matching, timeout, combined output bounds, private diagnostic artifacts, and canonical execution reports are enforced. Every report remains explicitly non-authoritative, the local profile reports absent memory and network isolation, and publication-profile execution is refused. Exact dependency closure, proof evidence, audits, and publication isolation remain open, so every verification item remains unchecked.

Phase 3 diagnostic evidence: the closed `evidence/1` contract names exact subject versions, all required evidence kinds, explicit authority, provenance, artifacts, environment, supersession, and staleness. Migration 0009 makes evidence rows immutable and rejects subject/version mismatch. The application can now promote only non-authoritative Lean elaboration diagnostics after matching an exact formalization, terminal job, environment, module, declaration, private verifier report, and CAS artifact closure. Retry, restart, mismatch, forged-provenance, and corruption tests pass locally. Proof closure, authoritative evidence, and mathematical-status derivation remain absent, so every verification item remains unchecked.

Phase 3 local audit evidence: the committed audit policy and closed request/report schemas bind an exact formalization, accepted elaboration diagnostic, environment, module, declaration, and policy identity. Durable audit jobs run source escape scans and verifier-controlled `#print axioms`, retain private diagnostics, and atomically promote diagnostic proof-closure and axiom-audit evidence. Policy mismatch, malformed output, duplicate axioms, retries, restart, partial promotion, and corruption fail closed. Local audits explicitly lack publication memory and network isolation and cannot become authoritative, so the complete proof-authority and publication criteria remain unchecked.

Phase 3 publication contract evidence: the closed publication policy, request, retained closure, candidate report, quarantine stage, canonical CAS attestation-verification record (`publication_attestation_verification/1`), and separate immutable SQLite ingestion receipt (`PublicationIngestionReceiptSnapshot` in `publication_ingestion_receipts`) bind exact proof or refutation intent, diagnostic and audit evidence, formalization, environment, module, declaration, Git commit and tree, protected workflow identity, runner class, required isolation controls, retained artifacts, and SHA-pinned attestation actions and verifier. The application derives canonical private request artifacts from the current formalization head and one exact accepted elaboration/audit chain without accepting request JSON or authority fields. Typed formalization polarity prevents one exact version/evidence closure from being requested as both proof and refutation; legacy records without polarity cannot publish until versioned and reverified. The workflow-facing validator and staging path require 25 exact fixed-path closure roles, recompute every member hash, and revalidate the source, claim, formalization, evidence, job, report, policy, environment, module, parser-derived protected dependency output, and local/protected axiom relationships. Hash-only CLI and MCP ingestion resolve that immutable stage, run an isolated copy of the Linux-amd64 SHA-pinned GitHub CLI with fixed arguments, parse one closed attestation result, retain the raw output and canonical CAS attestation-verification record, then register the separate immutable SQLite ingestion receipt plus the logical stage/actor idempotency result in one final transaction that rechecks currentness. Missing or altered CAS, stale state, key rebinding, parser ambiguity, and unpinned or misbehaving verifiers fail closed. The protected producer fails unless GitHub reports an actively protected source ref, then builds a real no-import candidate under clean-checkout, Bubblewrap network/process isolation, timeout, memory, and output-size controls; pull-request CI runs it without attestation, while protected `main` attests the exact candidate report. Every request, report, stage, validation result, attestation-verification record, and ingestion receipt remains structurally non-authoritative. The attestation-verification record and its ingestion receipt prove provenance only and may not be treated as a passed report or proof. Merged-tree controlled-ingestion evidence is complete and recorded below.

Phase 3 authority-gate evidence: closed `evidence/2` and nested `publication_authority_binding/1` contracts preserve `evidence/1` identities while allowing only accepted authoritative Lean kernel proof/refutation evidence with a complete receipt-bound CAS closure. Hash-only CLI and MCP promotion accept only an ingestion receipt hash plus attribution. The application revalidates the report, closure, all 25 members, bundle, raw output, canonical receipt, current Store snapshots, request derivation, and attestation parser; explicitly requires a `passed` report; and derives evidence kind from typed polarity. Controlled ingestion immutably binds the receipt's exact subject to the registered private generated publication-request artifact. The Store accepts only a non-deserializable derived commit and uses one immediate transaction to recheck that receipt/request/subject relationship, stage projections, current head, environment, polarity, uniqueness, and idempotency. Migration 0011 rejects every kernel/authoritative/v2 row outside the fixed shape. Statement fidelity is intentionally not an authority input; a later truth read must combine current fidelity with current proof authority. One exact protected proof record is now independently audited. The proof/refutation checkbox remains conservative until a protected refutation artifact is also recorded; the separate refutation type and Store path are already adversarially tested. Full SPEC replay remains a separate open capability.

Remote controlled-ingestion evidence: PR #34 merged as `95bd8a1d2068612b5eca644c3d77754b5e4f49fd`, tree `4027d03fe05bd997108c86250adcf2d920adda48`; main CI run `29721420110` passed all five jobs. Protected publication run `29721420136`, job `88285050682`, passed exact candidate construction, attestation, independent challenge, staging, controlled ingestion, and retention. Artifact `8452528096` has GitHub archive digest `66cd47753a77f90bf216b874d4d7c99f7ba561a4586e29c524e094c82eb3206c`. Downloaded inspection recomputed the report (`5f02696eac1308f48eae9f085f003e750f009dbe496d055543792b088e3b2aa6`), 25-role closure (`6440836d612d91dfe43af952d6ac83ca1445bf75c86b8b52a8c79f6121427b38`), candidate bundle (`c02cde0384cd015bbb085d2f98e69c245888dd7cfbc10973b0c9a9e722ad326c`), stage (`e4b307416358ef657c57d81d07c6b21df0381befbf5600982442bc435d98baa5`), raw verifier output (`bdfe54e9e889c52c7dc32941dc8fb16d50b1e2b8071a5b69be04eb1040d45932`), and canonical CAS attestation-verification/SQLite receipt key (`659b789a41a14ac59ca9253c2c71d73c4717acc58dcc2187809e92c710814402`) hashes. All 71 retained files, eight stderr files, immutable repository/owner identities, one subject, one source dependency, Rekor timestamp `2026-07-20T06:22:33Z`, and 18 false authority fields passed audit. This is exact provenance evidence only.

Remote authority evidence: PR #36 merged as `0ef8a132dfb99e63238325ac035de948e893f791`, tree `f3ed084c9c495de68e20a5a9cdda818fd4637d6b`. Main CI run `29726693096` passed all five required jobs. Protected publication run `29726693055`, job `88301341952`, completed exact candidate generation, both attestations and independent challenges, controlled ingestion, atomic promotion, and retention. Artifact `8454565233` contains 72 files; GitHub's archive digest and a separate downloaded ZIP both hash to `fae9d075ab9fbd4388474ed51458e6707ba8b9e337f9b18800a8f86e19a0e828`.

The independent audit recomputed the request (`3cfe3ebfa30e4446b659081cc8874ccbca684fd51d3b2c53a98b1534423a501c`), policy (`5e9f69968913a7f1f716baa1a1e548f27d5d6958582e75278b37f3fac245f167`), report (`31934fcc89b045f04f57d4a0807c0f599d188887be56507f296a9fe4d8c9e643`), 25-role closure (`20059d3ab54b2e075a7476250cf4de73f9a6c8196dd0005a7a96281d789c87df`), bundle (`99850335bf53d095653fbf59b777cbcc9c33d6fe0a769d71f3c5b8701aa9b42f`), stage (`527e4db5e0852268e1eda4d9dc5aa0e32432c6ac76e2b06f130e16aac7dbc9c6`), raw verification (`6adc6e41385249651adf372ece8334c650013b549093587891b36cafa7087453`), and receipt (`56166a02475a1d47be9013a024693bc3228801ad98d62c65fca06e6aebd90e40`) identities. Final evidence `019f7e90-7adb-7f03-8e37-e591b13eac4d` has canonical hash `dbaecfb7163476b98aead72d742891c7d8502494e81bf37e81aa5b214a9ef8cc`, kind/result/class `lean_kernel_proof` / `accepted` / `authoritative`, and an exact nine-field binding to that chain. All eight stderr files are empty, all 18 boolean authority fields remain false, and only the final `evidence/2.authority_class` is authoritative. It names one exact formalization version, has no local producing run/job, supersession, or stale state, and records the protected actor/timestamp. This completes issue #21 but creates no source-claim truth status.

Remote derived-status evidence: PR #39 merged final reviewed head `849dce08a84f70c7f4e1a00feb78aa863752b00f` as `8e65549d40b443497651b7dced2dc72d1b31335e`, tree `ca25e370a0ea5aa0312cef1e66720945a760da6b`. Exact-head PR run `29856409555` and main run `29856660451` passed all five required jobs. Protected publication run `29856660579`, job `88722566681`, retained the authority-only and post-fidelity CLI reads. Artifact `8505590232` contains 78 files and has GitHub and independently downloaded archive SHA-256 `e10111c9630b33e022d73ef45f716d958eb63491024b787a59e3949898be982f`.

The independent audit matched every archive member and all 25 closure roles; parsed 54 JSON files plus 25 JSONL closure entries; and recomputed report `6fdd7982316e28be2dd7a69b2ec743a37cae5479807e77ee00db749e9499b09b`, closure `4e2acef7c87d19421e0305eb1dcfbd7cf5bc6e701528bd815799cc008fc8a7bb`, stage `6036e9c90cf8edc550e898912bdd2a8672e3b76cd4ba6ba31318e5f9378dc2e5`, receipt `61b804b1888f71346e15e6e96452af58a34703507cdbf9d637834b0272ae1cf1`, bundle `27fbff08373c79d348b85495ba7e6c821386060bfb4c8f247cce3ab83528e217`, and raw-verification `fca79f52d98c047dde52f49c6d0e417506c30b27bccfe97ffdb316204fa0391e` identities. The exact claim first returns `open/no_current_verified_fidelity` with authoritative proof present, then returns `proved` with one witness only after distinct reviewer `protected-fidelity-reviewer` records v2 `claim` request `b12e35aca5c921e5af0cc8b1f6b7aebff3b20f2cc0467cb79ee89961ca554785`, report `c0a74cda75ab6e919c3b216a4d0b247bd3bd7133c088293827ec034490ae0708`, and reviewed evidence `afb6304b0af81b8dc18a51d41636519b5276e30ce73fdff2eb57c3af5eb5bdb3`. The attestation binds the exact protected workflow, commit, run, hosted runner, repository IDs, and Rekor timestamp `2026-07-21T18:19:58Z`. No Boolean authority assertion exists; the sole authoritative semantic is the accepted receipt-bound `evidence/2.authority_class`.

Remote local-audit evidence: GitHub Actions run `29704542965` passed the exact real Lean audit lifecycle and fresh Linux and Windows suites on tree `30d55d6e2ce7b0de2d921cff3e1368124fd9f66f`. That checkpoint validates local diagnostics only; the later protected authority evidence above is the basis for the checked pin and policy items, not for the still-open SPEC replay item.

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
