#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  printf 'usage: %s <state-root> <proof-candidate> <proof-ingestion> <release-output>\n' "$0" >&2
  exit 64
fi

state_root="$1"
candidate_dir="$2"
ingestion_dir="$3"
release_output="$4"
mcl_bin="${PUBLICATION_MCL_BIN:-target/debug/mcl}"
content_file="fixtures/release/pilot-a-repaired-proof-learning-unit.txt"

for directory in "$state_root" "$candidate_dir" "$ingestion_dir"; do
  [[ -d "$directory" && ! -L "$directory" ]] || {
    printf 'Pilot A release input is unavailable or unsafe: %s\n' "$directory" >&2
    exit 66
  }
done
[[ -x "$mcl_bin" && -f "$content_file" && ! -L "$content_file" ]] || {
  printf 'Pilot A release executable or content fixture is unavailable\n' >&2
  exit 69
}
[[ ! -e "$release_output" && ! -L "$release_output" ]] || {
  printf 'Pilot A release output already exists or is unsafe\n' >&2
  exit 66
}

state_root="$(cd "$state_root" && pwd -P)"
candidate_dir="$(cd "$candidate_dir" && pwd -P)"
ingestion_dir="$(cd "$ingestion_dir" && pwd -P)"
release_parent="$(cd "$(dirname "$release_output")" && pwd -P)"
release_output="$release_parent/$(basename "$release_output")"
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
    --training-status eligible_private \
    --notes-json '["Checked against the exact protected source, repaired claim, formalization, and publication receipt."]' \
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
    .training_status == "eligible_private"
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
  ([.units[].unit.payload.training_status] | all(. == "eligible_private"))
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
restore_database
trap - EXIT

jq -cnS \
  --arg manifest_hash "$manifest_hash" \
  --arg receipt_hash "$receipt_hash" \
  --arg root_object_id "$root_object_id" \
  --arg root_version_hash "$root_version_hash" \
  --arg release_output "$release_output" \
  '{
    schema_version: "pilot_a_release_playtest/1",
    manifest_hash: $manifest_hash,
    publication_receipt_hash: $receipt_hash,
    pedagogy_root: {object_id: $root_object_id, version_hash: $root_version_hash},
    release_output: $release_output,
    database_hidden_during_verification: true,
    clean_copy_verified: true,
    lean_replay_succeeded: true
  }' >"$evidence_dir/playtest-summary.json"
cat "$evidence_dir/playtest-summary.json"
