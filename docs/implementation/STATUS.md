# Implementation Status

Last updated: 2026-07-19

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

Active branch: `feat/publication-proof-authority`.

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
- Draft PR #22 carries the first hosted publication-boundary smoke. Runs `29706858126` through `29707579646` progressively exposed missing isolation software, hidden toolchain lookup, namespace identity, path traversal, and read-only mount assumptions. Run `29707668753` reached Lean inside the isolated namespace and proved that a 1 GiB address-space ceiling was too small for Lean 4.32.0 to initialize its runtime threads. The explicit limit is now 4 GiB and is recorded in the candidate report; the control remains enforced.
- Keep proof authority and mathematical status impossible while fidelity and publication-profile controls remain incomplete.
- Issue #20 is complete with exact-tree CI evidence. Issue #21 must now bind clean-checkout verification, dependency closure, retained artifacts, policy identity, and non-forgeable report provenance before authoritative proof or refutation evidence can exist.
- The first issue #21 slice defines closed publication policy, request, and candidate-report contracts. The policy pins repository, protected workflow, main ref, GitHub-hosted runner, Lean toolchain, allowed axioms, required isolation controls, SLSA predicate, and action commit identities.
- Publication requests separately name proof and refutation outcomes and bind exact diagnostic, proof-closure, axiom-audit, environment, module, declaration, policy, Git commit, and Git tree identities.
- Candidate reports must remain non-authoritative. A passed candidate fails validation if clean checkout, dependency closure, network isolation, memory enforcement, allowed axioms, retained artifacts, workflow identity, source identity, or policy identity is missing or inconsistent.
- ADR-0006 requires GitHub OIDC and Sigstore attestation of the exact report bytes, followed by repository, workflow, ref, commit, predicate, runner, and subject-digest verification before authority promotion. Candidate generation and attestation ingestion remain active work.
- The publication boundary smoke uses a clean checkout, pinned Lean, read-only root mount, separate mount, PID, and network namespaces, a private temporary filesystem, and a one-gibibyte address-space limit. Its report remains explicitly non-authoritative.
- Pull-request CI exercises the isolation boundary. The protected `main` workflow additionally attests the exact smoke report with the SHA-pinned official GitHub action and retains report bytes, diagnostics, and the Sigstore bundle for 90 days.

## Next highest-priority criteria

1. Implement issue #21 publication-profile verification and authoritative exact proof/refutation evidence without weakening local isolation claims.
2. Prove that local diagnostics, caller-authored reports, and altered retained artifacts cannot cross the authority boundary.
3. Derive mathematical status only from exact current proof and fidelity evidence.
4. Complete Pilot A through the real interfaces only after both authority and fidelity controls exist.

## Exact last validation commands

Run from the repository root. The explicit local toolchain path is required only in this managed workspace:

```text
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo fmt --check
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo clippy --workspace --all-targets --all-features -- -D warnings
PATH="$PWD/.toolchains/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" RUSTUP_HOME="$PWD/.toolchains/rustup" CARGO_HOME="$PWD/.toolchains/cargo" cargo test --workspace
PYTHONPATH=src PYTHONWARNINGS=error::ResourceWarning python -m unittest discover -s tests -v
git diff --check
```

Observed validation evidence for this update:

- formatting passed;
- warnings-denied Clippy passed;
- 76 Rust unit tests passed;
- 9 Rust CLI integration tests and 3 Rust MCP subprocess integration tests passed;
- 39 legacy Python regression tests passed;
- patch whitespace validation passed.
- GitHub Actions run `29699563931` passed all five jobs for the completed MCP invalid-action and environment-persistence state, including exact pinned Lean availability and both Rust operating-system targets.
- GitHub Actions run `29700398370` passed all five jobs for the canonical artifact slice, including both Rust operating-system targets.
- GitHub Actions run `29700933580` passed all jobs for the durable verifier input, leasing, recovery, CLI, and migration slice on its exact remote tree.
- GitHub Actions run `29701916437` passed all jobs for exact contained execution, including real Lean 4.32.0 acceptance and rejection plus wrong-version refusal.
- GitHub Actions run `29703359524` passed all jobs for exact diagnostic evidence, including the real pinned Lean worker and fresh Linux and Windows suites.
- GitHub Actions run `29704542965` passed all jobs for local proof-closure and axiom-audit evidence, including the exact real Lean lifecycle and fresh Linux and Windows suites.
- GitHub Actions run `29706138708` passed all jobs for controlled statement-fidelity evidence on exact tree `80b1d2e92e81192a2863bb445a7bef872fc21b72`, including fresh Linux and Windows, real storage, legacy regression, and pinned Lean jobs.
- The managed workspace still lacks Lean; local `mcl doctor` correctly reports that one unhealthy check while database, CAS, leases, environments, and artifact inventory remain healthy.

## Release readiness

Not ready. The release checklist is overwhelmingly open, all four mandatory pilots are incomplete in the specified architecture, later verifier, pedagogy, and release MCP families do not exist because their application capabilities do not yet exist, and no authoritative Lean evidence has been produced.
