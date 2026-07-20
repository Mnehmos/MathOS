#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  printf 'usage: %s <candidate-directory> <attestation-bundle> <output-directory>\n' "$0" >&2
  exit 64
fi

candidate_dir="$1"
bundle="$2"
output_dir="$3"
mcl_bin="${PUBLICATION_MCL_BIN:-target/debug/mcl}"

[[ -d "$candidate_dir" && ! -L "$candidate_dir" ]] || {
  printf 'publication candidate directory is unavailable or unsafe\n' >&2
  exit 66
}
[[ -f "$bundle" && ! -L "$bundle" ]] || {
  printf 'publication attestation bundle is unavailable or unsafe\n' >&2
  exit 66
}
[[ ! -e "$output_dir" && ! -L "$output_dir" ]] || {
  printf 'publication ingestion output already exists or is unsafe\n' >&2
  exit 66
}
[[ -x "$mcl_bin" ]] || {
  printf 'publication mcl binary is unavailable: %s\n' "$mcl_bin" >&2
  exit 69
}

candidate_dir="$(cd "$candidate_dir" && pwd -P)"
bundle_parent="$(cd "$(dirname "$bundle")" && pwd -P)"
bundle="$bundle_parent/$(basename "$bundle")"
output_parent="$(cd "$(dirname "$output_dir")" && pwd -P)"
output_dir="$output_parent/$(basename "$output_dir")"
mkdir -- "$output_dir"

staged_bundle="$candidate_dir/publication-attestation.json"
[[ ! -e "$staged_bundle" && ! -L "$staged_bundle" ]] || {
  printf 'controlled staged bundle destination already exists\n' >&2
  exit 66
}
cp -- "$bundle" "$staged_bundle"
bundle_hash="$(sha256sum "$staged_bundle" | cut -d ' ' -f 1)"
report_hash="$(sha256sum "$candidate_dir/publication-report.json" | cut -d ' ' -f 1)"

"$mcl_bin" --root "$candidate_dir" --json verify stage-publication-candidate \
  --report-file publication-report.json \
  --retained-closure-file publication-retained-closure.json \
  --retained-root . \
  --attestation-bundle-file publication-attestation.json \
  --actor publication-boundary \
  --idempotency-key "publication-stage:$report_hash:$bundle_hash" \
  >"$output_dir/publication-stage.json"

jq -e \
  --arg report_hash "$report_hash" \
  --arg bundle_hash "$bundle_hash" '
  .dry_run == false and
  .report_artifact_hash == $report_hash and
  .attestation_bundle_artifact_hash == $bundle_hash and
  .stage.stage.authoritative == false and
  .authoritative == false
' "$output_dir/publication-stage.json" >/dev/null || {
  printf 'publication stage output failed its closed contract\n' >&2
  exit 71
}

"$mcl_bin" --root "$candidate_dir" --json verify ingest-publication \
  --report-artifact-hash "$report_hash" \
  --attestation-bundle-artifact-hash "$bundle_hash" \
  --actor publication-boundary \
  --idempotency-key "publication-ingestion:$report_hash:$bundle_hash" \
  >"$output_dir/publication-ingestion.json"

receipt_hash="$(jq -er '.proposed_receipt_hash' "$output_dir/publication-ingestion.json")"
raw_hash="$(jq -er '.verification.raw_verification_hash' "$output_dir/publication-ingestion.json")"
jq -e \
  --arg report_hash "$report_hash" \
  --arg bundle_hash "$bundle_hash" \
  --arg receipt_hash "$receipt_hash" '
  .dry_run == false and
  .verification.report_content_hash == $report_hash and
  .verification.report_artifact_hash == $report_hash and
  .verification.attestation_bundle_hash == $bundle_hash and
  .verification.authoritative == false and
  .receipt.receipt_hash == $receipt_hash and
  .receipt.verification == .verification
' "$output_dir/publication-ingestion.json" >/dev/null || {
  printf 'publication ingestion output failed its closed contract\n' >&2
  exit 71
}

cas_path() {
  local hash="$1"
  printf '%s/.mcl/artifacts/sha256/%s/%s/%s' \
    "$candidate_dir" "${hash:0:2}" "${hash:2:2}" "$hash"
}

cp -- "$(cas_path "$raw_hash")" "$output_dir/attestation-verification-raw.json"
cp -- "$(cas_path "$receipt_hash")" "$output_dir/attestation-verification.json"
echo "$raw_hash  $output_dir/attestation-verification-raw.json" | sha256sum --check --strict
echo "$receipt_hash  $output_dir/attestation-verification.json" | sha256sum --check --strict

jq -cS . "$output_dir/publication-ingestion.json"
