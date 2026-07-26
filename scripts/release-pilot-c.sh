#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 5 ]]; then
  printf 'usage: %s <state-root> <candidate-directory> <ingestion-directory> <release-output> <corpus-output>\n' "$0" >&2
  exit 64
fi

state_root="$1"
candidate_dir="$2"
ingestion_dir="$3"
release_output="$4"
corpus_output="$5"
mcl_bin="${PUBLICATION_MCL_BIN:-target/debug/mcl}"
content_fixture="fixtures/release/pilot-c-frontier-note.txt"

for directory in "$state_root" "$candidate_dir" "$ingestion_dir"; do
  [[ -d "$directory" && ! -L "$directory" ]] || {
    printf 'Pilot C release input is unavailable or unsafe: %s\n' "$directory" >&2
    exit 66
  }
done
[[ -x "$mcl_bin" && -f "$content_fixture" && ! -L "$content_fixture" ]] || {
  printf 'Pilot C release executable or content fixture is unavailable\n' >&2
  exit 69
}
for control in /usr/bin/sudo /usr/bin/chown /usr/bin/bwrap /usr/bin/prlimit; do
  [[ -x "$control" ]] || {
    printf 'Pilot C release isolation control is unavailable: %s\n' "$control" >&2
    exit 69
  }
done
sudo -n true >/dev/null 2>&1 || {
  printf 'Pilot C release requires non-interactive sudo for publication replay\n' >&2
  exit 69
}
for output in "$release_output" "$corpus_output"; do
  [[ ! -e "$output" && ! -L "$output" ]] || {
    printf 'Pilot C release output already exists or is unsafe: %s\n' "$output" >&2
    exit 66
  }
done

state_root="$(cd "$state_root" && pwd -P)"
candidate_dir="$(cd "$candidate_dir" && pwd -P)"
ingestion_dir="$(cd "$ingestion_dir" && pwd -P)"
release_parent="$(cd "$(dirname "$release_output")" && pwd -P)"
release_output="$release_parent/$(basename "$release_output")"
corpus_parent="$(cd "$(dirname "$corpus_output")" && pwd -P)"
corpus_output="$corpus_parent/$(basename "$corpus_output")"
evidence_dir="$state_root/pilot-c-release-evidence"
[[ ! -e "$evidence_dir" && ! -L "$evidence_dir" ]] || {
  printf 'Pilot C release evidence output already exists or is unsafe\n' >&2
  exit 66
}
mkdir -- "$evidence_dir"
content_file="$evidence_dir/pilot-c-frontier-note.txt"
cp -- "$content_fixture" "$content_file"

receipt_hash="$(jq -er '.receipt.receipt_hash' "$ingestion_dir/publication-ingestion.json")"
claim_object_id="$(jq -er '.object_id' "$candidate_dir/closure/claim-version.json")"
claim_version_hash="$(jq -er '.version_hash' "$candidate_dir/closure/claim-version.json")"
formalization_object_id="$(jq -er '.object_id' "$candidate_dir/closure/formalization-version.json")"
formalization_version_hash="$(jq -er '.version_hash' "$candidate_dir/closure/formalization-version.json")"
project_hash="$(jq -er '.payload.project.archive_artifact_hash' "$candidate_dir/closure/formalization-version.json")"
grounded_sources="$(jq -c '.payload.source_citations' "$candidate_dir/closure/claim-version.json")"
license_expression="CC-BY-4.0"

jq -cnS --arg license "$license_expression" '{
  schema_version: "artifact_metadata/1",
  media_type: "text/plain",
  creation_source: "user_ingest",
  license_expression: $license,
  restriction: "private",
  semantic_metadata: {
    artifact_role: "learning_unit_content",
    pilot: "pilot_c",
    unit_kind: "frontier_note"
  }
}' >"$evidence_dir/content-metadata.json"

"$mcl_bin" --root "$state_root" --json artifact ingest \
  --input-file "$content_file" \
  --metadata-json "$(<"$evidence_dir/content-metadata.json")" \
  --actor pilot-c-release-author \
  --idempotency-key "pilot-c-release-content:$receipt_hash" \
  >"$evidence_dir/content.json"
