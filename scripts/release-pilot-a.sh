#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 7 ]]; then
  printf 'usage: %s <state-root> <proof-candidate> <proof-ingestion> <release-output> <corpus-output> <rl-output> <comparator-output>\n' "$0" >&2
  exit 64
fi

state_root="$1"
candidate_dir="$2"
ingestion_dir="$3"
release_output="$4"
corpus_output="$5"
rl_output="$6"
comparator_output="$7"
mcl_bin="${PUBLICATION_MCL_BIN:-target/debug/mcl}"
content_file="fixtures/release/pilot-a-repaired-proof-learning-unit.txt"
comparator_challenge="fixtures/comparator/pilot-a/Challenge.lean"

for directory in "$state_root" "$candidate_dir" "$ingestion_dir"; do
  [[ -d "$directory" && ! -L "$directory" ]] || {
    printf 'Pilot A release input is unavailable or unsafe: %s\n' "$directory" >&2
    exit 66
  }
done
[[ -x "$mcl_bin" \
    && -f "$content_file" && ! -L "$content_file" \
    && -f "$comparator_challenge" && ! -L "$comparator_challenge" ]] || {
  printf 'Pilot A release executable or fixture is unavailable\n' >&2
  exit 69
}
[[ ! -e "$release_output" && ! -L "$release_output" ]] || {
  printf 'Pilot A release output already exists or is unsafe\n' >&2
  exit 66
}
[[ ! -e "$corpus_output" && ! -L "$corpus_output" ]] || {
  printf 'Pilot A corpus export output already exists or is unsafe\n' >&2
  exit 66
}
[[ ! -e "$rl_output" && ! -L "$rl_output" ]] || {
  printf 'Pilot A RL export output already exists or is unsafe\n' >&2
  exit 66
}
[[ ! -e "$comparator_output" && ! -L "$comparator_output" ]] || {
  printf 'Pilot A Comparator package output already exists or is unsafe\n' >&2
  exit 66
}

state_root="$(cd "$state_root" && pwd -P)"
candidate_dir="$(cd "$candidate_dir" && pwd -P)"
ingestion_dir="$(cd "$ingestion_dir" && pwd -P)"
release_parent="$(cd "$(dirname "$release_output")" && pwd -P)"
release_output="$release_parent/$(basename "$release_output")"
corpus_parent="$(cd "$(dirname "$corpus_output")" && pwd -P)"
corpus_output="$corpus_parent/$(basename "$corpus_output")"
rl_parent="$(cd "$(dirname "$rl_output")" && pwd -P)"
rl_output="$rl_parent/$(basename "$rl_output")"
comparator_parent="$(cd "$(dirname "$comparator_output")" && pwd -P)"
comparator_output="$comparator_parent/$(basename "$comparator_output")"
evidence_dir="$state_root/release-build-evidence"
[[ ! -e "$evidence_dir" && ! -L "$evidence_dir" ]] || {
  printf 'Pilot A release evidence output already exists or is unsafe\n' >&2
  exit 66
}
mkdir -- "$evidence_dir"
state_content_file="$evidence_dir/pilot-a-repaired-proof-learning-unit.txt"
cp -- "$content_file" "$state_content_file"
content_file="$state_content_file"

receipt_hash="$(jq -er '.receipt.receipt_hash' "$ingestion_dir/publication-ingestion.json")"
source_object_id="$(jq -er '.object_id' "$candidate_dir/closure/source-version.json")"
source_version_hash="$(jq -er '.version_hash' "$candidate_dir/closure/source-version.json")"
claim_object_id="$(jq -er '.payload.claim_version.object_id' "$candidate_dir/closure/formalization-version.json")"
claim_version_hash="$(jq -er '.payload.claim_version.version_hash' "$candidate_dir/closure/formalization-version.json")"
formalization_object_id="$(jq -er '.object_id' "$candidate_dir/closure/formalization-version.json")"
formalization_version_hash="$(jq -er '.version_hash' "$candidate_dir/closure/formalization-version.json")"
license_expression="PolyForm-Noncommercial-1.0.0"

