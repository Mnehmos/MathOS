#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 || $# -gt 4 ]]; then
  printf 'usage: %s <candidate-directory> <attestation-bundle> <output-directory> [state-directory]\n' "$0" >&2
  exit 64
fi

candidate_dir="$1"
bundle="$2"
output_dir="$3"
requested_state_root="${4:-$candidate_dir}"
mcl_bin="${PUBLICATION_MCL_BIN:-target/debug/mcl}"

[[ -d "$candidate_dir" && ! -L "$candidate_dir" ]] || {
  printf 'publication candidate directory is unavailable or unsafe\n' >&2
  exit 66
}
[[ -f "$bundle" && ! -L "$bundle" ]] || {
  printf 'publication attestation bundle is unavailable or unsafe\n' >&2
  exit 66
}
[[ -d "$requested_state_root" && ! -L "$requested_state_root" ]] || {
  printf 'publication state directory is unavailable or unsafe\n' >&2
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
state_root="$(cd "$requested_state_root" && pwd -P)"
case "$candidate_dir/" in
  "$state_root/"*) ;;
  *)
    printf 'publication candidate must be contained by the canonical state directory\n' >&2
    exit 66
    ;;
esac
bundle_parent="$(cd "$(dirname "$bundle")" && pwd -P)"
bundle="$bundle_parent/$(basename "$bundle")"
output_parent="$(cd "$(dirname "$output_dir")" && pwd -P)"
output_dir="$output_parent/$(basename "$output_dir")"
case "$output_dir/" in
  "$state_root/"*) ;;
  *)
    printf 'publication ingestion output must be contained by the canonical state directory\n' >&2
    exit 66
    ;;
esac
mkdir -- "$output_dir"

staged_bundle="$candidate_dir/publication-attestation.json"
[[ ! -e "$staged_bundle" && ! -L "$staged_bundle" ]] || {
  printf 'controlled staged bundle destination already exists\n' >&2
  exit 66
}
cp -- "$bundle" "$staged_bundle"
bundle_hash="$(sha256sum "$staged_bundle" | cut -d ' ' -f 1)"
report_hash="$(sha256sum "$candidate_dir/publication-report.json" | cut -d ' ' -f 1)"

"$mcl_bin" --root "$state_root" --json verify stage-publication-candidate \
  --report-file "$candidate_dir/publication-report.json" \
  --retained-closure-file "$candidate_dir/publication-retained-closure.json" \
  --retained-root "$candidate_dir" \
  --attestation-bundle-file "$staged_bundle" \
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

"$mcl_bin" --root "$state_root" --json verify ingest-publication \
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
    "$state_root" "${hash:0:2}" "${hash:2:2}" "$hash"
}

cp -- "$(cas_path "$raw_hash")" "$output_dir/attestation-verification-raw.json"
cp -- "$(cas_path "$receipt_hash")" "$output_dir/attestation-verification.json"
echo "$raw_hash  $output_dir/attestation-verification-raw.json" | sha256sum --check --strict
echo "$receipt_hash  $output_dir/attestation-verification.json" | sha256sum --check --strict

"$mcl_bin" --root "$state_root" --json verify promote-publication-authority \
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

"$mcl_bin" --root "$state_root" --json verify claim-status \
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
"$mcl_bin" --root "$state_root" --json research start \
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

"$mcl_bin" --root "$state_root" --json verify review-fidelity \
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

"$mcl_bin" --root "$state_root" --json verify claim-status \
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

