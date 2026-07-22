#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 5 ]]; then
  printf 'usage: %s <state-root> <boundary-artifact> <run-bundle> <attestation-bundle> <output-directory>\n' "$0" >&2
  exit 64
fi

state_root="$1"
boundary_root="$2"
run_source="$3"
bundle_source="$4"
output_dir="$5"
mcl_bin="${COMPARATOR_AUTHORITY_MCL_BIN:-target/debug/mcl}"

for directory in "$state_root" "$boundary_root" "$run_source"; do
  [[ -d "$directory" && ! -L "$directory" ]] || {
    printf 'Comparator authority input directory is unavailable or unsafe: %s\n' "$directory" >&2
    exit 66
  }
done
[[ -f "$bundle_source" && ! -L "$bundle_source" ]] || {
  printf 'Comparator authority attestation bundle is unavailable or unsafe\n' >&2
  exit 66
}
[[ -x "$mcl_bin" ]] || {
  printf 'Comparator authority mcl binary is unavailable: %s\n' "$mcl_bin" >&2
  exit 69
}

state_root="$(cd "$state_root" && pwd -P)"
boundary_root="$(cd "$boundary_root" && pwd -P)"
run_source="$(cd "$run_source" && pwd -P)"
bundle_source="$(cd "$(dirname "$bundle_source")" && pwd -P)/$(basename "$bundle_source")"
[[ -f "$state_root/.mcl/state.sqlite3" && ! -L "$state_root/.mcl/state.sqlite3" ]] || {
  printf 'Comparator authority state snapshot is not initialized\n' >&2
  exit 66
}

output_parent="$(cd "$(dirname "$output_dir")" && pwd -P)"
output_dir="$output_parent/$(basename "$output_dir")"
case "$output_dir/" in
  "$state_root/"*) ;;
  *)
    printf 'Comparator authority output must be inside the canonical state root\n' >&2
    exit 66
    ;;
esac
[[ ! -e "$output_dir" && ! -L "$output_dir" ]] || {
  printf 'Comparator authority output already exists or is unsafe\n' >&2
  exit 66
}

plan_source="$boundary_root/publication-candidate/release-build-evidence/comparator-package-plan.json"
release_source="$boundary_root/pilot-a-portable-release"
[[ -f "$plan_source" && ! -L "$plan_source" \
    && -d "$release_source" && ! -L "$release_source" ]] || {
  printf 'same-run Comparator plan or frozen source release is unavailable\n' >&2
  exit 66
}

input_root="$state_root/comparator-authority-input"
[[ ! -e "$input_root" && ! -L "$input_root" ]] || {
  printf 'Comparator authority controlled input root already exists\n' >&2
  exit 66
}
mkdir -- "$input_root" "$output_dir"
cp -a -- "$run_source" "$input_root/run"
cp -a -- "$release_source" "$input_root/release"
cp -- "$plan_source" "$input_root/plan.json"
cp -- "$bundle_source" "$input_root/attestation.json"

report_hash="$(sha256sum "$input_root/run/report.json" | cut -d ' ' -f 1)"
package_hash="$(sha256sum "$input_root/run/package/verification.json" | cut -d ' ' -f 1)"
release_hash="$(sha256sum "$input_root/release/manifest.json" | cut -d ' ' -f 1)"
bundle_hash="$(sha256sum "$input_root/attestation.json" | cut -d ' ' -f 1)"

stage_args=(
  --run-dir "$input_root/run"
  --expected-report-hash "$report_hash"
  --expected-package-verification-hash "$package_hash"
  --plan-file "$input_root/plan.json"
  --release-dir "$input_root/release"
  --expected-release-manifest-hash "$release_hash"
  --attestation-bundle-file "$input_root/attestation.json"
  --actor comparator-authority-boundary
  --idempotency-key "comparator-authority-stage:$report_hash:$bundle_hash"
)

"$mcl_bin" --root "$state_root" --json verify stage-comparator-authority \
  "${stage_args[@]}" --dry-run >"$output_dir/stage-dry-run.json"
"$mcl_bin" --root "$state_root" --json verify stage-comparator-authority \
  "${stage_args[@]}" >"$output_dir/stage.json"
