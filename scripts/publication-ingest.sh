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

"$mcl_bin" --root "$candidate_dir" --json verify promote-publication-authority \
  --publication-receipt-hash "$receipt_hash" \
  --actor publication-boundary \
  --idempotency-key "publication-authority:$receipt_hash" \
  >"$output_dir/publication-authority.json"

expected_evidence_kind="$(
  jq -er '
    if .request.outcome == "proof" then "lean_kernel_proof"
    elif .request.outcome == "refutation" then "lean_kernel_refutation"
    else error("unsupported publication outcome")
    end
  ' "$candidate_dir/publication-report.json"
)"
stage_hash="$(jq -er '.stage.stage_hash' "$output_dir/publication-stage.json")"
jq -e \
  --arg receipt_hash "$receipt_hash" \
  --arg stage_hash "$stage_hash" \
  --arg evidence_kind "$expected_evidence_kind" '
  .dry_run == false and
  .publication_receipt_hash == $receipt_hash and
  .proposed_evidence_hash == .evidence.evidence_hash and
  .evidence_kind == $evidence_kind and
  .evidence.payload.schema_version == "evidence/2" and
  .evidence.payload.evidence_kind == $evidence_kind and
  .evidence.payload.result == "accepted" and
  .evidence.payload.authority_class == "authoritative" and
  .evidence.payload.producing_run_id == null and
  .evidence.payload.producing_job_id == null and
  .evidence.payload.stale == false and
  .evidence.payload.publication_authority.ingestion_receipt_hash == $receipt_hash and
  .evidence.payload.publication_authority.stage_hash == $stage_hash
' "$output_dir/publication-authority.json" >/dev/null || {
  printf 'publication authority output failed its closed contract\n' >&2
  exit 71
}

claim_object_id="$(jq -er '.object_id' "$candidate_dir/closure/claim-version.json")"
claim_version_hash="$(jq -er '.version_hash' "$candidate_dir/closure/claim-version.json")"
formalization_object_id="$(jq -er '.object_id' "$candidate_dir/closure/formalization-version.json")"
formalization_version_hash="$(jq -er '.version_hash' "$candidate_dir/closure/formalization-version.json")"
source_object_id="$(jq -er '.object_id' "$candidate_dir/closure/source-version.json")"
source_version_hash="$(jq -er '.version_hash' "$candidate_dir/closure/source-version.json")"
module_hash="$(jq -er '.payload.module_artifact_hash' "$candidate_dir/closure/formalization-version.json")"

"$mcl_bin" --root "$candidate_dir" --json verify claim-status \
  --claim-object-id "$claim_object_id" \
  --claim-version-hash "$claim_version_hash" \
  >"$output_dir/claim-status-before-fidelity.json"

jq -e \
  --arg claim_object_id "$claim_object_id" \
  --arg claim_version_hash "$claim_version_hash" '
  .schema_version == "claim_research_status/1" and
  .claim.object_id == $claim_object_id and
  .claim.version_hash == $claim_version_hash and
  .status == "open" and
  (.witnesses | length) == 0 and
  (.nonqualifications | length) == 1 and
  .nonqualifications[0].reason == "no_current_verified_fidelity"
' "$output_dir/claim-status-before-fidelity.json" >/dev/null || {
  printf 'claim status before fidelity failed its closed contract\n' >&2
  exit 71
}

reviewer="protected-fidelity-reviewer"
"$mcl_bin" --root "$candidate_dir" --json research start \
  --kind literature_review \
  --budget-json '{}' \
  --actor "$reviewer" \
  --idempotency-key "publication-fidelity-run:$receipt_hash" \
  >"$output_dir/fidelity-review-run.json"
fidelity_run_id="$(jq -er '.run.run_id' "$output_dir/fidelity-review-run.json")"

reviewed_source_relation="$([ "$expected_evidence_kind" = "lean_kernel_proof" ] && printf claim || printf logical_negation)"
expected_research_status="$([ "$expected_evidence_kind" = "lean_kernel_proof" ] && printf proved || printf disproved)"
expected_witness_kind="$([ "$expected_evidence_kind" = "lean_kernel_proof" ] && printf proof || printf refutation)"
jq -cnS \
  --arg source_object_id "$source_object_id" \
  --arg source_version_hash "$source_version_hash" \
  --arg claim_object_id "$claim_object_id" \
  --arg claim_version_hash "$claim_version_hash" \
  --arg formalization_object_id "$formalization_object_id" \
  --arg formalization_version_hash "$formalization_version_hash" \
  --arg reviewed_source_relation "$reviewed_source_relation" \
  --arg reviewer "$reviewer" \
  --arg module_hash "$module_hash" \
  --arg fidelity_run_id "$fidelity_run_id" '
  {
    schema_version: "fidelity_review_request/2",
    source: {object_id: $source_object_id, version_hash: $source_version_hash},
    claim: {object_id: $claim_object_id, version_hash: $claim_version_hash},
    formalization: {
      object_id: $formalization_object_id,
      version_hash: $formalization_version_hash
    },
    reviewed_source_relation: $reviewed_source_relation,
    review_level: "mathematical_statement",
    verdict: "verified",
    reviewer_identity: $reviewer,
    findings: [
      "Protected role-separated review confirms the exact declaration states the selected source relation."
    ],
    ambiguity_disposition: "no_ambiguity",
    definition_mappings: [],
    supporting_artifact_hashes: [$module_hash],
    producing_run_id: $fidelity_run_id,
    supersedes_evidence_id: null
  }
