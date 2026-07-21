# Canonical pedagogy

MathOS stores pedagogy as immutable `learning_unit/1` records. A learning unit can explain mathematics or organize practice, but it cannot mark a claim proved, disproved, faithful, or authoritative.

## Lifecycle

1. Ingest the exact lesson content as a text or JSON artifact with `artifact_role=learning_unit_content`.
2. Propose a complete draft learning-unit payload through `mcl pedagogy propose` or MCP `pedagogy` action `propose`.
3. Revise with compare-and-swap through `version`; authored versions remain draft.
4. Record an actor-bound `reviewed` or `rejected` decision through `review`.
5. Bind prerequisite and semantic links to the reviewed version through `link`.
6. Run `validate`; only fully grounded, policy-compliant records whose prerequisite payload and edges agree pass.
7. Query a reviewed prerequisite or recommended path through `path`.

Review creates a new immutable version. Any prerequisite edges created for the draft remain historical, so links used by validation and path queries must name the reviewed version.

## Fail-closed policy

- Targets, sources, prerequisites, related units, and formalizations must resolve to exact current canonical versions of the expected kind.
- Related example, nonexample, counterexample, misconception, exercise, mastery-check, application, and frontier fields must resolve to matching learning-unit kinds.
- Content bytes are rehashed from CAS. The content must be text or JSON, carry the learning-content artifact role, and have the exact license recorded by the unit.
- Training-eligible units must be reviewed and licensed. Public eligibility additionally requires public content and publicly redistributable, publicly redacted, licensed grounded sources.
- Rejected units are ineligible or quarantined.
- Hard and soft prerequisites remain distinct. The same exact unit cannot be both, and hard cycles are rejected.
- A path contains only current reviewed units and uses bounded deterministic graph traversal.
- External taxonomy crosswalks on concepts resolve an exact current source and preserve the same reviewed license expression.

## CLI families

```text
mcl pedagogy propose --payload-json <learning_unit/1> --searchable-text <text> --actor <actor> --idempotency-key <key>
mcl pedagogy version --object-id <uuid> --expected-head <sha256> --payload-json <learning_unit/1> --searchable-text <text> --actor <actor> --idempotency-key <key>
mcl pedagogy get --object-id <uuid> [--version-hash <sha256>]
mcl pedagogy validate --object-id <uuid> --version-hash <sha256>
mcl pedagogy review --object-id <uuid> --expected-head <sha256> --decision reviewed --training-status eligible_public --notes-json <string-array> --actor <reviewer> --idempotency-key <key>
mcl pedagogy link --kind pedagogy.hard_prerequisite --source-object-id <uuid> --source-version-hash <sha256> --target-object-id <uuid> --target-version-hash <sha256> --payload-json <rationale> --actor <actor> --idempotency-key <key>
mcl pedagogy path --root-object-id <uuid> --root-version-hash <sha256> --mode prerequisites [--include-soft]
```

Every mutation supports `--dry-run`. MCP uses the same action names and application service, with closed request fields and structured errors.