jq -cnS \
  --arg license "$license_expression" \
  '{
    schema_version: "artifact_metadata/1",
    media_type: "text/plain",
    creation_source: "user_ingest",
    license_expression: $license,
    restriction: "private",
    semantic_metadata: {
      artifact_role: "learning_unit_content",
      pilot: "pilot_a_repaired_proof"
    }
  }' >"$evidence_dir/content-metadata.json"

"$mcl_bin" --root "$state_root" --json artifact ingest \
  --input-file "$content_file" \
  --metadata-json "$(<"$evidence_dir/content-metadata.json")" \
  --actor pilot-a-release-author \
  --idempotency-key "pilot-a-release-content:$receipt_hash" \
  >"$evidence_dir/content.json"
content_hash="$(jq -er '.artifact.artifact_hash' "$evidence_dir/content.json")"

declare -A unit_object_ids
declare -A unit_version_hashes

build_unit() {
  local name="$1"
  local kind="$2"
  local objective="$3"
  shift 3
  local prerequisites='[]'
  local prerequisite
  for prerequisite in "$@"; do
    prerequisites="$(
      jq -cnS \
        --argjson existing "$prerequisites" \
        --arg object_id "${unit_object_ids[$prerequisite]}" \
        --arg version_hash "${unit_version_hashes[$prerequisite]}" \
        '$existing + [{object_id: $object_id, version_hash: $version_hash}]'
    )"
  done

  jq -cnS \
    --arg kind "$kind" \
    --arg objective "$objective" \
    --arg source_object_id "$source_object_id" \
    --arg source_version_hash "$source_version_hash" \
    --arg claim_object_id "$claim_object_id" \
    --arg claim_version_hash "$claim_version_hash" \
    --arg formalization_object_id "$formalization_object_id" \
    --arg formalization_version_hash "$formalization_version_hash" \
    --arg content_hash "$content_hash" \
    --arg license "$license_expression" \
    --argjson prerequisites "$prerequisites" '
    {
      unit_kind: $kind,
      target: {
        kind: "claim",
        object_id: $claim_object_id,
        version_hash: $claim_version_hash
      },
      audience_track: "pilot_a_counterexample_repair",
      entry_assumptions: [
        "The learner can distinguish a refuted source claim from a separately identified repair."
      ],
      learning_objectives: [$objective],
      hard_prerequisites: $prerequisites,
      soft_prerequisites: [],
      grounded_source_references: [{
        object_id: $source_object_id,
        version_hash: $source_version_hash
      }],
      content_artifact_hash: $content_hash,
      examples: [],
      nonexamples: [],
      counterexamples: [],
      misconceptions: [],
      exercises: [],
      mastery_checks: [],
      formalization_references: [{
        object_id: $formalization_object_id,
        version_hash: $formalization_version_hash
      }],
      application_references: [],
      frontier_references: [],
      review: {state: "draft", reviewer: null, notes: []},
      license_expression: $license,
      training_status: "ineligible"
    }' >"$evidence_dir/$name-payload.json"

  "$mcl_bin" --root "$state_root" --json pedagogy propose \
    --payload-json "$(<"$evidence_dir/$name-payload.json")" \
    --searchable-text "Pilot A repaired proof $name" \
    --actor pilot-a-release-author \
    --idempotency-key "pilot-a-release-$name-draft:$receipt_hash" \
    >"$evidence_dir/$name-draft.json"
  local draft_object_id
  local draft_version_hash
  draft_object_id="$(jq -er '.record.object_id' "$evidence_dir/$name-draft.json")"
  draft_version_hash="$(jq -er '.record.version_hash' "$evidence_dir/$name-draft.json")"

  "$mcl_bin" --root "$state_root" --json pedagogy review \
    --object-id "$draft_object_id" \
    --expected-head "$draft_version_hash" \
    --decision reviewed \
    --training-status ineligible \
    --notes-json '["Checked against the exact protected source, repaired claim, formalization, and publication receipt; the restricted source keeps this unit training-ineligible."]' \
    --actor pilot-a-release-reviewer \
    --idempotency-key "pilot-a-release-$name-review:$receipt_hash" \
    >"$evidence_dir/$name-review.json"
  unit_object_ids[$name]="$(jq -er '.record.object_id' "$evidence_dir/$name-review.json")"
  unit_version_hashes[$name]="$(jq -er '.record.version_hash' "$evidence_dir/$name-review.json")"

  for prerequisite in "$@"; do
    "$mcl_bin" --root "$state_root" --json pedagogy link \
      --kind pedagogy.hard_prerequisite \
      --source-object-id "${unit_object_ids[$name]}" \
      --source-version-hash "${unit_version_hashes[$name]}" \
      --target-object-id "${unit_object_ids[$prerequisite]}" \
      --target-version-hash "${unit_version_hashes[$prerequisite]}" \
      --payload-json "{\"rationale\":\"$name requires $prerequisite.\"}" \
      --actor pilot-a-release-author \
      --idempotency-key "pilot-a-release-$name-$prerequisite-link:$receipt_hash" \
      >"$evidence_dir/$name-$prerequisite-link.json"
  done

  "$mcl_bin" --root "$state_root" --json pedagogy validate \
    --object-id "${unit_object_ids[$name]}" \
    --version-hash "${unit_version_hashes[$name]}" \
    >"$evidence_dir/$name-validation.json"
  jq -e '
    .valid == true and
    .review_state == "reviewed" and
    .training_status == "ineligible"
  ' "$evidence_dir/$name-validation.json" >/dev/null
}