content_hash="$(jq -er '.artifact.artifact_hash' "$evidence_dir/content.json")"

jq -cnS \
  --arg claim_object_id "$claim_object_id" \
  --arg claim_version_hash "$claim_version_hash" \
  --arg formalization_object_id "$formalization_object_id" \
  --arg formalization_version_hash "$formalization_version_hash" \
  --arg content_hash "$content_hash" \
  --arg license "$license_expression" \
  --argjson grounded_sources "$grounded_sources" '{
  unit_kind: "frontier_note",
  target: {
    kind: "claim",
    object_id: $claim_object_id,
    version_hash: $claim_version_hash
  },
  audience_track: "pilot_c_research_formalization",
  entry_assumptions: [
    "The reader knows the definition of false discovery rate and the Benjamini-Hochberg step-up procedure.",
    "The reader can distinguish source-statement fidelity from kernel verification."
  ],
  learning_objectives: [
    "State the exact asymptotic FDR counterexample and explain the Lean N+1 indexing shift.",
    "Distinguish reusable statistical lemmas from the paper-specific model and generated certificate."
  ],
  hard_prerequisites: [],
  soft_prerequisites: [],
  grounded_source_references: $grounded_sources,
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
}' >"$evidence_dir/frontier-note-payload.json"

"$mcl_bin" --root "$state_root" --json pedagogy propose \
  --payload-json "$(<"$evidence_dir/frontier-note-payload.json")" \
  --searchable-text "Pilot C Gaussian BH FDR formalization frontier note" \
  --actor pilot-c-release-author \
  --idempotency-key "pilot-c-release-frontier-note-draft:$receipt_hash" \
  >"$evidence_dir/frontier-note-draft.json"
root_object_id="$(jq -er '.record.object_id' "$evidence_dir/frontier-note-draft.json")"
draft_version_hash="$(jq -er '.record.version_hash' "$evidence_dir/frontier-note-draft.json")"

"$mcl_bin" --root "$state_root" --json pedagogy review \
  --object-id "$root_object_id" \
  --expected-head "$draft_version_hash" \
  --decision reviewed \
  --training-status ineligible \
  --notes-json '["Checked against the exact protected paper, repositories, project closure, fidelity review, trust axes, and publication receipt; restricted source provenance keeps this unit training-ineligible."]' \
  --actor pilot-c-release-reviewer \
  --idempotency-key "pilot-c-release-frontier-note-review:$receipt_hash" \
  >"$evidence_dir/frontier-note-review.json"
root_version_hash="$(jq -er '.record.version_hash' "$evidence_dir/frontier-note-review.json")"

"$mcl_bin" --root "$state_root" --json pedagogy validate \
  --object-id "$root_object_id" \
  --version-hash "$root_version_hash" \
  >"$evidence_dir/frontier-note-validation.json"
jq -e '
  .valid == true and
  .review_state == "reviewed" and
  .training_status == "ineligible"
' "$evidence_dir/frontier-note-validation.json" >/dev/null || {
  printf 'Pilot C frontier note failed reviewed validation\n' >&2
  exit 71
}

"$mcl_bin" --root "$state_root" --json pedagogy path \
  --root-object-id "$root_object_id" \
  --root-version-hash "$root_version_hash" \
  --mode prerequisites \
  --max-depth 8 \
  --limit 20 \
  >"$evidence_dir/pedagogy-path.json"
jq -e '
  (.units | length) == 1 and
  .units[0].unit.payload.unit_kind == "frontier_note" and
  .units[0].unit.payload.review.state == "reviewed" and
  .units[0].unit.payload.training_status == "ineligible" and
  (.units[0].unit.payload.grounded_source_references | length) == 3
' "$evidence_dir/pedagogy-path.json" >/dev/null || {
  printf 'Pilot C pedagogy path lost its exact reviewed root\n' >&2
  exit 71
}