"$mcl_bin" --root "$state_root" --json verify stage-comparator-authority \
  "${stage_args[@]}" >"$output_dir/stage-retry.json"
cmp --silent "$output_dir/stage.json" "$output_dir/stage-retry.json" || {
  printf 'Comparator authority stage retry was not byte-identical\n' >&2
  exit 71
}
stage_hash="$(jq -er '.stage.stage_hash' "$output_dir/stage.json")"
jq -e \
  --arg report "$report_hash" \
  --arg package "$package_hash" \
  --arg bundle "$bundle_hash" \
  --arg stage "$stage_hash" '
  .dry_run == false and
  .proposed_stage_hash == $stage and
  .report_artifact_hash == $report and
  .package_verification_hash == $package and
  .attestation_bundle_artifact_hash == $bundle and
  .stage.stage.authoritative == false and
  .authoritative == false
' "$output_dir/stage.json" >/dev/null || {
  printf 'Comparator authority stage output failed its closed contract\n' >&2
  exit 71
}

ingest_args=(
  --report-artifact-hash "$report_hash"
  --attestation-bundle-artifact-hash "$bundle_hash"
  --actor comparator-authority-boundary
  --idempotency-key "comparator-authority-ingestion:$stage_hash"
)
"$mcl_bin" --root "$state_root" --json verify ingest-comparator-authority \
  "${ingest_args[@]}" >"$output_dir/ingestion.json"
"$mcl_bin" --root "$state_root" --json verify ingest-comparator-authority \
  "${ingest_args[@]}" >"$output_dir/ingestion-retry.json"
cmp --silent "$output_dir/ingestion.json" "$output_dir/ingestion-retry.json" || {
  printf 'Comparator authority ingestion retry was not byte-identical\n' >&2
  exit 71
}
receipt_hash="$(jq -er '.receipt.receipt_hash' "$output_dir/ingestion.json")"
raw_hash="$(jq -er '.verification.raw_verification_hash' "$output_dir/ingestion.json")"
jq -e \
  --arg stage "$stage_hash" \
  --arg report "$report_hash" \
  --arg bundle "$bundle_hash" \
  --arg receipt "$receipt_hash" '
  .dry_run == false and
  .proposed_receipt_hash == $receipt and
  .verification.stage_hash == $stage and
  .verification.report_artifact_hash == $report and
  .verification.attestation_bundle_hash == $bundle and
  .verification.verified_attestation_count == 1 and
  .verification.verified_timestamp_count >= 1 and
  .verification.authoritative == false and
  .receipt.verification == .verification
' "$output_dir/ingestion.json" >/dev/null || {
  printf 'Comparator authority ingestion output failed its closed contract\n' >&2
  exit 71
}

promote_args=(
  --comparator-receipt-hash "$receipt_hash"
  --actor comparator-authority-boundary
  --idempotency-key "comparator-authority-promotion:$receipt_hash"
)
"$mcl_bin" --root "$state_root" --json verify promote-comparator-authority \
  "${promote_args[@]}" --dry-run >"$output_dir/promotion-dry-run.json"
"$mcl_bin" --root "$state_root" --json verify promote-comparator-authority \
  "${promote_args[@]}" >"$output_dir/promotion.json"
"$mcl_bin" --root "$state_root" --json verify promote-comparator-authority \
  "${promote_args[@]}" >"$output_dir/promotion-retry.json"
cmp --silent "$output_dir/promotion.json" "$output_dir/promotion-retry.json" || {
  printf 'Comparator authority promotion retry was not byte-identical\n' >&2
  exit 71
}
evidence_id="$(jq -er '.evidence.evidence_id' "$output_dir/promotion.json")"
evidence_hash="$(jq -er '.evidence.evidence_hash' "$output_dir/promotion.json")"
jq -e \
  --arg receipt "$receipt_hash" \
  --arg stage "$stage_hash" '
  .dry_run == false and
  .comparator_receipt_hash == $receipt and
  .evidence_kind == "comparator_run" and
  .proposed_evidence_hash == .evidence.evidence_hash and
  .evidence.payload.schema_version == "evidence/3" and
  .evidence.payload.evidence_kind == "comparator_run" and
  .evidence.payload.result == "accepted" and
  .evidence.payload.authority_class == "authoritative" and
  .evidence.payload.producing_run_id == null and
  .evidence.payload.producing_job_id == null and
  .evidence.payload.publication_authority == null and
  .evidence.payload.comparator_authority.ingestion_receipt_hash == $receipt and
  .evidence.payload.comparator_authority.stage_hash == $stage