build_unit \
  explanation \
  explanation \
  'Explain why the repaired theorem has a separate identity from the refuted source statement.'
build_unit \
  counterexample \
  counterexample \
  'Interpret 2 as the retained counterexample to the unqualified source statement.' \
  explanation
build_unit \
  misconception \
  misconception \
  'Reject the misconception that a repair proof erases the original refutation.' \
  explanation
build_unit \
  exercise \
  exercise \
  'Classify the original and repaired claims without transferring authority between them.' \
  counterexample misconception
build_unit \
  mastery_check \
  mastery_check \
  'Verify that source, claim, formalization, lesson, and receipt identities stay exact.' \
  exercise

root_object_id="${unit_object_ids[mastery_check]}"
root_version_hash="${unit_version_hashes[mastery_check]}"
"$mcl_bin" --root "$state_root" --json pedagogy path \
  --root-object-id "$root_object_id" \
  --root-version-hash "$root_version_hash" \
  --mode prerequisites \
  --max-depth 8 \
  --limit 20 \
  >"$evidence_dir/pedagogy-path.json"
jq -e '
  (.units | length) == 5 and
  ([.units[].unit.payload.review.state] | all(. == "reviewed")) and
  ([.units[].unit.payload.training_status] | all(. == "ineligible"))
' "$evidence_dir/pedagogy-path.json" >/dev/null

"$mcl_bin" --root "$state_root" --json release build \
  --publication-receipt-hash "$receipt_hash" \
  --pedagogy-root-object-id "$root_object_id" \
  --pedagogy-root-version-hash "$root_version_hash" \
  --mode prerequisites \
  --max-depth 8 \
  --limit 20 \
  --profile private \
  --output-dir "$release_output" \
  --dry-run \
  >"$evidence_dir/release-dry-run.json"
preview_manifest_hash="$(jq -er 'select(.dry_run == true) | .manifest_hash' "$evidence_dir/release-dry-run.json")"
[[ ! -e "$release_output" && ! -L "$release_output" ]] || {
  printf 'release dry-run wrote its output directory\n' >&2
  exit 71
}

"$mcl_bin" --root "$state_root" --json release build \
  --publication-receipt-hash "$receipt_hash" \
  --pedagogy-root-object-id "$root_object_id" \
  --pedagogy-root-version-hash "$root_version_hash" \
  --mode prerequisites \
  --max-depth 8 \
  --limit 20 \
  --profile private \
  --output-dir "$release_output" \
  >"$evidence_dir/release-build.json"

manifest_hash="$(sha256sum "$release_output/manifest.json" | cut -d ' ' -f 1)"
[[ "$preview_manifest_hash" == "$manifest_hash" ]] || {
  printf 'release dry-run and build manifest identities differ\n' >&2
  exit 71
}
jq -e \
  --arg manifest_hash "$manifest_hash" '
  .dry_run == false and
  .manifest_hash == $manifest_hash and
  .profile == "private" and
  .member_count > 0 and
  .total_member_bytes > 0