if [[ "$expected_evidence_kind" == "lean_kernel_refutation" ]]; then
  counterexample_researcher="protected-counterexample-researcher"
  "$mcl_bin" --root "$state_root" --json research start \
    --kind counterexample_search \
    --budget-json '{"max_candidates":1}' \
    --actor "$counterexample_researcher" \
    --idempotency-key "pilot-a-counterexample-search:$receipt_hash" \
    >"$output_dir/counterexample-search-run.json"
  counterexample_run_id="$(jq -er '.run.run_id' "$output_dir/counterexample-search-run.json")"
  counterexample_run_head="$(jq -er '.run.event_head_hash' "$output_dir/counterexample-search-run.json")"

  jq -cnS \
    --arg claim_object_id "$claim_object_id" \
    --arg claim_version_hash "$claim_version_hash" \
    --arg formalization_object_id "$formalization_object_id" \
    --arg formalization_version_hash "$formalization_version_hash" '
    {
      schema_version: "counterexample_search_result/1",
      original_claim: {
        object_id: $claim_object_id,
        version_hash: $claim_version_hash
      },
      refutation_formalization: {
        object_id: $formalization_object_id,
        version_hash: $formalization_version_hash
      },
      witness: {mathematical_type: "Nat", canonical_value: 2},
      result: "counterexample_confirmed"
    }
  ' >"$output_dir/counterexample-search-event.json"
  "$mcl_bin" --root "$state_root" --json research submit \
    --run-id "$counterexample_run_id" \
    --expected-head "$counterexample_run_head" \
    --kind observation \
    --payload-json "$(<"$output_dir/counterexample-search-event.json")" \
    --actor "$counterexample_researcher" \
    --idempotency-key "pilot-a-counterexample-result:$receipt_hash" \
    >"$output_dir/counterexample-search-result.json"
  counterexample_run_head="$(jq -er '.event.event_hash' "$output_dir/counterexample-search-result.json")"
  "$mcl_bin" --root "$state_root" --json research events \
    --run-id "$counterexample_run_id" \
    >"$output_dir/counterexample-search-events.json"
  "$mcl_bin" --root "$state_root" --json research verify \
    --run-id "$counterexample_run_id" \
    >"$output_dir/counterexample-search-verification.json"
  jq -e \
    --arg run_id "$counterexample_run_id" \
    --arg head "$counterexample_run_head" '
    .run_id == $run_id and
    .valid == true and
    .event_count == 2 and
    .head_hash == $head and
    .first_invalid_sequence == null
  ' "$output_dir/counterexample-search-verification.json" >/dev/null || {
    printf 'Pilot A counterexample search chain failed exact replay\n' >&2
    exit 71
  }

  jq -cnS \
    --arg source_object_id "$source_object_id" \
    --arg source_version_hash "$source_version_hash" \
    --arg claim_object_id "$claim_object_id" \
    --arg claim_version_hash "$claim_version_hash" \
    --arg formalization_object_id "$formalization_object_id" \
    --arg formalization_version_hash "$formalization_version_hash" \
    --arg module_hash "$module_hash" \
    --arg run_id "$counterexample_run_id" \
    --arg run_head "$counterexample_run_head" '
    {
      schema_version: "counterexample_repair_request/1",
      original_claim: {
        object_id: $claim_object_id,
        version_hash: $claim_version_hash
      },
      refutation_formalization: {
        object_id: $formalization_object_id,
        version_hash: $formalization_version_hash
      },
      witness: {mathematical_type: "Nat", canonical_value: 2},
      minimization: {
        explanation: "The witness is one canonical scalar natural number; no structural reduction is applicable.",
        supporting_artifact_hashes: [$module_hash]
      },
      failing_assumption_explanation: "The universal conclusion fails at boundary prime 2 because the retained odd predicate excludes 2; the repaired claim excludes that boundary case.",
      repair_operation: "exclude_boundary_case",
      proposed_repaired_claim: {
        source_reference: {
          object_id: $source_object_id,
          version_hash: $source_version_hash
        },
        normalized_informal_statement: "Every prime number other than 2 is odd.",
        claim_kind: "universal",
        logical_shape: "forall n : Nat, Prime(n) -> n != 2 -> Odd(n)",
        assumptions: ["n != 2"],
        variables: [{
          symbol: "n",
          domain: "natural numbers",
          notes: "Prime and odd use the exact predicates retained in the refutation module."
        }],
        concept_links: [],
        source_citations: [],
        ambiguity_notes: []
      },
      repaired_claim_searchable_text: "Every prime number other than 2 is odd",
      counterexample_search_run_id: $run_id,
      counterexample_search_run_head_hash: $run_head
    }
  ' >"$output_dir/counterexample-repair-request.json"

  "$mcl_bin" --root "$state_root" --json counterexample repair \
    --request-json "$(<"$output_dir/counterexample-repair-request.json")" \
    --actor "$counterexample_researcher" \
    --idempotency-key "pilot-a-counterexample-repair:$receipt_hash" \
    --dry-run \
    >"$output_dir/counterexample-repair-dry-run.json"
  "$mcl_bin" --root "$state_root" --json counterexample repair \
    --request-json "$(<"$output_dir/counterexample-repair-request.json")" \
    --actor "$counterexample_researcher" \
    --idempotency-key "pilot-a-counterexample-repair:$receipt_hash" \
    >"$output_dir/counterexample-repair.json"
  "$mcl_bin" --root "$state_root" --json counterexample repair \
    --request-json "$(<"$output_dir/counterexample-repair-request.json")" \
    --actor "$counterexample_researcher" \
    --idempotency-key "pilot-a-counterexample-repair:$receipt_hash" \
    >"$output_dir/counterexample-repair-retry.json"
  cmp --silent \
    "$output_dir/counterexample-repair.json" \
    "$output_dir/counterexample-repair-retry.json" || {
    printf 'Pilot A counterexample repair retry changed its exact result\n' >&2
    exit 71
  }

  counterexample_package_hash="$(jq -er '.proposed_counterexample_package_artifact_hash' "$output_dir/counterexample-repair.json")"
  repaired_claim_object_id="$(jq -er '.repair.repaired_claim.object_id' "$output_dir/counterexample-repair.json")"
  repaired_claim_version_hash="$(jq -er '.repair.repaired_claim.version_hash' "$output_dir/counterexample-repair.json")"
  cp -- "$(cas_path "$counterexample_package_hash")" "$output_dir/counterexample-package.json"
  echo "$counterexample_package_hash  $output_dir/counterexample-package.json" \
    | sha256sum --check --strict

  "$mcl_bin" --root "$state_root" --json counterexample get \
    --artifact-hash "$counterexample_package_hash" \
    >"$output_dir/counterexample-package-read.json"
  "$mcl_bin" --root "$state_root" --json verify claim-status \
    --claim-object-id "$claim_object_id" \
    --claim-version-hash "$claim_version_hash" \
    >"$output_dir/claim-status-after-repair-original.json"
  "$mcl_bin" --root "$state_root" --json verify claim-status \
    --claim-object-id "$repaired_claim_object_id" \
    --claim-version-hash "$repaired_claim_version_hash" \
    >"$output_dir/claim-status-repaired.json"

  jq -e \
    --arg package_hash "$counterexample_package_hash" \
    --arg source_object_id "$source_object_id" \
    --arg source_version_hash "$source_version_hash" \
    --arg claim_object_id "$claim_object_id" \
    --arg claim_version_hash "$claim_version_hash" \
    --arg formalization_object_id "$formalization_object_id" \
    --arg formalization_version_hash "$formalization_version_hash" \
    --arg run_id "$counterexample_run_id" \
    --arg run_head "$counterexample_run_head" \
    --arg repaired_claim_object_id "$repaired_claim_object_id" \
    --arg repaired_claim_version_hash "$repaired_claim_version_hash" '
    .dry_run == false and
    .proposed_counterexample_package_artifact_hash == $package_hash and
    .proposed_repaired_claim_version_hash == $repaired_claim_version_hash and
    .package.schema_version == "counterexample_package/1" and
    .package.source == {object_id: $source_object_id, version_hash: $source_version_hash} and
    .package.original_claim == {object_id: $claim_object_id, version_hash: $claim_version_hash} and
    .package.witness == {mathematical_type: "Nat", canonical_value: 2} and
    .package.refutation_witness.formalization == {object_id: $formalization_object_id, version_hash: $formalization_version_hash} and
    .package.refutation_witness.kind == "refutation" and
    .package.repair_operation == "exclude_boundary_case" and
    .package.proposed_repaired_claim.payload.normalized_informal_statement == "Every prime number other than 2 is odd." and
    .package.search_provenance == {run_id: $run_id, event_head_hash: $run_head} and
    .repair.package_artifact.artifact_hash == $package_hash and
    .repair.package_artifact.creation_source == "generated" and
    .repair.package_artifact.restriction == "private" and
    .repair.package_artifact.semantic_metadata.artifact_role == "counterexample_package" and
    .repair.repaired_claim.object_id == $repaired_claim_object_id and
    .repair.repaired_claim.version_hash == $repaired_claim_version_hash and
    .repair.repaired_claim.predecessor_hash == null and
    .repair.repair_edge.kind == "research.repairs" and
    .repair.repair_edge.source_object_id == $repaired_claim_object_id and
    .repair.repair_edge.source_version_hash == $repaired_claim_version_hash and
    .repair.repair_edge.target_object_id == $claim_object_id and
    .repair.repair_edge.target_version_hash == $claim_version_hash and
    .repair.repair_edge.payload.counterexample_package_artifact_hash == $package_hash
  ' "$output_dir/counterexample-repair.json" >/dev/null || {
    printf 'Pilot A counterexample repair failed its closed contract\n' >&2
    exit 71
  }
  jq -e \
    --arg package_hash "$counterexample_package_hash" \
    --arg repaired_claim_version_hash "$repaired_claim_version_hash" '
    .dry_run == true and
    .repair == null and
    .proposed_counterexample_package_artifact_hash == $package_hash and
    .proposed_repaired_claim_version_hash == $repaired_claim_version_hash
  ' "$output_dir/counterexample-repair-dry-run.json" >/dev/null || {
    printf 'Pilot A counterexample repair dry-run changed proposed identities\n' >&2
    exit 71
  }
  jq -e --slurpfile repair "$output_dir/counterexample-repair.json" '
    $repair[0] as $expected |
    .artifact == $expected.repair.package_artifact and
    .package == $expected.package and
    .repaired_claim == $expected.repair.repaired_claim and
    .repair_edge == $expected.repair.repair_edge
  ' "$output_dir/counterexample-package-read.json" >/dev/null || {
    printf 'Pilot A counterexample package read did not replay the atomic repair\n' >&2
    exit 71
  }
  cmp --silent \
    "$output_dir/claim-status-after-fidelity.json" \
    "$output_dir/claim-status-after-repair-original.json" || {
    printf 'Pilot A repair changed the original claim status response\n' >&2
    exit 71
  }
  jq -e \
    --arg object_id "$repaired_claim_object_id" \
    --arg version_hash "$repaired_claim_version_hash" '
    .schema_version == "claim_research_status/1" and
    .claim == {object_id: $object_id, version_hash: $version_hash} and
    .status == "not_started" and
    (.witnesses | length) == 0 and
    (.nonqualifications | length) == 0
  ' "$output_dir/claim-status-repaired.json" >/dev/null || {
    printf 'Pilot A repaired claim did not begin as an independent unproved claim\n' >&2
    exit 71
  }