' "$output_dir/promotion.json" >/dev/null || {
  printf 'Comparator authority promotion output failed its closed contract\n' >&2
  exit 71
}

"$mcl_bin" --root "$state_root" --json verify comparator-authority-status \
  --evidence-id "$evidence_id" >"$output_dir/status.json"
jq -e \
  --arg evidence_id "$evidence_id" \
  --arg evidence_hash "$evidence_hash" \
  --arg receipt "$receipt_hash" '
  .schema_version == "comparator_authority_status/1" and
  .evidence_id == $evidence_id and
  .evidence_hash == $evidence_hash and
  .ingestion_receipt_hash == $receipt and
  .currentness == "current" and
  .stale_reasons == [] and
  .authoritative == true
' "$output_dir/status.json" >/dev/null || {
  printf 'Comparator authority currentness output failed its closed contract\n' >&2
  exit 71
}

cas_path() {
  local hash="$1"
  printf '%s/.mcl/artifacts/sha256/%s/%s/%s' \
    "$state_root" "${hash:0:2}" "${hash:2:2}" "$hash"
}

portable="$output_dir/portable"
mkdir -- "$portable" "$portable/objects"
jq -r '.evidence.payload.artifact_hashes[]' "$output_dir/promotion.json" \
  >"$portable/object-hashes.txt"
while IFS= read -r hash; do
  [[ "$hash" =~ ^[0-9a-f]{64}$ ]] || {
    printf 'invalid portable Comparator authority object hash\n' >&2
    exit 71
  }
  source_object="$(cas_path "$hash")"
  [[ -f "$source_object" && ! -L "$source_object" ]] || {
    printf 'portable Comparator authority object is missing: %s\n' "$hash" >&2
    exit 71
  }
  cp -- "$source_object" "$portable/objects/$hash"
  echo "$hash  $portable/objects/$hash" | sha256sum --check --strict
done <"$portable/object-hashes.txt"

cp -- "$(cas_path "$stage_hash")" "$portable/stage.json"
cp -- "$(cas_path "$receipt_hash")" "$portable/receipt.json"
cp -- "$(cas_path "$raw_hash")" "$portable/attestation-verification-raw.json"
cp -- "$output_dir/promotion.json" "$portable/promotion.json"
cp -- "$output_dir/status.json" "$portable/status.json"
jq -cS '.evidence' "$output_dir/promotion.json" | tr -d '\n' >"$portable/evidence.json"
jq -cS '.evidence.payload' "$output_dir/promotion.json" \
  | tr -d '\n' >"$portable/evidence-payload.json"
echo "$evidence_hash  $portable/evidence-payload.json" | sha256sum --check --strict
jq -cS -n \
  --arg stage_hash "$stage_hash" \
  --arg receipt_hash "$receipt_hash" \
  --arg evidence_id "$evidence_id" \
  --arg evidence_hash "$evidence_hash" \
  --arg report_hash "$report_hash" \
  --arg package_hash "$package_hash" \
  --arg release_hash "$release_hash" \
  --arg bundle_hash "$bundle_hash" \
  --arg raw_hash "$raw_hash" \
  --argjson object_count "$(wc -l <"$portable/object-hashes.txt")" \
  '{schema_version:"comparator_authority_portable_audit/1",stage_hash:$stage_hash,receipt_hash:$receipt_hash,evidence_id:$evidence_id,evidence_hash:$evidence_hash,report_hash:$report_hash,package_verification_hash:$package_hash,source_release_manifest_hash:$release_hash,attestation_bundle_hash:$bundle_hash,raw_verification_hash:$raw_hash,object_count:$object_count,database_required:false}' \
  | tr -d '\n' >"$portable/audit-index.json"

printf '%s\n' "$evidence_hash" >"$output_dir/evidence.sha256"