' "$evidence_dir/release-build.json" >/dev/null
jq -e '
  .schema_version == "release_manifest/1" and
  .profile == "private" and
  (.publication.authority_evidence_id | type) == "string" and
  (.publication.fidelity_evidence_id | type) == "string" and
  ([.members[].kind] | unique | sort) ==
    (["artifact", "edge", "environment", "evidence", "export", "license", "object", "replay", "report"] | sort)
' "$release_output/manifest.json" >/dev/null
jq -s -e '
  any(.[]; .kind == "research.repairs") and
  any(.[]; .kind == "pedagogy.hard_prerequisite")
' "$release_output"/edges/*.json >/dev/null
jq -s -e '
  any(.[]; .payload.authority_class == "authoritative") and
  any(.[]; .payload.evidence_kind == "statement_fidelity_review")
' "$release_output"/evidence/*.json >/dev/null
[[ -z "$(find "$release_output" -type l -print -quit)" ]] || {
  printf 'portable release contains a symbolic link\n' >&2
  exit 71
}

release_copy="$release_parent/$(basename "$release_output")-clean-copy"
[[ ! -e "$release_copy" && ! -L "$release_copy" ]] || {
  printf 'clean release copy destination already exists\n' >&2
  exit 66
}
cp -a -- "$release_output" "$release_copy"

database="$state_root/.mcl/state.sqlite3"
hidden_database="$state_root/.mcl/state.sqlite3.release-hidden"
[[ -f "$database" && ! -e "$hidden_database" ]] || {
  printf 'canonical database cannot be hidden safely for release replay\n' >&2
  exit 71
}
mv -- "$database" "$hidden_database"
restore_database() {
  if [[ -f "$hidden_database" && ! -e "$database" ]]; then
    mv -- "$hidden_database" "$database"
  fi
}
trap restore_database EXIT

"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release verify \
  --bundle-dir "$release_copy" \
  --expected-manifest-hash "$manifest_hash" \
  >"$evidence_dir/release-verification.json"
jq -e \
  --arg manifest_hash "$manifest_hash" '
  .manifest_hash == $manifest_hash and
  .profile == "private" and
  .database_independent == true and
  .inventory_verified == true and
  .hashes_verified == true and
  .schemas_verified == true and
  .references_verified == true and
  .replay_succeeded == true and
  (.observed_lean_toolchain | contains("version 4.32.0"))
' "$evidence_dir/release-verification.json" >/dev/null

"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release export \
  --bundle-dir "$release_copy" \
  --expected-manifest-hash "$manifest_hash" \
  --packet-id mathos.number_theory.pilot_a_repair.v1 \
  --domain number_theory \
  --level L1_proof_basics \
  --difficulty-bin D1 \
  --output-dir "$corpus_output" \
  --dry-run \
  >"$evidence_dir/corpus-export-dry-run.json"
corpus_preview_hash="$(
  jq -er 'select(.dry_run == true and .policy == "private_audit_only") | .manifest_hash' \
    "$evidence_dir/corpus-export-dry-run.json"
)"
[[ ! -e "$corpus_output" && ! -L "$corpus_output" ]] || {
  printf 'corpus export dry-run wrote its output directory\n' >&2
  exit 71
}

"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release export \
  --bundle-dir "$release_copy" \
  --expected-manifest-hash "$manifest_hash" \
  --packet-id mathos.number_theory.pilot_a_repair.v1 \
  --domain number_theory \
  --level L1_proof_basics \
  --difficulty-bin D1 \
  --output-dir "$corpus_output" \
  >"$evidence_dir/corpus-export-build.json"
corpus_manifest_hash="$(sha256sum "$corpus_output/manifest.json" | cut -d ' ' -f 1)"
[[ "$corpus_preview_hash" == "$corpus_manifest_hash" ]] || {
  printf 'corpus export dry-run and build manifest identities differ\n' >&2
  exit 71
}
jq -e \
  --arg corpus_manifest_hash "$corpus_manifest_hash" \
  --arg source_manifest_hash "$manifest_hash" '
  .dry_run == false and
  .manifest_hash == $corpus_manifest_hash and
  .source_release_manifest_hash == $source_manifest_hash and
  .policy == "private_audit_only" and
  .member_count == 11 and
  .total_member_bytes > 0
