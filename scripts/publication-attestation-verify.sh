#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  printf 'usage: %s <report> <attestation-bundle> <output-directory>\n' "$0" >&2
  exit 64
fi

report="$1"
bundle="$2"
output_dir="$3"
repository="Mnehmos/MathOS"
workflow="Mnehmos/MathOS/.github/workflows/publication.yml"
source_ref="refs/heads/main"
predicate_type="https://slsa.dev/provenance/v1"
certificate_identity="https://github.com/Mnehmos/MathOS/.github/workflows/publication.yml@refs/heads/main"
verifier_version="2.96.0"
verifier_binary_hash="56b8bbbb27b066ecb33dbef9a256dc9d1314adaeff0908a752feba6c34053b40"
source_digest="${PUBLICATION_SOURCE_DIGEST:-${GITHUB_SHA:-}}"
gh_bin="${PUBLICATION_GH_BIN:-gh}"

if [[ ! -f "$report" || ! -f "$bundle" ]]; then
  printf 'publication report and attestation bundle must both exist\n' >&2
  exit 66
fi
if [[ ! "$source_digest" =~ ^[0-9a-f]{40}$ ]]; then
  printf 'publication source digest must be one exact lowercase Git commit SHA\n' >&2
  exit 65
fi
if ! command -v "$gh_bin" >/dev/null 2>&1; then
  printf 'pinned GitHub attestation verifier is unavailable: %s\n' "$gh_bin" >&2
  exit 69
fi
if [[ "$("$gh_bin" --version | sed -n '1s/^gh version \([^ ]*\).*/\1/p')" != "$verifier_version" ]]; then
  printf 'GitHub attestation verifier version does not match policy\n' >&2
  exit 65
fi
if [[ "$(sha256sum "$(command -v "$gh_bin")" | cut -d ' ' -f 1)" != "$verifier_binary_hash" ]]; then
  printf 'GitHub attestation verifier binary does not match policy\n' >&2
  exit 65
fi

mkdir -p "$output_dir"
raw_verification="${output_dir}/attestation-verification-raw.json"
verification_record="${output_dir}/attestation-verification.json"

"$gh_bin" attestation verify "$report" \
  --repo "$repository" \
  --bundle "$bundle" \
  --signer-workflow "$workflow" \
  --cert-identity "$certificate_identity" \
  --source-ref "$source_ref" \
  --source-digest "$source_digest" \
  --signer-digest "$source_digest" \
  --predicate-type "$predicate_type" \
  --deny-self-hosted-runners \
  --format json >"$raw_verification"

report_hash="$(sha256sum "$report" | cut -d ' ' -f 1)"
report_content_hash="$(jq -cS . "$report" | tr -d '\n' | sha256sum | cut -d ' ' -f 1)"
bundle_hash="$(sha256sum "$bundle" | cut -d ' ' -f 1)"
raw_hash="$(sha256sum "$raw_verification" | cut -d ' ' -f 1)"

jq -e --arg report_hash "$report_hash" --arg predicate_type "$predicate_type" '
  type == "array" and length > 0 and
  all(.[].verificationResult.statement.predicateType == $predicate_type) and
  any(.[].verificationResult.statement.subject[]?; .digest.sha256 == $report_hash) and
  all(.[].verificationResult.verifiedTimestamps | type == "array" and length > 0)
' "$raw_verification" >/dev/null

attestation_count="$(jq 'length' "$raw_verification")"
timestamp_count="$(jq '[.[].verificationResult.verifiedTimestamps | length] | add' "$raw_verification")"

jq -n \
  --arg schema_version "publication_attestation_verification/1" \
  --arg report_content_hash "$report_content_hash" \
  --arg report_hash "$report_hash" \
  --arg bundle_hash "$bundle_hash" \
  --arg raw_hash "$raw_hash" \
  --arg verifier_version "$verifier_version" \
  --arg verifier_binary_hash "$verifier_binary_hash" \
  --arg repository "$repository" \
  --arg workflow "$workflow" \
  --arg certificate_identity "$certificate_identity" \
  --arg source_ref "$source_ref" \
  --arg source_digest "$source_digest" \
  --arg predicate_type "$predicate_type" \
  --argjson attestation_count "$attestation_count" \
  --argjson timestamp_count "$timestamp_count" \
  '{schema_version:$schema_version,report_content_hash:$report_content_hash,report_artifact_hash:$report_hash,attestation_bundle_hash:$bundle_hash,raw_verification_hash:$raw_hash,verifier_name:"gh",verifier_version:$verifier_version,verifier_binary_sha256:$verifier_binary_hash,repository:$repository,signer_workflow:$workflow,certificate_identity:$certificate_identity,source_ref:$source_ref,source_commit_sha:$source_digest,predicate_type:$predicate_type,self_hosted_runners_denied:true,verified_attestation_count:$attestation_count,verified_timestamp_count:$timestamp_count,authoritative:false}' \
  >"$verification_record"

jq -e '
  .schema_version == "publication_attestation_verification/1" and
  .self_hosted_runners_denied == true and
  .verified_attestation_count > 0 and
  .verified_timestamp_count > 0 and
  .authoritative == false
' "$verification_record" >/dev/null