build_release() {
  "$mcl_bin" --root "$state_root" --json release build \
    --publication-receipt-hash "$receipt_hash" \
    --pedagogy-root-object-id "$root_object_id" \
    --pedagogy-root-version-hash "$root_version_hash" \
    --mode prerequisites \
    --max-depth 8 \
    --limit 20 \
    --profile private \
    --output-dir "$release_output" \
    "$@"
}
build_release --dry-run >"$evidence_dir/release-dry-run.json"
preview_manifest_hash="$(jq -er 'select(.dry_run == true) | .manifest_hash' "$evidence_dir/release-dry-run.json")"
[[ ! -e "$release_output" && ! -L "$release_output" ]] || {
  printf 'Pilot C release dry-run wrote its output directory\n' >&2
  exit 71
}
build_release >"$evidence_dir/release-build.json"
manifest_hash="$(sha256sum "$release_output/manifest.json" | cut -d ' ' -f 1)"
[[ "$preview_manifest_hash" == "$manifest_hash" ]] || {
  printf 'Pilot C release dry-run and build identities differ\n' >&2
  exit 71
}
jq -e \
  --arg manifest_hash "$manifest_hash" \
  --arg project_hash "$project_hash" '
  .schema_version == "release_manifest/2" and
  .profile == "private" and
  .trust.profile == "experimental" and
  .trust.kernel_status == "kernel_verified" and
  .trust.fidelity_status == "reviewer_checked" and
  .trust.definition_status == "source_grounded" and
  .trust.reuse_status == "campaign_specific" and
  .trust.coverage_status == "full_theorem" and
  .trust.eligible == true and
  .replay.project_archive_path == "replay/project.tar" and
  .publication.project.archive_artifact_hash == $project_hash and
  any(.members[];
    .path == "replay/project.tar" and
    .content_hash == $project_hash and
    .kind == "replay"
  ) and
  (.publication.authority_evidence_id | type) == "string" and
  (.publication.fidelity_evidence_id | type) == "string"
' "$release_output/manifest.json" >/dev/null || {
  printf 'Pilot C release manifest lost its project or trust binding\n' >&2
  exit 71
}
[[ "$(sha256sum "$release_output/replay/project.tar" | cut -d ' ' -f 1)" == "$project_hash" \
    && -z "$(find "$release_output" -type l -print -quit)" ]] || {
  printf 'Pilot C portable release project bytes or inventory are unsafe\n' >&2
  exit 71
}

release_copy="$release_parent/$(basename "$release_output")-clean-copy"
[[ ! -e "$release_copy" && ! -L "$release_copy" ]] || {
  printf 'Pilot C clean release copy already exists\n' >&2
  exit 66
}
cp -a -- "$release_output" "$release_copy"
[[ -z "$(find "$release_copy" -type f -iname '*.sqlite*' -print -quit)" ]] || {
  printf 'Pilot C portable release retained a SQLite file\n' >&2
  exit 71
}

database="$state_root/.mcl/state.sqlite3"
[[ -f "$database" && ! -L "$database" ]] || {
  printf 'Pilot C canonical database cannot be hidden safely\n' >&2
  exit 71
}
database_backup_root="$(mktemp -d "${RUNNER_TEMP:-/tmp}/mathos-pilot-c-database.XXXXXX")"
hidden_database="$database_backup_root/state.sqlite3"
offline_root="$database_backup_root/nonexistent-offline-root"
restore_database() {
  if [[ -f "$hidden_database" && ! -e "$database" ]]; then
    mv -- "$hidden_database" "$database"
  fi
  rmdir -- "$database_backup_root" 2>/dev/null || true
}
trap restore_database EXIT
mv -- "$database" "$hidden_database"
[[ ! -e "$database" ]] || {
  printf 'Pilot C canonical database remained visible during offline verification\n' >&2
  exit 71
}

"$mcl_bin" --root "$offline_root" --json release verify \
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
  (.observed_lean_toolchain | contains("version 4.32.0-rc1"))
' "$evidence_dir/release-verification.json" >/dev/null || {
  printf 'Pilot C release failed database-independent verification\n' >&2
  exit 71
}