' "$evidence_dir/corpus-export-build.json" >/dev/null
jq -e '
  .schema_version == "corpus_export_manifest/1" and
  .source_release.release_profile == "private" and
  .curation.policy == "private_audit_only" and
  .curation.packet_id == "mathos.number_theory.pilot_a_repair.v1" and
  .upstream.repository == "Mnehmos/mathcorpus" and
  (.members | length) == 11
' "$corpus_output/manifest.json" >/dev/null
jq -e '
  .training.eligibility == "private_audit_only" and
  .training.split == "private_audit_only" and
  .training.can_export_proof_body == false and
  .trust.public_claim_class == "private_only" and
  .hashes.private_artifact_bundle_sha256 != null
' "$corpus_output/mathcorpus/packet.json" >/dev/null
jq -e '
  .mcip_version == "1.0.0" and
  (.records | length) == 3 and
  ([.records[].record_type] | sort) ==
    (["dependency_manifest", "packet_identity", "proof_variant"] | sort) and
  ([.records[].export_eligibility] | all(. == "private_only"))
' "$corpus_output/mcip/bundle.json" >/dev/null
[[ -z "$(find "$corpus_output" -type l -print -quit)" ]] || {
  printf 'corpus export contains a symbolic link\n' >&2
  exit 71
}

corpus_copy="$corpus_parent/$(basename "$corpus_output")-clean-copy"
[[ ! -e "$corpus_copy" && ! -L "$corpus_copy" ]] || {
  printf 'clean corpus export copy destination already exists\n' >&2
  exit 66
}
cp -a -- "$corpus_output" "$corpus_copy"
"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release verify-export \
  --export-dir "$corpus_copy" \
  --expected-manifest-hash "$corpus_manifest_hash" \
  --source-bundle-dir "$release_copy" \
  >"$evidence_dir/corpus-export-verification.json"
jq -e \
  --arg corpus_manifest_hash "$corpus_manifest_hash" \
  --arg source_manifest_hash "$manifest_hash" '
  .manifest_hash == $corpus_manifest_hash and
  .source_release_manifest_hash == $source_manifest_hash and
  .policy == "private_audit_only" and
  .database_independent == true and
  .inventory_verified == true and
  .hashes_verified == true and
  .schemas_verified == true and
  .bindings_verified == true and
  .deterministic_reprojection_verified == true
' "$evidence_dir/corpus-export-verification.json" >/dev/null

release_id="$(basename "$release_output")"
published_timestamp="$(jq -er '.created_at' "$release_output/reports/publication-receipt.json")"
published_on="$(date --utc --date="@$published_timestamp" +%F)"
publication_cutoff="$(date --utc --date="$published_on - 1 day" +%F)"
jq -cnS \
  --arg release_id "$release_id" \
  --arg manifest_hash "$manifest_hash" \
  --arg published_on "$published_on" \
  --arg publication_cutoff "$publication_cutoff" '
  {
    schema_version: "rl_export_plan/1",
    publication_cutoff: $publication_cutoff,
    releases: [{
      release_id: $release_id,
      expected_manifest_hash: $manifest_hash,
      split: "held_out_evaluation",
      published_on: $published_on,
      benchmark_identity: "mathos-pilot-a-prime-parity",
      leakage_labels: {
        theorem_dependency_components: ["pilot-a-prime-parity"],
        equivalent_formalizations: ["pilot-a-prime-parity-refutation-repair"],
        shared_sources: ["pilot-a-protected-source"],
        certificate_families: ["pilot-a-refutation-repair-certificate"],
        proof_variants: ["pilot-a-refutation-repair-module"]
      }
    }]
  }' >"$evidence_dir/rl-export-plan.json"

"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release export-rl \
  --plan "$evidence_dir/rl-export-plan.json" \
  --source-root "$release_parent" \
  --output-dir "$rl_output" \
  --dry-run \
  >"$evidence_dir/rl-export-dry-run.json"