else
  prior_ingestion_dir="$state_root/refutation-ingestion"
  prior_repair="$prior_ingestion_dir/counterexample-repair.json"
  prior_original_status="$prior_ingestion_dir/claim-status-after-repair-original.json"
  prior_package_read="$prior_ingestion_dir/counterexample-package-read.json"
  [[ -f "$prior_repair" && ! -L "$prior_repair" \
      && -f "$prior_original_status" && ! -L "$prior_original_status" \
      && -f "$prior_package_read" && ! -L "$prior_package_read" ]] || {
    printf 'repaired proof requires the retained refutation and atomic repair lifecycle\n' >&2
    exit 71
  }

  original_claim_object_id="$(jq -er '.package.original_claim.object_id' "$prior_repair")"
  original_claim_version_hash="$(jq -er '.package.original_claim.version_hash' "$prior_repair")"
  repaired_claim_object_id="$(jq -er '.repair.repaired_claim.object_id' "$prior_repair")"
  repaired_claim_version_hash="$(jq -er '.repair.repaired_claim.version_hash' "$prior_repair")"
  counterexample_package_hash="$(jq -er '.proposed_counterexample_package_artifact_hash' "$prior_repair")"
  [[ "$claim_object_id" == "$repaired_claim_object_id" \
      && "$claim_version_hash" == "$repaired_claim_version_hash" ]] || {
    printf 'proof publication subject is not the exact repaired claim\n' >&2
    exit 71
  }

  "$mcl_bin" --root "$state_root" --json verify claim-status \
    --claim-object-id "$original_claim_object_id" \
    --claim-version-hash "$original_claim_version_hash" \
    >"$output_dir/claim-status-original-after-proof.json"
  cmp --silent \
    "$prior_original_status" \
    "$output_dir/claim-status-original-after-proof.json" || {
    printf 'repaired proof changed the original disproved claim status\n' >&2
    exit 71
  }

  "$mcl_bin" --root "$state_root" --json counterexample get \
    --artifact-hash "$counterexample_package_hash" \
    >"$output_dir/counterexample-package-after-proof.json"
  cmp --silent \
    "$prior_package_read" \
    "$output_dir/counterexample-package-after-proof.json" || {
    printf 'repaired proof changed the original counterexample package or repair edge\n' >&2
    exit 71
  }

  "$mcl_bin" --root "$state_root" --json claim get \
    --object-id "$original_claim_object_id" \
    --version-hash "$original_claim_version_hash" \
    | jq -cS . | tr -d '\n' >"$output_dir/original-claim-version-after-proof.json"
  cmp --silent \
    "$state_root/closure/claim-version.json" \
    "$output_dir/original-claim-version-after-proof.json" || {
    printf 'repaired proof changed the original immutable claim version\n' >&2
    exit 71
  }

  jq -e \
    --slurpfile original "$prior_original_status" \
    --slurpfile authority "$output_dir/publication-authority.json" \
    --slurpfile fidelity "$output_dir/fidelity-review.json" '
    .status == "proved" and
    (.witnesses | length) == 1 and
    .witnesses[0].kind == "proof" and
    .witnesses[0].authority_evidence_id == $authority[0].evidence.evidence_id and
    .witnesses[0].fidelity_evidence_id == $fidelity[0].evidence.evidence_id and
    .witnesses[0].authority_evidence_id != .witnesses[0].fidelity_evidence_id and
    .witnesses[0].authority_evidence_hash != .witnesses[0].fidelity_evidence_hash and
    .witnesses[0].authority_evidence_id != $original[0].witnesses[0].authority_evidence_id and
    .witnesses[0].fidelity_evidence_id != $original[0].witnesses[0].fidelity_evidence_id and
    $authority[0].evidence.created_by == "publication-boundary" and
    $fidelity[0].evidence.created_by == "protected-fidelity-reviewer"
  ' "$output_dir/claim-status-after-fidelity.json" >/dev/null || {
    printf 'repaired proof inherited evidence or violated role separation\n' >&2
    exit 71
  }
fi

jq -cS . "$output_dir/claim-status-after-fidelity.json"