' >"$output_dir/fidelity-review-request.json"

"$mcl_bin" --root "$candidate_dir" --json verify review-fidelity \
  --request-json "$(<"$output_dir/fidelity-review-request.json")" \
  --actor "$reviewer" \
  --idempotency-key "publication-fidelity-review:$receipt_hash" \
  >"$output_dir/fidelity-review.json"
fidelity_evidence_id="$(jq -er '.evidence.evidence_id' "$output_dir/fidelity-review.json")"
fidelity_evidence_hash="$(jq -er '.evidence.evidence_hash' "$output_dir/fidelity-review.json")"
fidelity_report_hash="$(jq -er '.proposed_report_artifact_hash' "$output_dir/fidelity-review.json")"
authority_evidence_id="$(jq -er '.evidence.evidence_id' "$output_dir/publication-authority.json")"
authority_evidence_hash="$(jq -er '.evidence.evidence_hash' "$output_dir/publication-authority.json")"
cp -- "$(cas_path "$fidelity_report_hash")" "$output_dir/fidelity-review-report.json"
echo "$fidelity_report_hash  $output_dir/fidelity-review-report.json" | sha256sum --check --strict
jq -e \
  --arg reviewer "$reviewer" \
  --arg relation "$reviewed_source_relation" \
  --arg fidelity_report_hash "$fidelity_report_hash" '
  .dry_run == false and
  .proposed_report_artifact_hash == $fidelity_report_hash and
  .report.schema_version == "fidelity_review_report/2" and
  .report.reviewed_source_relation == $relation and
  .report.request.reviewed_source_relation == $relation and
  .report.request.reviewer_identity == $reviewer and
  .report.formalization_author != $reviewer and
  .evidence.payload.evidence_kind == "statement_fidelity_review" and
  .evidence.payload.result == "accepted" and
  .evidence.payload.authority_class == "reviewed" and
  .evidence.payload.stale == false and
  (.evidence.payload.artifact_hashes | index($fidelity_report_hash)) != null
' "$output_dir/fidelity-review.json" >/dev/null || {
  printf 'protected fidelity review output failed its closed contract\n' >&2
  exit 71
}

"$mcl_bin" --root "$candidate_dir" --json verify claim-status \
  --claim-object-id "$claim_object_id" \
  --claim-version-hash "$claim_version_hash" \
  >"$output_dir/claim-status-after-fidelity.json"

jq -e \
  --arg claim_object_id "$claim_object_id" \
  --arg claim_version_hash "$claim_version_hash" \
  --arg expected_status "$expected_research_status" \
  --arg witness_kind "$expected_witness_kind" \
  --arg relation "$reviewed_source_relation" \
  --arg fidelity_evidence_id "$fidelity_evidence_id" \
  --arg fidelity_evidence_hash "$fidelity_evidence_hash" \
  --arg fidelity_report_hash "$fidelity_report_hash" \
  --arg authority_evidence_id "$authority_evidence_id" \
  --arg authority_evidence_hash "$authority_evidence_hash" \
  --arg receipt_hash "$receipt_hash" '
  .schema_version == "claim_research_status/1" and
  .claim.object_id == $claim_object_id and
  .claim.version_hash == $claim_version_hash and
  .status == $expected_status and
  (.nonqualifications | length) == 0 and
  (.witnesses | length) == 1 and
  .witnesses[0].kind == $witness_kind and
  .witnesses[0].reviewed_source_relation == $relation and
  .witnesses[0].fidelity_request_schema_version == "fidelity_review_request/2" and
  .witnesses[0].fidelity_evidence_id == $fidelity_evidence_id and
  .witnesses[0].fidelity_evidence_hash == $fidelity_evidence_hash and
  .witnesses[0].fidelity_report_artifact_hash == $fidelity_report_hash and
  .witnesses[0].authority_evidence_id == $authority_evidence_id and
  .witnesses[0].authority_evidence_hash == $authority_evidence_hash and
  .witnesses[0].publication_receipt_hash == $receipt_hash
' "$output_dir/claim-status-after-fidelity.json" >/dev/null || {
  printf 'claim status after fidelity failed its closed contract\n' >&2
  exit 71
}

jq -cS . "$output_dir/claim-status-after-fidelity.json"