rl_preview_hash="$(
  jq -er '
    select(
      .dry_run == true and
      .source_release_count == 1 and
      .component_count == 1 and
      .task_count == 10
    ) | .manifest_hash
  ' "$evidence_dir/rl-export-dry-run.json"
)"
[[ ! -e "$rl_output" && ! -L "$rl_output" ]] || {
  printf 'RL export dry-run wrote its output directory\n' >&2
  exit 71
}

"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release export-rl \
  --plan "$evidence_dir/rl-export-plan.json" \
  --source-root "$release_parent" \
  --output-dir "$rl_output" \
  >"$evidence_dir/rl-export-build.json"
rl_manifest_hash="$(sha256sum "$rl_output/manifest.json" | cut -d ' ' -f 1)"
[[ "$rl_preview_hash" == "$rl_manifest_hash" ]] || {
  printf 'RL export dry-run and build manifest identities differ\n' >&2
  exit 71
}
jq -e \
  --arg manifest_hash "$rl_manifest_hash" \
  --arg source_manifest_hash "$manifest_hash" '
  .dry_run == false and
  .manifest_hash == $manifest_hash and
  .source_release_count == 1 and
  .component_count == 1 and
  .task_count == 10 and
  .member_count > 10 and
  .total_member_bytes > 0
' "$evidence_dir/rl-export-build.json" >/dev/null
jq -e \
  --arg source_manifest_hash "$manifest_hash" '
  .schema_version == "rl_export_manifest/1" and
  .task_count == 10 and
  .component_count == 1 and
  (.source_releases | length) == 1 and
  .source_releases[0].release_manifest_hash == $source_manifest_hash and
  .source_releases[0].release_profile == "private" and
  .source_releases[0].split == "held_out_evaluation" and
  ([.members[] | select(.kind == "task") | .restriction] | all(. == "private"))
' "$rl_output/manifest.json" >/dev/null
jq -e '
  .cross_split_overlap_count == 0 and
  .temporal_policy_verified == true and
  (.components | length) == 1 and
  .components[0].split == "held_out_evaluation" and
  (.components[0].task_ids | length) == 10 and
  ([.task_families[] | select(.emitted_task_count > 0) | .family] | sort) ==
    (["counterexample", "declaration_retrieval", "fidelity_selection", "formalization", "proof_generation", "statement_repair"] | sort) and
  ([.task_families[] | select(.emitted_task_count == 0) | .skip_reason] | all(type == "string"))
' "$rl_output/leakage/report.json" >/dev/null
[[ -z "$(find "$rl_output" -type l -print -quit)" ]] || {
  printf 'RL export contains a symbolic link\n' >&2
  exit 71
}

rl_copy="$rl_parent/$(basename "$rl_output")-clean-copy"
[[ ! -e "$rl_copy" && ! -L "$rl_copy" ]] || {
  printf 'clean RL export copy destination already exists\n' >&2
  exit 66
}
cp -a -- "$rl_output" "$rl_copy"
"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release verify-rl-export \
  --export-dir "$rl_copy" \
  --expected-manifest-hash "$rl_manifest_hash" \
  --plan "$evidence_dir/rl-export-plan.json" \
  --source-root "$release_parent" \
  >"$evidence_dir/rl-export-verification.json"
jq -e \
  --arg manifest_hash "$rl_manifest_hash" '
  .manifest_hash == $manifest_hash and
  .source_release_count == 1 and
  .component_count == 1 and
  .task_count == 10 and
  .database_independent == true and
  .inventory_verified == true and
  .hashes_verified == true and
  .schemas_verified == true and
  .split_isolation_verified == true and
  .temporal_policy_verified == true and
  .source_releases_verified == true and
  .deterministic_reprojection_verified == true
' "$evidence_dir/rl-export-verification.json" >/dev/null

