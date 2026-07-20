# Implementation Status

Last updated: 2026-07-20

## Release truth

MathOS 1.0.0 is not complete and has not been released.

The binding contract is the root [SPEC.md](../../SPEC.md). The former Python finite-domain implementation is retained as legacy migration input. It is not the canonical product implementation and does not satisfy the 1.0 Definition of Done.

## Current phase

Transition from Phase 2 canonical interfaces to Phase 3 formalization and Lean authority.

Active implementation issue: [#21, define publication-profile proof authority and retained evidence](https://github.com/Mnehmos/MathOS/issues/21).

Issues [#14](https://github.com/Mnehmos/MathOS/issues/14) and [#15](https://github.com/Mnehmos/MathOS/issues/15) closed after GitHub Actions run `29699563931` passed all jobs on fresh Linux and Windows runners.

Issue [#16](https://github.com/Mnehmos/MathOS/issues/16) closed after GitHub Actions run `29700398370` passed all jobs on the exact canonical artifact tree.

The first issue #17 durable verifier-job slice passed all jobs in GitHub Actions run `29700933580` on its exact remote tree. It remained non-authoritative while contained execution was completed in later slices.

Issue [#17](https://github.com/Mnehmos/MathOS/issues/17) closed after GitHub Actions run `29701916437` passed all jobs, including real accepted and rejected Lean modules, wrong-version rejection, and fresh Linux and Windows suites on exact tree `724cbd2332c988c874462ea6d825fd93b2c4d809`. This closes contained diagnostic execution only, not proof authority.

Issue [#18](https://github.com/Mnehmos/MathOS/issues/18) closed after GitHub Actions run `29703359524` passed all five jobs, including real pinned Lean integration, on exact remote tree `7622bf7c2408061104f2b27fcca0fb451d4653a2`. This closes immutable diagnostic-evidence binding only, not proof authority.

Issue [#19](https://github.com/Mnehmos/MathOS/issues/19) closed after GitHub Actions run `29704542965` passed all five jobs, including the real pinned Lean audit lifecycle and fresh Linux and Windows suites, on exact remote tree `30d55d6e2ce7b0de2d921cff3e1368124fd9f66f`. This closes local proof-closure and axiom-audit evidence only, not proof authority or statement fidelity.

Issue [#20](https://github.com/Mnehmos/MathOS/issues/20) closed after GitHub Actions run `29706138708` passed all five jobs, including role-separated fidelity review through real CLI and MCP paths, adversarial provenance and corruption checks, the pinned Lean integration, and fresh Linux and Windows suites on exact remote tree `80b1d2e92e81192a2863bb445a7bef872fc21b72`. This closes statement fidelity evidence only, not proof authority or mathematical truth status.

PR [#22](https://github.com/Mnehmos/MathOS/pull/22) merged the isolated publication-boundary smoke after all five checks passed in run `29707914210` on exact tree `496e8b5dc71550aa83f11e6ba659f3353195741a`. Protected `main` publication run `29707995584` and ordinary CI run `29707995606` then passed on merge commit `31ccfdda41d538bde7e01e061865580cab2f04e5`.

PR [#23](https://github.com/Mnehmos/MathOS/pull/23) passed all five checks in run `29708510408` on exact tree `a2f57f427d5e59b4042bcbdd703436595014bdd3` and merged as `47346cd4a378716711e6b4bbc079e847ab2621b5`.

PR [#26](https://github.com/Mnehmos/MathOS/pull/26) corrected the attestation policy's `jq` object iteration. All five PR checks passed in run `29709569292` on exact tree `e5b0bcbf4eacf9cb402d87b04b5dc9b0431134f2`, and the PR merged as `6b4a0f22d3498a4cb0c8dab744da7f9a09993fd8`.

Active branch: `main`; issue #21 continues with controlled ingestion of a real publication closure.

## Completed criteria with evidence

- Root normative specification exists and includes sections 0 through 37.
- The package and legacy Python adapter report version `0.1.0`, removing the false local 1.0 assertion.
- The Rust `mcl` binary compiles from `Cargo.lock` and exposes only implemented Phase 1 commands.
- `mcl init` creates a real SQLite database in WAL mode, applies all committed migrations, and writes a SHA-256 content-addressed canary.
- `mcl health` checks database integrity, migration state, WAL mode, FTS5, and artifact-root containment without creating a missing database.
- `mcl doctor` adds artifact round-trip, stale-lease, and Lean availability checks and exits nonzero when unhealthy.
- Artifact paths reject malformed hashes, parent traversal, and a symlinked artifact root.
- Configured database and artifact paths reject parent traversal and existing-ancestor symlink escape.
- Rust unit and CLI integration tests use real temporary SQLite databases and artifact stores.
- Lean is pinned in `lean-toolchain` to `leanprover/lean4:v4.32.0`.
- Canonical JSON uses RFC 8785 with fail-closed IEEE-754 safe-integer validation and a golden cross-language hash vector.
- Stable canonical objects use UUIDv7; immutable versions use the specified schema-bound SHA-256 formula.
- Create and version mutations persist actor attribution and immutable idempotency receipts.
- Compare-and-swap heads serialize concurrent writers into one winner and one structured conflict.
- Database triggers reject version rewrites, head clearing, head downgrade, cross-object heads, identity rewrites, and idempotency-receipt mutation.
- Exact object and version lookup, restart persistence, and current-head FTS5 projection work through the real SQLite store.
- All 30 specified logical, pedagogical, research, provenance, and implementation edges are exhaustive Rust variants.
- Edge endpoints bind exact versions owned by exact stable objects; edge payloads are canonical JSON and edge rows are immutable.
- Hard pedagogical prerequisites remain acyclic through both application checks and SQLite triggers, while logical equivalence cycles remain valid.
- All 11 specified run kinds and a closed execution-event vocabulary are exhaustive Rust variants.
- Run creation atomically records actor, canonical budget, UUIDv7 identity, and a hash-chained origin event.
- Event append uses expected-head compare-and-swap and immutable idempotency receipts; concurrent writers produce one winner and one structured conflict.
- SQLite anchors and triggers reject missing predecessors, gaps, rewrites, deletion, and run-origin mutation.
- Chain verification detects forged payloads, reordered events, and final-event truncation, including after restart.
- Run history remains explicitly non-authoritative for proof, fidelity, and novelty.
- Graph traversal begins from an exact object and version pair and preserves exact version-bound edges in every result.
- Incoming, outgoing, and bidirectional traversal support typed edge-kind filters without accepting raw query text.
- Depth, result count, and scanned edges are bounded; cycles terminate without duplicate edge results.
- Traversal ordering is deterministic across restart and remains read-only and non-authoritative.
- Source and claim payloads have separate closed Rust types and committed JSON Schemas for `source/1` and `claim/1`.
- Source records explicitly preserve original text, locator, licensing, redistribution, citations, redaction, and provenance.
- Claim records explicitly preserve exact source reference, normalized statement, kind, assumptions, variables, concept links, citations, and ambiguity.
- Canonical create and version paths reject unknown fields, unsupported schema versions, malformed hashes, empty required text, and excessive collections before persistence.
- Schema rejection leaves no record or idempotency receipt, while valid original source text survives restart byte-for-byte.
- Concept payloads have a closed Rust type and committed `concept/1` JSON Schema covering aliases, domains, formal declarations, licensed taxonomy crosswalks, pedagogy references, and provenance.
- Formalization payloads have a closed Rust type and committed `formalization/1` JSON Schema covering one exact claim version, Lean environment, module artifact, declaration identity, theorem type, imports, notes, and separate evidence references.
- Formalization payloads reject embedded `proved`, `disproved`, `faithful`, and `certified` verdicts. These conclusions remain outside the formalization record.
- One claim can retain multiple formalization objects, and changes to theorem type, environment, module artifact, or imports produce different canonical hashes.
- A formalization must reference an exact existing claim object and version. Missing references and references to other record kinds fail before persistence.
- GitHub Actions run `29696708243` passed Rust tests and warnings-denied lint on fresh Linux and Windows runners, the real-storage smoke test, and all legacy Python regression tests.
- The fresh Linux runner installed the exact pinned Lean 4.32.0 toolchain from a SHA-256-verified Elan installer and executed `lean --version` successfully. This establishes toolchain availability only, not proof authority.
- Sources, concepts, claims, and formalizations now use one typed application service for CLI create, version, exact retrieval, and dry-run validation.
- CLI entity mutations bind the committed schema version, require actor and idempotency attribution, and preserve compare-and-swap versioning.
- Canonical FTS search is available through that same application service, and CLI integration covers dry-run non-mutation, create, version, current and historical reads, restart, search, and wrong-family rejection.
- Version-bound edge creation, exact edge retrieval, and bounded typed graph traversal now use the same application service and CLI path.
- Research run creation, retrieval, event listing, event append, and hash-chain verification now use that shared path while remaining explicitly non-authoritative.
- Edge, run, and run-event dry runs validate without mutation. Real mutations preserve store-level idempotency before evaluating changed current state.
- CLI adversarial coverage caught and fixed an application-layer retry-order defect, then verified identical event retries, stale-head conflicts, graph bounds, restart persistence, and chain validity.
- Golden fixtures pin representative record-mutation, edge-mutation, and run-chain JSON response shapes after normalizing only dynamic identities and timestamps.
- The issue #13 CLI surface contains no proof, disproof, fidelity, novelty, certification, raw SQL, arbitrary shell, or unrestricted executable action. Its only process launch remains the allowlisted Lean availability check in `doctor`.
- CLI integration rejects stale canonical version writers without changing the accepted head.
- ADR-0004 pins the MCP `2025-11-25` stable protocol, stdio transport, exact official Rust SDK release, one-way application-service dependency, and disabled inference and network capabilities.
- `mcl serve` now runs a real MCP `2025-11-25` server over newline-delimited stdio through the exactly pinned official Rust SDK.
- The initial MCP surface exposes only closed `system` and `query` families. It provides identity, health, capability, policy, exact record, FTS5 search, and bounded graph actions without direct storage access.
- MCP tool schemas have an object root, reject unknown fields, bound search and graph work, and return stable application errors as structured tool failures.
- Real subprocess tests exercise initialization, tool discovery, tool calls, invalid parameters, forbidden tool names, stdout purity, clean EOF shutdown, restart, and persisted-state recovery.
- CLI-created canonical state produces the same serialized search and exact-record results when read through MCP, establishing parity for the implemented read surface.
- The MCP surface now includes all six issue #14 families: `system`, `query`, `source`, `claim`, `formalization`, and `research`.
- Source, claim, and formalization proposals and versions require explicit actor and idempotency attribution; versions additionally require compare-and-swap object and head identities.
- Research start and submit actions require the same attribution controls, while observe remains read-only. Every recorded run remains explicitly non-authoritative.
- MCP dry runs validate without mutation, exact retries return the original result, stale writers fail with structured conflicts, and irrelevant mutation fields fail closed.
- Adversarial MCP coverage rejects embedded proof verdicts in formalizations and confirms that no `mark_proved`, raw shell, raw SQL, model-routing, or publication action exists.
- The environment domain now has a closed `environment/1` Rust manifest, committed JSON Schema, and golden canonical SHA-256 fixture.
- Environment identity includes exact Lean release, dependency revisions, imports, project configuration hashes, platform, trust profile, typed verifier command, explicit resource limits, network policy, and working-directory policy.
- Environment validation rejects unpinned dependencies, unknown and machine-specific fields, path-shaped imports, duplicates, noncanonical ordering, arbitrary commands, network-enabled verification, unsafe hashes, and zero or excessive limits before hashing.
- Changing an environment-relevant field changes the canonical environment hash. This establishes context identity only and does not establish a proof.
- Migration 0006 adds immutable environment attribution and database triggers that reject environment update and deletion.
- Environment registration is content-addressed, actor-attributed, idempotent, durable across restart, exactly retrievable, and deterministically listable through the shared application service and CLI.
- Environment dry runs validate and predict the exact hash without mutation. Idempotency-key reuse for a different manifest fails closed.
- Environment reads recompute manifest identity and trust profile, detecting corrupted stored JSON even if database mutation guards are bypassed.
- `mcl doctor` reports registered environment count while explicitly stating that environment identity does not establish proof authority.
- New formalizations must reference an exact registered environment hash. A syntactically valid but unresolved hash fails before canonical record persistence.
- Artifact metadata has a closed `artifact_metadata/1` Rust type and committed JSON Schema for media type, creation source, license, restriction, and bounded semantic metadata.
- Artifact semantic metadata cannot claim proof, disproof, fidelity, certification, or authority.
- Lean source, text, and JSON bytes are checked against their declared media type before CAS ingestion. Empty and excessive artifacts fail closed.
- CAS ingestion is atomic and content-addressed. Immutable SQLite metadata adds actor attribution, idempotent retry, exact lookup, deterministic listing, restart persistence, and corruption detection.
- Doctor inventories registered metadata against CAS, reports safe unregistered orphans and incomplete crash-window files, and fails when registered bytes are missing. It never promotes filesystem contents to canonical state.
- `mcl artifact ingest/get/list/verify` uses the shared application path. Dry runs predict the exact SHA-256 identity without writing bytes or metadata.
- CLI input paths must resolve to regular files inside the instance root. Symbolic links and outside-root paths fail closed.
- Verified artifact materialization accepts only a fresh temporary workspace and one verifier-selected plain file name. Traversal and overwrite attempts fail.
- Formalizations now require their exact module artifact to be registered as `text/x-lean`. Missing and wrong-media artifacts fail before persistence.
- Verifier intent has a closed `verifier_request/1` Rust type and committed JSON Schema containing only exact environment and Lean source hashes plus a bounded declaration name.
- Verifier requests expose no executable, shell, working directory, environment-variable, model, provider, or mathematical-status field.
- Migration 0008 persists canonical request JSON, SHA-256 input identity, actor, priority, UUIDv7 job identity, state, lease, attempts, progress, result reference, and structured last error.
- Database triggers reject verifier job identity rewrites, deletion, and illegal state transitions.
- Enqueue is actor-attributed and idempotent. Exact retries return the original job, changed retries fail closed, and missing environments or non-Lean artifacts never enter the queue.
- Worker leases are bounded and transactional. Only one worker can select a queued job, wrong-worker starts fail, and expired leased or running jobs return safely to the queue without losing attempt history.
- `mcl verify check/status/list` uses the shared application service. Dry runs validate exact references and predict input identity without mutation.
- Verifier jobs remain explicitly non-authoritative. No evidence record or derived mathematical status exists in this slice.
- A worker leases at most one canonical job, resolves its exact environment and Lean source artifact, and invokes only the configured `lean` or `lean.exe` executable with verifier-selected arguments.
- Lean runs in a fresh temporary workspace below the instance root with null stdin, a cleared environment, a narrow runtime-variable allowlist, wall-clock timeout, and a combined retained-output bound.
- The source policy rejects explicit holes, custom source axioms, unsafe declarations, native evaluation, command elaborators, initialization hooks, and file inclusion before launch.
- Each completed attempt stores bounded stdout and stderr plus a canonical execution report as private content-addressed artifacts. The terminal job links the exact report and cannot be rewritten.
- Execution classifications distinguish elaboration, Lean rejection, unsafe source, timeout, output exhaustion, toolchain mismatch, and launch failure from operational job state.
- Every local execution report is permanently non-authoritative and records that memory and network isolation are not enforced. The worker refuses publication-profile environments.
- ADR-0005 documents the contained local process boundary and its limitations. Dependency closure, proof evidence, proof-closure scans, axiom audits, fidelity review, and publication isolation remain separate gates.
- Evidence now has a closed `evidence/1` Rust contract and committed JSON Schema with exhaustive evidence kinds, explicit result and authority classes, exact subject version, producing run or job, artifact set, environment, identity, supersession, and staleness fields.
- Evidence payload identity is deterministic and rejects unsorted or malformed artifact sets, missing provenance, malformed references, and inconsistent staleness metadata.
- Migration 0009 adds content identity and provenance projections to the evidence table. Database triggers reject subject/version mismatch, update, and deletion.
- `mcl verify promote-diagnostic` is the only public evidence-creation path. It derives a closed payload from an exact formalization and completed verifier job rather than accepting caller-authored evidence fields.
- Diagnostic promotion requires exact agreement on formalization object and version, environment, module, declaration, job, execution report, and complete diagnostic artifact closure.
- The execution report is re-read from CAS, hash-checked, schema-validated, and required to carry private verifier provenance for the exact job before evidence can persist.
- Public artifact ingestion can no longer claim verifier, generator, importer, or migration provenance. Those identities belong only to their controlled application paths.
- Evidence creation is deterministic and actor-attributed. Exact retries return the same record, reads survive restart, and projection corruption is detected by recomputing identity and comparing every stored field.
- CLI integration covers dry-run non-mutation, exact promotion, retrieval, deterministic listing, retry, cross-object versions, mismatched formalizations, forged report provenance, corrupted CAS bytes, and unchanged mathematical status.
- Diagnostic elaboration remains structurally non-authoritative. No authoritative evidence or mathematical status exists.
- The local Lean audit policy is a closed, canonical, content-identified contract that rejects holes, custom axioms, unsafe declarations, native evaluation, command elaborators, initialization hooks, file inclusion, and metaprogramming escapes on the submitted authority path.
- Audit requests bind one exact formalization version, accepted diagnostic elaboration record, environment, Lean module, declaration, and committed policy hash. Callers cannot submit an audit verdict.
- `mcl verify audit/status/list` and `mcl worker --job-kind audit` use durable SQLite jobs with idempotent enqueue, transactional leases, legal state transitions, bounded contained execution, restart recovery, and private CAS reports.
- The audit worker appends verifier-controlled `#print axioms` inspection, accepts one declaration-specific bounded output, records transitive axiom dependencies, and rejects unexpected axioms outside the narrow standard allowlist.
- Audit reports are schema-closed, hash-bound to their request and artifacts, and permanently non-authoritative. Local reports state honestly that memory and network isolation are not enforced.
- `mcl verify promote-audit` revalidates the exact terminal report and atomically creates one diagnostic proof-closure scan plus one diagnostic axiom audit. Exact retries return the same pair, while partial or cross-object promotion fails closed.
- Unit coverage exercises policy identity, closed schemas, report shape, output ambiguity, duplicate axioms, job retry, policy mismatch, restart recovery, evidence-pair atomicity, immutable retrieval, and corruption detection. The real pinned Lean integration is delegated to the protected Linux CI job because this managed workspace has no Lean executable.
- Statement fidelity now has closed request and report contracts covering all six specified review levels, compatible verdicts, explicit findings, definition mappings, ambiguity disposition, exact lineage, supporting artifacts, producing runs, and supersession.
- Fidelity review uses the shared application service through both CLI and MCP. The attributed actor must be the named reviewer, and verified review requires role separation from the formalization author.
- The application, rather than the caller, creates a private canonical JSON report with controlled `human_review` provenance. Public artifact ingestion cannot forge that creation source.
- Fidelity evidence is immutable, reviewed rather than authoritative, bound to one exact formalization and run, deterministic under retry, and durable across process restart.
- Fidelity status is derived from exactly one unsuperseded evidence head. Earlier reviews remain visible as `superseded`; stale reviewers receive a compare-and-swap conflict.
- Status reads revalidate the complete evidence chain and controlled CAS reports. Integration coverage rejects self-verification, missing artifacts, substituted source lineage, erased ambiguity, stale heads, forged report reuse, and corrupted report bytes.
- Fidelity evidence does not alter proof, disproof, novelty, or publication status. Mathematical truth derivation remains unavailable.

These items establish only part of the product foundation and Phase 2 trace model. They do not establish any mathematical claim, Lean proof authority, complete MCP mutation surface, pilot, portable release, or 1.0 acceptance result.

## Active work

- Implement issue #21 publication-profile proof authority without allowing local diagnostic workers or caller-authored reports to self-promote.
- PR #22 established the first hosted publication-boundary smoke. Runs `29706858126` through `29707579646` progressively exposed missing isolation software, hidden toolchain lookup, namespace identity, path traversal, and read-only mount assumptions. Runs `29707668753` and `29707749539` showed that default host-sized thread creation exceeds the address-space ceiling. Run `29707844720` rejected the initially assumed `-j=1` syntax before executing. Run `29707914210` passed all five PR checks with Lean 4.32.0's observed `-j 1` form and the enforced 4 GiB limit.
- Keep proof authority and mathematical status impossible while fidelity and publication-profile controls remain incomplete.
- Issue #20 is complete with exact-tree CI evidence. Issue #21 must now bind clean-checkout verification, dependency closure, retained artifacts, policy identity, and non-forgeable report provenance before authoritative proof or refutation evidence can exist.
- The first issue #21 slice defines closed publication policy, request, and candidate-report contracts. The policy pins repository, protected workflow, main ref, GitHub-hosted runner, Lean toolchain, allowed axioms, required isolation controls, SLSA predicate, and action commit identities.
- Publication requests separately name proof and refutation outcomes and bind exact diagnostic, proof-closure, axiom-audit, environment, module, declaration, policy, Git commit, and Git tree identities.
- `mcl verify prepare-publication` and MCP `verify.prepare_publication` now derive that request from the current canonical formalization head and its exact accepted elaboration/audit chain. Typed `claim_polarity` maps `claim` to proof and `negation` to refutation; earlier records may omit it for read compatibility but cannot publish until versioned and reverified. The application reopens the controlled jobs and reports, verifies every referenced CAS object, and retains only private `generated` canonical request bytes behind a transactional head/idempotency check. Callers cannot submit request JSON, derived hashes, or authority fields, and polarity remains non-authoritative intent until fidelity and protected publication evidence are combined.
- Candidate reports must remain non-authoritative. A passed candidate fails validation if clean checkout, dependency closure, network isolation, memory enforcement, allowed axioms, retained artifacts, workflow identity, source identity, or policy identity is missing or inconsistent.
- ADR-0006 requires GitHub OIDC and Sigstore attestation of the exact report bytes, followed by repository, workflow, ref, commit, predicate, runner, and subject-digest verification before authority promotion. The policy now additionally pins GitHub CLI 2.96.0 by release archive and executable SHA-256.
- The publication boundary smoke uses a clean checkout, pinned Lean, read-only root mount, separate mount, PID, and network namespaces, a private temporary filesystem, one Lean worker thread, and a four-gibibyte address-space limit. Its report remains explicitly non-authoritative.
- Pull-request CI exercises the isolation boundary. The protected `main` workflow additionally attests the exact smoke report with the SHA-pinned official GitHub action and retains report bytes, diagnostics, and the Sigstore bundle for 90 days.
- Protected `main` publication run `29707995584` retained artifact `8448453305`. Its archive digest is `0e3d0007da30460b1918d98fea39fad08cffda4b0249035a88bb9a1cd2d30896`; the exact smoke report digest is `08bead82cea25ffdfc3424084cb0878f2a648b3375317daa0c799395751dba40`; and its Sigstore bundle digest is `5dd23a13fa66efef8885e2116bd994bce92244a290c3ce0d9467d5eeb6d5ac14`. Inspection confirmed the DSSE subject digest matches the report, while the certificate binds the protected workflow, `main`, exact source commit, push event, and GitHub-hosted runner.
- The protected verification path now emits a closed, permanently non-authoritative attestation-verification record after the pinned verifier challenges exact report bytes and the retained bundle. Controlled canonical ingestion and authoritative evidence creation remain unavailable.
- Protected run `29708636210` proved installation, isolated Lean execution, and attestation, then rejected verification because GitHub CLI 2.96.0 makes exact certificate identity and signer-workflow selectors mutually exclusive. The corrective slice keeps the stronger exact certificate SAN, removes the redundant selector, captures verifier stderr, and makes failed-attempt artifact retention unconditional.
- Protected run `29708831882` then completed cryptographic verification but exposed incorrect `jq` use of `all` in the final policy query. Failed artifact `8448611307` retained the exact report, bundle, raw verifier JSON, and stderr. PR #26 changed the predicate and timestamp constraints to `all(.[]; condition)`, retained the exact subject-digest check, and made policy rejection preserve diagnostics, emit a stable message, exit `71`, and omit the constrained success record.
- Protected publication run `29709634846`, job `88251709818`, passed on merge commit `6b4a0f22d3498a4cb0c8dab744da7f9a09993fd8` and tree `e5b0bcbf4eacf9cb402d87b04b5dc9b0431134f2`. Retained artifact `8448739399` has downloaded archive SHA-256 `a379d01a60157f6ad22de4d933e662a044684bc471ec528de6017e34519498af`. The smoke report hash is `15a7a938504f8c57a8693bf57efec7692c1c4270d3325304bae61234fb203021`, canonical report content hash is `c8cbeb76cc2092ed4b537a0686c00f2ce80a5ba8598a5d7d1d744f51eccd3904`, Sigstore bundle hash is `2aa75b909c67fe44f464f1b5d78f8f19c36bab0df1d41ffd4121ea5f445ab7a1`, raw verifier JSON hash is `fc47407047615b97ff25a1790ead1dfe090636202ab8ae779216040018b6cab4`, and constrained record hash is `de1ecfaa304bc403c76752a7fecd0906fe1ea1bfefdc7a94ed54e286e651bc4a`.
- The retained certificate binds the protected workflow, `refs/heads/main`, exact merge commit, push event, and GitHub-hosted runner. Its subject digest matches the report, its predicate is SLSA provenance v1, and its Rekor timestamp is `2026-07-20T00:33:02Z`. The constrained verification record remains `authoritative: false`; this is infrastructure evidence only.
- PR #30 merged canonical request preparation as commit `da33431a1061bb3f05db7a7d2473f1fb5b8059f2`, tree `620f2a3060290ffe37fb3051ff604fa4433679af`. Main CI run `29711916515` passed Linux, Windows, real storage, legacy Python, and real Lean jobs. Publication run `29711916501`, job `88257002282`, retained artifact `8449114011` with archive digest `3f8b0567519521a29d302ab6dd4f229c59ce2d823ddbf144db01d07458e0fd31`. Its smoke report, canonical content, Sigstore bundle, raw verifier result, and constrained-record hashes are `b6edcfc92ed622e847444629ce2b38c1f20afb7b9e065bb75a65bdaa69cb3af9`, `43859d5dac0eb406c39b9dfd08862a8b874bf9102de6fec79d1c36b821864778`, `58013a04bb8f10aac04d55402e605fc169c90abf11fcc22211e98fcbba23d759`, `541b74172ae4c12f2ce2b31ce2330917a090214c0f1358a3795fcf15f8d99eb0`, and `afdef248f30dd3b5d26ba7a342b9bba2a1e569a69fcf62145ee722c1cfd4e40a`. Both retained records remain `authoritative: false`.
- The next issue #21 slice defines `publication_retained_closure/1` as exactly 25 sorted, fixed-path roles covering the request, canonical source/claim/formalization chain, environment, module, policies, evidence, terminal jobs/reports/local logs, and protected rebuild, parser-derived dependency, and axiom-audit logs. Member bytes and semantic identities are separately hashed; the candidate report binds every member hash plus the canonical closure-manifest hash.
- `mcl verify validate-publication-candidate` is a semantically read-only workflow gate. It rejects noncanonical or oversized JSON, path or symbolic-link substitution, missing or altered member bytes, broken record/evidence/job/report relationships, stale request state, changed CAS objects, undeclared parser-observed dependencies, local or protected axiom-output mismatch, altered retention sets, and any authority assertion. It re-derives the exact request through the same application service, creates no canonical record or artifact, performs no promotion, and returns only `authoritative: false`; opening an instance may still create ordinary operational directories or SQLite WAL files.
- The protected workflow now refuses an unprotected source ref, constructs a fresh real no-import canonical lifecycle, retains its request-bound local diagnostic environment honestly, and independently rebuilds the exact module and verifier-controlled axiom driver inside GitHub-hosted Bubblewrap with timeout, a four-gibibyte Lean heap cap, and a six-gibibyte process address-space ceiling. It attests and independently challenges the resulting real `publication_report/1` while retaining the complete closure. Pull-request CI runs the same producer only in an explicit non-attested simulation and uploads bounded attempt evidence on failure.
- Security review found that `main` had no active protection despite the earlier protected-workflow label. A `gh api` branch-protection update now requires pull requests, strict passage of all five CI jobs, resolved conversations, stale-review dismissal, and admin enforcement; force-pushes and deletions are disabled. A live follow-up read reports `protected:true`, and both the workflow and producer independently require GitHub's immutable protected-ref context before generating a candidate.
- PR #31 CI run `29714982064`, job `88266362527`, preserved the first hosted candidate failure: the protected rebuild and parser-derived dependency scan passed, but the axiom driver aborted while creating a Lean thread under the four-gibibyte process address-space ceiling. The repair keeps a four-gibibyte Lean heap cap, raises only the OS address-space allowance to six GiB for runtime/thread overhead, and makes future PR failures upload their bounded structured attempt evidence.
- The next PR #31 run `29715179869`, job `88266955678`, proved that the memory repair works: all three protected Lean executions passed, the dependency output contained only pinned `Init.olean`, and the axiom audit reported no axioms. It then failed closed because the producer treated a direct job snapshot as a wrapped `.job` object and attempted to copy CAS identity `null`. Failed-attempt artifact `8450312976` (`sha256:8976c4a151487bae6b414e605216fcd86c1bfe99dd792afb2cc505d0c9e145db`) retained the exact partial closure, structured executions, intermediate records, and seven CAS objects. The corrected producer now extracts and validates the direct snapshot's report, policy, and canonical-input hashes before use.
- PR #31 CI run `29715394683` passed all five required jobs on head `bf4abfe4d67dcd09ef2d717c4a706308a15676df`; its pull-request merge commit `0d14b5819ef07a733cde09a3f54c097a96f11f3b` had tree `47a28313024731861618678b80ec56d6934fa2ff`. The hosted simulated candidate completed the real canonical lifecycle, all three protected Lean executions, exact 25-member closure construction, and shared application validation. Its request, closure, and report hashes were `d38608cdf8067b14050d45e74b7c1ddb741e5b86463bc66627d8aa82b6b61bdb`, `7dad2fa6b0bc14dba31c54b38fabaec5cd9bcaa812aa2d7877c6586380fa8cd5`, and `0bb66bee9114141efe9ae6202fe9b3931f88e460d1262d6df5cb82154eba8278`; the validation result remained `authoritative:false`.
- PR #31 merged as commit `55a0ae0fad9fe5cfd20ed0a5fbddf6b80a5303c1`, tree `7508582f4ac01b6cd4533c135d7da5f238ad2149`; main CI run `29716292647` passed all five jobs. The first protected candidate run `29716292659` passed the protected-ref gate, smoke execution, smoke attestation, and constrained smoke challenge, then failed closed with exit `65` because the script's report-vocabulary constant `RUNNER_ENVIRONMENT=github_hosted` shadowed GitHub's immutable `RUNNER_ENVIRONMENT=github-hosted`. Failed artifact `8450708477` retained the completed smoke evidence with archive digest `29c70446a4ba089b44621b254bd6ff54ca3a109a836ff8c60a4525e8e7059c27`.
- PR #32 renamed only that internal constant and merged as `fc9116492a60fd891bbb1a175096002c8ab504a4`, tree `5dc109a29888c7ac34e67cdd6fb84454947133e5`. Main CI run `29716676603` passed all five jobs. Protected publication run `29716676599`, job `88271222144`, then completed candidate construction, attestation, independent challenge, and unconditional retention. Artifact `8450846116` has GitHub archive digest `1e0ecbf1ca9250494e164ffe8fe093e4b2a99ca4aa34bcaa1126c9a698ff26e6`.
- Downloaded inspection of that first exact protected candidate verified all 25 sorted fixed-path roles and every member byte hash. The canonical request, retained closure, publication report, retained validator result, and attempt summary hashes are `9f32d3eca940100b92f6ceb91e975fb859f99c1cf97fb98ae859e6cd79419103`, `ff67bc6f101681ff5c8345c763036ca1c07caed4eec714ba8c35f5017a5c28e3`, `12f0aaca0f6e9a16c242205da24f53fadc08ba9177cff6e039ec394c50ee1012`, `1d5adfc932ffa566f859c45caba62d97443e80ee1a3eb4532f792e5ca69362c0`, and `503c86845b6e0a78fa3712b9cf999f78203c15df2550356e3965a12944740ea5`.
- Inspection also confirmed exact source commit, tree, run, and attempt binding; clean checkout; empty declared imports; only pinned `/opt/lib/lean/Init.olean` in protected dependency output; no axioms in both local and protected audits; seven intact retained CAS objects; and passed rebuild, dependency, and axiom execution records. The candidate Sigstore bundle, raw pinned-`gh` verification, and constrained verification-record hashes are `a23d190fdaffe7e87fc98747b32525f96f199aa05f5431cba39548b38207a84e`, `1e6063a8ca1ef58476992d8691f702cc39d547ce94d82ebf90a848eef75d549a`, and `93871363affcfcc6a36d954577779de5791752d2951340c90f77df095c81ce3a`. The certificate binds the exact protected workflow, `main`, merge commit, push event, and GitHub-hosted runner; its subject matches the report and its verified Rekor timestamp is `2026-07-20T04:22:41Z`. Every candidate, attempt, validation, and verification record remains `authoritative:false`.

## Next highest-priority criteria

1. Implement controlled application ingestion that revalidates the downloaded closure and attestation before creating any authority.
2. Create authoritative exact proof/refutation evidence only from that verified retained closure, never from caller-authored reports.
3. Derive mathematical status only from exact current proof and fidelity evidence.
4. Complete Pilot A through the real interfaces only after both authority and fidelity controls exist.

## Exact last validation commands

Run from the repository root. The explicit local toolchain path is required only in this managed workspace:

```text
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo fmt --check
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo clippy --workspace --all-targets --all-features -- -D warnings
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo test --workspace --all-targets
MCL_RUN_LEAN_INTEGRATION=1 PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo test --test lean_worker -- --nocapture
PYTHONPATH=src PYTHONWARNINGS=error::ResourceWarning python -m unittest discover -s tests -v
bash -n scripts/publication-candidate.sh scripts/publication-smoke.sh scripts/publication-attestation-verify.sh
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 -color=false
git diff --check
```

Observed validation evidence for this update:

- Home workstation: Windows 10.0.19045, PowerShell 5.1.19041.6456, Rust 1.97.1, Python 3.12.10, GitHub CLI 2.70.0, GNU Bash 5.2.21 through WSL, and Lean 4.32.0 `x86_64-w64-windows-gnu`.
- Formatting, warnings-denied Clippy across all targets and features, all 102 default Rust tests, the opt-in real Lean 4.32 lifecycle, all 39 legacy Python regressions, Bash syntax, Actionlint 1.7.12, YAML parsing, CLI help surfaces, and patch whitespace validation passed for the retained-closure and protected-candidate slice.
- The corrected policy passed against retained artifact `8448611307`; independent report-digest, predicate, empty-timestamp, and empty-attestation mutations all failed closed.
- `mcl init`, `mcl health`, and `mcl doctor` passed against a fresh Windows instance. Doctor observed the exact pinned Lean 4.32.0 toolchain and honestly reports the local profile.
- PR #29 corrected the Windows-specific Lean platform expectation and temporary-database cleanup assumptions. Main CI run `29709993224` and publication run `29709993225` passed after that repair.
- PR #26 CI run `29709569292` passed fresh Linux, Windows, storage, legacy Python, and pinned Lean jobs on exact tree `e5b0bcbf4eacf9cb402d87b04b5dc9b0431134f2`.
- GitHub Actions run `29699563931` passed all five jobs for the completed MCP invalid-action and environment-persistence state, including exact pinned Lean availability and both Rust operating-system targets.
- GitHub Actions run `29700398370` passed all five jobs for the canonical artifact slice, including both Rust operating-system targets.
- GitHub Actions run `29700933580` passed all jobs for the durable verifier input, leasing, recovery, CLI, and migration slice on its exact remote tree.
- GitHub Actions run `29701916437` passed all jobs for exact contained execution, including real Lean 4.32.0 acceptance and rejection plus wrong-version refusal.
- GitHub Actions run `29703359524` passed all jobs for exact diagnostic evidence, including the real pinned Lean worker and fresh Linux and Windows suites.
- GitHub Actions run `29704542965` passed all jobs for local proof-closure and axiom-audit evidence, including the exact real Lean lifecycle and fresh Linux and Windows suites.
- GitHub Actions run `29706138708` passed all jobs for controlled statement-fidelity evidence on exact tree `80b1d2e92e81192a2863bb445a7bef872fc21b72`, including fresh Linux and Windows, real storage, legacy regression, and pinned Lean jobs.
- Protected publication run `29709634846` passed on the exact merged tree and retained the constrained, non-authoritative verification evidence recorded above.

## Release readiness

Not ready. The release checklist is overwhelmingly open, all four mandatory pilots are incomplete in the specified architecture, later verifier, pedagogy, and release MCP families do not exist because their application capabilities do not yet exist, and no authoritative Lean evidence has been produced.