export_corpus() {
  "$mcl_bin" --root "$offline_root" --json release export \
    --bundle-dir "$release_copy" \
    --expected-manifest-hash "$manifest_hash" \
    --packet-id mathos.probability.pilot_c_bh.v1 \
    --domain probability \
    --level L7_frontier \
    --difficulty-bin D5 \
    --output-dir "$corpus_output" \
    "$@"
}
export_corpus --dry-run >"$evidence_dir/corpus-export-dry-run.json"
corpus_preview_hash="$(
  jq -er 'select(.dry_run == true and .policy == "private_audit_only") | .manifest_hash' \
    "$evidence_dir/corpus-export-dry-run.json"
)"
[[ ! -e "$corpus_output" && ! -L "$corpus_output" ]] || {
  printf 'Pilot C corpus dry-run wrote its output directory\n' >&2
  exit 71
}
export_corpus >"$evidence_dir/corpus-export-build.json"
corpus_manifest_hash="$(sha256sum "$corpus_output/manifest.json" | cut -d ' ' -f 1)"
[[ "$corpus_preview_hash" == "$corpus_manifest_hash" ]] || {
  printf 'Pilot C corpus dry-run and build identities differ\n' >&2
  exit 71
}
jq -e \
  --arg source_manifest_hash "$manifest_hash" \
  --arg project_hash "$project_hash" '
  .schema_version == "corpus_export_manifest/1" and
  .source_release.release_manifest_hash == $source_manifest_hash and
  .source_release.project_archive_hash == $project_hash and
  .curation.policy == "private_audit_only" and
  .curation.packet_id == "mathos.probability.pilot_c_bh.v1" and
  (.members | length) == 12 and
  .outputs.project_path == "lean-project/project.tar" and
  .outputs.project_sha256 == $project_hash and
  any(.members[];
    .path == "lean-project/project.tar" and
    .content_hash == $project_hash
  )
' "$corpus_output/manifest.json" >/dev/null || {
  printf 'Pilot C corpus manifest lost its exact project projection\n' >&2
  exit 71
}
jq -e '
  .training.eligibility == "private_audit_only" and
  .training.split == "private_audit_only" and
  .training.can_export_proof_body == false and
  .trust.public_claim_class == "private_only" and
  .hashes.private_artifact_bundle_sha256 != null
' "$corpus_output/mathcorpus/packet.json" >/dev/null || {
  printf 'Pilot C MathCorpus packet violated private export policy\n' >&2
  exit 71
}
jq -e '
  .mcip_version == "1.0.0" and
  (.records | length) == 3 and
  ([.records[].record_type] | sort) ==
    (["dependency_manifest", "packet_identity", "proof_variant"] | sort) and
  ([.records[].export_eligibility] | all(. == "private_only"))
' "$corpus_output/mcip/bundle.json" >/dev/null || {
  printf 'Pilot C MCIP bundle violated its closed projection\n' >&2
  exit 71
}
[[ -z "$(find "$corpus_output" -type l -print -quit)" ]] || {
  printf 'Pilot C corpus export contains a symbolic link\n' >&2
  exit 71
}

corpus_copy="$corpus_parent/$(basename "$corpus_output")-clean-copy"
[[ ! -e "$corpus_copy" && ! -L "$corpus_copy" ]] || {
  printf 'Pilot C clean corpus copy already exists\n' >&2
  exit 66
}
cp -a -- "$corpus_output" "$corpus_copy"
[[ -z "$(find "$corpus_copy" -type f -iname '*.sqlite*' -print -quit)" ]] || {
  printf 'Pilot C corpus export retained a SQLite file\n' >&2
  exit 71
}
"$mcl_bin" --root "$offline_root" --json release verify-export \
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
' "$evidence_dir/corpus-export-verification.json" >/dev/null || {
  printf 'Pilot C corpus export failed database-independent verification\n' >&2
  exit 71
}

jq -cnS \
  --arg receipt_hash "$receipt_hash" \
  --arg release_manifest_hash "$manifest_hash" \
  --arg corpus_manifest_hash "$corpus_manifest_hash" \
  --arg project_hash "$project_hash" '{
  pilot: "pilot_c",
  publication_receipt_hash: $receipt_hash,
  release_manifest_hash: $release_manifest_hash,
  corpus_manifest_hash: $corpus_manifest_hash,
  project_archive_hash: $project_hash,
  database_independent: true
}'