comparator_declaration="$(jq -er '.publication.declaration_name' "$release_copy/manifest.json")"
jq -cnS \
  --rawfile challenge "$comparator_challenge" \
  --arg manifest_hash "$manifest_hash" \
  --arg formalization_object_id "$formalization_object_id" \
  --arg formalization_version_hash "$formalization_version_hash" \
  --arg declaration "$comparator_declaration" \
  --arg comparator_repository "https://github.com/leanprover/comparator" \
  --arg comparator_commit "68a064109f01c08f47c8edc9f51d6a2bbffaa188" \
  --arg lean4export_repository "https://github.com/leanprover/lean4export" \
  --arg lean4export_commit "af5aa64bb914c3c2c781f378088dbd38acf4f804" \
  --arg landrun_repository "https://github.com/Zouuup/landrun" \
  --arg landrun_commit "5ed4a3db3a4ad930d577215c6b9abaa19df7f99f" '
  {
    schema_version: "comparator_package_plan/1",
    source_release_manifest_hash: $manifest_hash,
    formalization: {
      object_id: $formalization_object_id,
      version_hash: $formalization_version_hash
    },
    challenge_source: $challenge,
    theorem_names: [$declaration],
    permitted_axioms: [],
    enable_nanoda: false,
    tool_pins: {
      comparator_repository: $comparator_repository,
      comparator_commit: $comparator_commit,
      lean4export_repository: $lean4export_repository,
      lean4export_commit: $lean4export_commit,
      landrun_repository: $landrun_repository,
      landrun_commit: $landrun_commit
    },
    formalization_metadata: {
      mathematical_source: "Pilot A repaired claim: every prime natural number other than 2 is odd.",
      theorem_scope: "Natural-number primality is defined in the frozen module; the theorem excludes the unique even prime 2.",
      ai_involvement: "AI-assisted formalization, proof repair, packaging, and review under protected MathOS controls.",
      human_operators: ["Mnehmos"],
      upstream_repositories: ["https://github.com/Mnehmos/MathOS"],
      publication_status: "internal"
    }
  }' >"$evidence_dir/comparator-package-plan.json"
truncate --size=-1 "$evidence_dir/comparator-package-plan.json"

"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release export-comparator \
  --plan "$evidence_dir/comparator-package-plan.json" \
  --bundle-dir "$release_copy" \
  --expected-release-manifest-hash "$manifest_hash" \
  --output-dir "$comparator_output" \
  --dry-run \
  >"$evidence_dir/comparator-export-dry-run.json"
comparator_preview_hash="$(
  jq -er '
    select(
      .dry_run == true and
      .status == "ready" and
      .comparator_verified == false and
      .authoritative == false and
      .member_count == 5
    ) | .verification_hash
  ' "$evidence_dir/comparator-export-dry-run.json"
)"
[[ ! -e "$comparator_output" && ! -L "$comparator_output" ]] || {
  printf 'Comparator package dry-run wrote its output directory\n' >&2
  exit 71
}

"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release export-comparator \
  --plan "$evidence_dir/comparator-package-plan.json" \
  --bundle-dir "$release_copy" \
  --expected-release-manifest-hash "$manifest_hash" \
  --output-dir "$comparator_output" \
  >"$evidence_dir/comparator-export-build.json"
comparator_verification_hash="$(sha256sum "$comparator_output/verification.json" | cut -d ' ' -f 1)"
[[ "$comparator_preview_hash" == "$comparator_verification_hash" ]] || {
  printf 'Comparator package dry-run and build identities differ\n' >&2
  exit 71
}
jq -e \
  --arg verification_hash "$comparator_verification_hash" \
  --arg manifest_hash "$manifest_hash" '
  .dry_run == false and
  .verification_hash == $verification_hash and
  .source_release_manifest_hash == $manifest_hash and
  .status == "ready" and
  .comparator_verified == false and
  .authoritative == false and
  .member_count == 5 and
  .total_member_bytes > 0
' "$evidence_dir/comparator-export-build.json" >/dev/null
jq -e \
  --arg manifest_hash "$manifest_hash" \
  --arg declaration "$comparator_declaration" '
  .schema_version == "comparator_package_verification/1" and
  .source_release_manifest_hash == $manifest_hash and
  .declaration_name == $declaration and
  .status == "ready" and
  .comparator_verified == false and
  .authoritative == false and
  .tool_pins.comparator_repository == "https://github.com/leanprover/comparator" and
  .tool_pins.comparator_commit == "68a064109f01c08f47c8edc9f51d6a2bbffaa188" and
  .tool_pins.lean4export_repository == "https://github.com/leanprover/lean4export" and
  .tool_pins.lean4export_commit == "af5aa64bb914c3c2c781f378088dbd38acf4f804" and
  .tool_pins.landrun_repository == "https://github.com/Zouuup/landrun" and
  .tool_pins.landrun_commit == "5ed4a3db3a4ad930d577215c6b9abaa19df7f99f"
' "$comparator_output/verification.json" >/dev/null
cmp --silent "$comparator_output/Solution.lean" "$release_copy/replay/Submission.lean" || {
  printf 'Comparator Solution.lean differs from the frozen theorem source\n' >&2
  exit 71
}
[[ "$(find "$comparator_output" -mindepth 1 -maxdepth 1 -type f | wc -l)" -eq 5 \
    && -z "$(find "$comparator_output" -mindepth 1 ! -type f -print -quit)" ]] || {
  printf 'Comparator package does not contain exactly five regular files\n' >&2
  exit 71
}
lean "$comparator_output/Challenge.lean" \
  >"$evidence_dir/comparator-challenge-lean.stdout" \
  2>"$evidence_dir/comparator-challenge-lean.stderr"
lean "$comparator_output/Solution.lean" \
  >"$evidence_dir/comparator-solution-lean.stdout" \
  2>"$evidence_dir/comparator-solution-lean.stderr"

comparator_copy="$comparator_parent/$(basename "$comparator_output")-clean-copy"
[[ ! -e "$comparator_copy" && ! -L "$comparator_copy" ]] || {
  printf 'clean Comparator package copy destination already exists\n' >&2
  exit 66
}
cp -a -- "$comparator_output" "$comparator_copy"
"$mcl_bin" --root "$state_root/nonexistent-offline-root" --json release verify-comparator-package \
  --package-dir "$comparator_copy" \
  --expected-verification-hash "$comparator_verification_hash" \
  --plan "$evidence_dir/comparator-package-plan.json" \
  --bundle-dir "$release_copy" \
  --expected-release-manifest-hash "$manifest_hash" \
  >"$evidence_dir/comparator-package-verification.json"
jq -e \
  --arg verification_hash "$comparator_verification_hash" \
  --arg manifest_hash "$manifest_hash" '
  .verification_hash == $verification_hash and
  .source_release_manifest_hash == $manifest_hash and
  .status == "ready" and
  .comparator_verified == false and
  .authoritative == false and
  .member_count == 5 and
  .database_independent == true and
  .inventory_verified == true and
  .hashes_verified == true and
  .bindings_verified == true and
  .deterministic_reprojection == true
' "$evidence_dir/comparator-package-verification.json" >/dev/null
restore_database
trap - EXIT

jq -cnS \
  --arg manifest_hash "$manifest_hash" \
  --arg receipt_hash "$receipt_hash" \
  --arg root_object_id "$root_object_id" \
  --arg root_version_hash "$root_version_hash" \
  --arg release_output "$release_output" \
  --arg corpus_manifest_hash "$corpus_manifest_hash" \
  --arg corpus_output "$corpus_output" \
  --arg rl_manifest_hash "$rl_manifest_hash" \
  --arg rl_output "$rl_output" \
  --arg comparator_verification_hash "$comparator_verification_hash" \
  --arg comparator_output "$comparator_output" \
  '{
    schema_version: "pilot_a_release_playtest/4",
    manifest_hash: $manifest_hash,
    corpus_export_manifest_hash: $corpus_manifest_hash,
    rl_export_manifest_hash: $rl_manifest_hash,
    comparator_package_verification_hash: $comparator_verification_hash,
    publication_receipt_hash: $receipt_hash,
    pedagogy_root: {object_id: $root_object_id, version_hash: $root_version_hash},
    release_output: $release_output,
    corpus_export_output: $corpus_output,
    rl_export_output: $rl_output,
    comparator_package_output: $comparator_output,
    database_hidden_during_verification: true,
    clean_copy_verified: true,
    lean_replay_succeeded: true,
    corpus_export_reprojection_succeeded: true,
    rl_export_reprojection_succeeded: true,
    comparator_package_reprojection_succeeded: true,
    comparator_status: "ready",
    comparator_verified: false,
    authoritative: false
  }' >"$evidence_dir/playtest-summary.json"
cat "$evidence_dir/playtest-summary.json"
