#!/usr/bin/env bash
set -euo pipefail

readonly EXIT_USAGE=64
readonly EXIT_CONTEXT=65
readonly EXIT_INPUT=66
readonly EXIT_CONTROL=69
readonly EXIT_EXECUTION=70
readonly EXIT_VALIDATION=71
readonly MEMORY_LIMIT_BYTES=6442450944
readonly LEAN_MEMORY_LIMIT_MEGABYTES=4096
readonly MAX_OUTPUT_BYTES=1048576
readonly MAX_SAFE_JSON_INTEGER=9007199254740991
readonly REPOSITORY="Mnehmos/MathOS"
readonly WORKFLOW_PATH=".github/workflows/publication.yml"
readonly SOURCE_REF="refs/heads/main"
readonly REPORT_RUNNER_ENVIRONMENT="github_hosted"
readonly AUDIT_POLICY="policies/lean-local-audit-1.json"
readonly BH_FORMALIZATION_REPOSITORY="https://github.com/Mnehmos/BHFormalization.git"
readonly BH_FORMALIZATION_COMMIT="8875bef812375d7e6fd00bff95764cd5c057c7fe"
readonly BH_FORMALIZATION_TREE="600f3e1230ca88f9784837933e2417c319aaad3e"
readonly BH_FORMALIZATION_ARCHIVE_HASH="d0b9cd41dbf684c6ef20a61747b70b333596ba21f67e843e0dc7ad3c11e06ac4"
readonly BH_PROJECT_TREE="8688e0be616b809cc247a5d667e264d9a36db567"
readonly BH_PROJECT_ARCHIVE_HASH="512d6ea1ac29b7263f5b3e8d2893dc1feecd9e54f50b805f98922b72f628353f"
readonly BH_MODULE_HASH="12b0b9e31e85e6f82ee46db6edc846d0419b460476de817b39618ddaa7fb1302"
readonly BH_REPRODUCTION_REPOSITORY="https://github.com/dobriban/BH.git"
readonly BH_REPRODUCTION_COMMIT="bc23a801100e18d6c9ab278dd210c194fc1c5333"
readonly BH_REPRODUCTION_TREE="7c42242497ad067241b7038edf92362e3ea292ac"
readonly BH_REPRODUCTION_ARCHIVE_HASH="192f7845167c93075a1fb5b95bcd3b3c369ddf438718a968e5cb851b6f5bde18"
readonly BH_PAPER_URL="https://arxiv.org/pdf/2607.12208v1"
readonly BH_PAPER_HASH="de1db8081da57ddbb5b2af5574a35b90c1a23c0e27fe0d4f9f1dba4209cc45f7"

if [[ $# -lt 1 || $# -gt 3 ]]; then
  printf 'usage: %s <output-directory> [refutation|pilot-c|repaired-proof [state-directory]]\n' "$0" >&2
  exit "$EXIT_USAGE"
fi

readonly CANDIDATE_MODE="${2:-refutation}"
readonly REQUESTED_STATE_ROOT="${3:-}"
case "$CANDIDATE_MODE" in
  refutation)
    [[ $# -eq 1 ]] || {
      printf 'the refutation candidate creates its own state directory\n' >&2
      exit "$EXIT_USAGE"
    }
    readonly DECLARATION_NAME="MathOS.PilotA.every_prime_is_odd_refuted"
    readonly EXACT_THEOREM_TYPE="Not (∀ n : Nat, MathOS.PilotA.Prime n -> MathOS.PilotA.Odd n)"
    readonly PUBLICATION_OUTCOME="refutation"
    readonly CLAIM_POLARITY="negation"
    readonly FORMALIZATION_NOTES="Exact no-import formalization of the logical negation of the normalized Pilot A claim; the retained module exposes witness 2 separately."
    readonly FORMALIZATION_SEARCH_SUFFIX="witness 2"
    readonly CANDIDATE_ACTOR="publication-candidate"
    readonly CANDIDATE_KEY_PREFIX="publication-candidate"
    readonly LEAN_TOOLCHAIN="leanprover/lean4:v4.32.0"
    readonly TOOLCHAIN_VERSION_FRAGMENT="Lean (version 4.32.0,"
    readonly MODULE_FIXTURE="fixtures/publication/PilotARefutation.lean"
    readonly ENVIRONMENT_FIXTURE="fixtures/environment/lean-4.32-no-imports-local.json"
    readonly ENVIRONMENT_HASH_FIXTURE="fixtures/environment/lean-4.32-no-imports-local.sha256"
    readonly PUBLICATION_POLICY="policies/lean-publication-1.json"
    readonly PUBLICATION_POLICY_HASH="policies/lean-publication-1.sha256"
    readonly EXPECTED_AXIOMS_JSON="[]"
    readonly CREATES_STATE=true
    readonly PROJECT_MODE=false
    ;;
  pilot-c)
    [[ $# -eq 2 ]] || {
      printf 'the Pilot C candidate creates its own state directory\n' >&2
      exit "$EXIT_USAGE"
    }
    readonly DECLARATION_NAME="BH.Final.GaussianBHCounterexample"
    readonly EXACT_THEOREM_TYPE="∀ᶠ N in atTop, (13 / 1250 : ℝ) < gaussianBHFDR N"
    readonly PUBLICATION_OUTCOME="proof"
    readonly CLAIM_POLARITY="claim"
    readonly FORMALIZATION_NOTES="Exact headline theorem from Mnehmos/BHFormalization commit 8875bef812375d7e6fd00bff95764cd5c057c7fe. The model uses N+1 indexing; fidelity review must check the eventual shift. The generated Lean 5000-bin certificate remains distinct from the paper certificate."
    readonly FORMALIZATION_SEARCH_SUFFIX="Gaussian BH FDR eventual counterexample"
    readonly CANDIDATE_ACTOR="publication-pilot-c-candidate"
    readonly CANDIDATE_KEY_PREFIX="publication-pilot-c-candidate"
    readonly LEAN_TOOLCHAIN="leanprover/lean4:v4.32.0-rc1"
    readonly TOOLCHAIN_VERSION_FRAGMENT="Lean (version 4.32.0-rc1,"
    readonly MODULE_FIXTURE=""
    readonly ENVIRONMENT_FIXTURE="fixtures/environment/lean-4.32-rc1-bh-project-linux.json"
    readonly ENVIRONMENT_HASH_FIXTURE="fixtures/environment/lean-4.32-rc1-bh-project-linux.sha256"
    readonly PUBLICATION_POLICY="policies/lean-project-publication-1.json"
    readonly PUBLICATION_POLICY_HASH="policies/lean-project-publication-1.sha256"
    readonly EXPECTED_AXIOMS_JSON='["Classical.choice","Quot.sound","propext"]'
    readonly CREATES_STATE=true
    readonly PROJECT_MODE=true
    ;;
  repaired-proof)
    [[ $# -eq 3 && -n "$REQUESTED_STATE_ROOT" ]] || {
      printf 'the repaired proof candidate requires the existing canonical state directory\n' >&2
      exit "$EXIT_USAGE"
    }
    readonly DECLARATION_NAME="MathOS.PilotA.every_prime_other_than_two_is_odd"
    readonly EXACT_THEOREM_TYPE="∀ n : Nat, MathOS.PilotA.Prime n -> n ≠ 2 -> MathOS.PilotA.Odd n"
    readonly PUBLICATION_OUTCOME="proof"
    readonly CLAIM_POLARITY="claim"
    readonly FORMALIZATION_NOTES="Exact no-import formalization of the independently versioned repaired Pilot A claim; it excludes only the disproving boundary witness 2."
    readonly FORMALIZATION_SEARCH_SUFFIX="repaired claim excludes 2"
    readonly CANDIDATE_ACTOR="publication-repaired-proof-candidate"
    readonly CANDIDATE_KEY_PREFIX="publication-repaired-proof-candidate"
    readonly LEAN_TOOLCHAIN="leanprover/lean4:v4.32.0"
    readonly TOOLCHAIN_VERSION_FRAGMENT="Lean (version 4.32.0,"
    readonly MODULE_FIXTURE="fixtures/publication/PilotARefutation.lean"
    readonly ENVIRONMENT_FIXTURE="fixtures/environment/lean-4.32-no-imports-local.json"
    readonly ENVIRONMENT_HASH_FIXTURE="fixtures/environment/lean-4.32-no-imports-local.sha256"
    readonly PUBLICATION_POLICY="policies/lean-publication-1.json"
    readonly PUBLICATION_POLICY_HASH="policies/lean-publication-1.sha256"
    readonly EXPECTED_AXIOMS_JSON="[]"
    readonly CREATES_STATE=false
    readonly PROJECT_MODE=false
    ;;
  *)
    printf 'publication candidate mode must be refutation, pilot-c, or repaired-proof\n' >&2
    exit "$EXIT_USAGE"
    ;;
esac

die() {
  local exit_code="$1"
  shift
  printf '%s\n' "$*" >&2
  exit "$exit_code"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 \
    || die "$EXIT_CONTROL" "publication candidate control is missing: $1"
}

require_context_value() {
  local name="$1"
  [[ -n "${!name:-}" ]] \
    || die "$EXIT_CONTEXT" "publication candidate context is missing: $name"
}

decimal_at_most() {
  local value="$1"
  local maximum="$2"
  [[ "${#value}" -lt "${#maximum}" \
      || ( "${#value}" -eq "${#maximum}" \
        && ( "$value" == "$maximum" || "$value" < "$maximum" ) ) ]]
}

require_clean_checkout() {
  local checkout_status
  if ! checkout_status="$(git status --porcelain --untracked-files=all)"; then
    die "$EXIT_CONTEXT" "cannot establish clean-checkout status"
  fi
  [[ -z "$checkout_status" ]] \
    || die "$EXIT_CONTEXT" "publication candidate requires a clean checkout"
}

sha256_file() {
  sha256sum "$1" | cut -d ' ' -f 1
}

canonical_select() {
  local input="$1"
  local filter="$2"
  local output="$3"
  jq -cS "$filter" "$input" | tr -d '\n' >"$output"
  jq -e . "$output" >/dev/null \
    || die "$EXIT_VALIDATION" "failed to retain canonical JSON: $output"
}

assert_file_hash() {
  local file="$1"
  local expected="$2"
  local label="$3"
  local observed
  observed="$(sha256_file "$file")"
  [[ "$observed" == "$expected" ]] \
    || die "$EXIT_VALIDATION" "$label hash mismatch: expected $expected, observed $observed"
}

for name in \
  PUBLICATION_CONTEXT_MODE \
  PUBLICATION_REPOSITORY \
  PUBLICATION_WORKFLOW_PATH \
  PUBLICATION_WORKFLOW_REF \
  PUBLICATION_SOURCE_REF \
  PUBLICATION_SOURCE_COMMIT_SHA \
  PUBLICATION_SOURCE_TREE_SHA \
  PUBLICATION_WORKFLOW_RUN_ID \
  PUBLICATION_WORKFLOW_RUN_ATTEMPT \
  PUBLICATION_RUNNER_ENVIRONMENT; do
  require_context_value "$name"
done

[[ "$PUBLICATION_CONTEXT_MODE" == "protected-main" \
    || "$PUBLICATION_CONTEXT_MODE" == "simulated-main" ]] \
  || die "$EXIT_CONTEXT" "publication context mode must be protected-main or simulated-main"
[[ "$PUBLICATION_REPOSITORY" == "$REPOSITORY" ]] \
  || die "$EXIT_CONTEXT" "publication repository does not match policy"
[[ "$PUBLICATION_WORKFLOW_PATH" == "$WORKFLOW_PATH" ]] \
  || die "$EXIT_CONTEXT" "publication workflow path does not match policy"
[[ "$PUBLICATION_WORKFLOW_REF" == "$REPOSITORY/$WORKFLOW_PATH@$SOURCE_REF" ]] \
  || die "$EXIT_CONTEXT" "publication workflow ref does not bind the protected main workflow"
[[ "$PUBLICATION_SOURCE_REF" == "$SOURCE_REF" ]] \
  || die "$EXIT_CONTEXT" "publication source ref must be exactly refs/heads/main"
[[ "$PUBLICATION_RUNNER_ENVIRONMENT" == "$REPORT_RUNNER_ENVIRONMENT" ]] \
  || die "$EXIT_CONTEXT" "publication runner environment does not match policy"
[[ "$PUBLICATION_SOURCE_COMMIT_SHA" =~ ^[0-9a-f]{40}$ ]] \
  || die "$EXIT_CONTEXT" "publication source commit must be one lowercase 40-hex Git identity"
[[ "$PUBLICATION_SOURCE_TREE_SHA" =~ ^[0-9a-f]{40}$ ]] \
  || die "$EXIT_CONTEXT" "publication source tree must be one lowercase 40-hex Git identity"
if [[ ! "$PUBLICATION_WORKFLOW_RUN_ID" =~ ^[1-9][0-9]*$ ]] \
    || ! decimal_at_most "$PUBLICATION_WORKFLOW_RUN_ID" "$MAX_SAFE_JSON_INTEGER"; then
  die "$EXIT_CONTEXT" "publication workflow run ID is missing or outside the canonical JSON bound"
fi
if [[ ! "$PUBLICATION_WORKFLOW_RUN_ATTEMPT" =~ ^[1-9][0-9]*$ ]] \
    || ! decimal_at_most "$PUBLICATION_WORKFLOW_RUN_ATTEMPT" 4294967295; then
  die "$EXIT_CONTEXT" "publication workflow run attempt is invalid"
fi

if [[ "$PUBLICATION_CONTEXT_MODE" == "protected-main" ]]; then
  [[ "${GITHUB_ACTIONS:-}" == "true" \
      && "${GITHUB_REPOSITORY:-}" == "$REPOSITORY" \
      && "${GITHUB_REF:-}" == "$SOURCE_REF" \
      && "${GITHUB_SHA:-}" == "$PUBLICATION_SOURCE_COMMIT_SHA" \
      && "${GITHUB_WORKFLOW_REF:-}" == "$PUBLICATION_WORKFLOW_REF" \
      && "${GITHUB_RUN_ID:-}" == "$PUBLICATION_WORKFLOW_RUN_ID" \
      && "${GITHUB_RUN_ATTEMPT:-}" == "$PUBLICATION_WORKFLOW_RUN_ATTEMPT" \
      && "${GITHUB_REF_PROTECTED:-}" == "true" \
      && "${RUNNER_ENVIRONMENT:-}" == "github-hosted" \
      && ( "${GITHUB_EVENT_NAME:-}" == "push" \
        || "${GITHUB_EVENT_NAME:-}" == "workflow_dispatch" ) ]] \
    || die "$EXIT_CONTEXT" "protected publication context disagrees with GitHub's immutable run context or ref-protection state"
fi

for command_name in git jq sha256sum elan lean sudo mktemp cp grep tr cut sed sort stat find basename chmod mkdir dirname rm realpath id; do
  require_command "$command_name"
done
if [[ "$PROJECT_MODE" == true ]]; then
  for command_name in curl tar lake; do
    require_command "$command_name"
  done
fi
[[ -x /usr/bin/bwrap ]] \
  || die "$EXIT_CONTROL" "publication isolation control is missing: /usr/bin/bwrap"
[[ -x /usr/bin/chown ]] \
  || die "$EXIT_CONTROL" "publication workspace ownership control is missing: /usr/bin/chown"
[[ -x /usr/bin/prlimit ]] \
  || die "$EXIT_CONTROL" "publication resource control is missing: /usr/bin/prlimit"
[[ -x /usr/bin/timeout ]] \
  || die "$EXIT_CONTROL" "publication timeout control is missing: /usr/bin/timeout"
sudo -n true >/dev/null 2>&1 \
  || die "$EXIT_CONTROL" "publication candidate requires non-interactive sudo for Bubblewrap"

repo_root="$(cd "$(git rev-parse --show-toplevel)" && pwd -P)"
[[ "$(pwd -P)" == "$(cd "$repo_root" && pwd -P)" ]] \
  || die "$EXIT_CONTEXT" "publication candidate must run from the repository root"
[[ -x target/debug/mcl && ! -L target/debug/mcl \
    && "$(realpath -- target/debug/mcl)" == "$repo_root/target/debug/mcl" ]] \
  || die "$EXIT_CONTROL" "publication candidate requires the reviewed contained binary at target/debug/mcl"
require_clean_checkout
[[ "$(git rev-parse HEAD)" == "$PUBLICATION_SOURCE_COMMIT_SHA" ]] \
  || die "$EXIT_CONTEXT" "checked-out commit does not match publication context"
[[ "$(git rev-parse 'HEAD^{tree}')" == "$PUBLICATION_SOURCE_TREE_SHA" ]] \
  || die "$EXIT_CONTEXT" "checked-out tree does not match publication context"

required_files=(
  "$ENVIRONMENT_FIXTURE"
  "$ENVIRONMENT_HASH_FIXTURE"
  "$PUBLICATION_POLICY"
  "$PUBLICATION_POLICY_HASH"
  "$AUDIT_POLICY"
)
if [[ "$PROJECT_MODE" == false ]]; then
  required_files+=("$MODULE_FIXTURE" lean-toolchain)
fi
for required_file in "${required_files[@]}"; do
  [[ -f "$required_file" && ! -L "$required_file" \
      && "$(realpath -- "$required_file")" == "$repo_root/"* ]] \
    || die "$EXIT_INPUT" "publication candidate input is missing: $required_file"
done

state_root=""
if [[ "$CREATES_STATE" == false ]]; then
  [[ -d "$REQUESTED_STATE_ROOT" && ! -L "$REQUESTED_STATE_ROOT" ]] \
    || die "$EXIT_INPUT" "repaired proof state directory is unavailable or unsafe"
  state_root="$(cd "$REQUESTED_STATE_ROOT" && pwd -P)"
  case "$state_root/" in
    "$repo_root/"*)
      die "$EXIT_INPUT" "publication state must be outside the clean checkout"
      ;;
  esac
  [[ -f "$state_root/.mcl/state.sqlite3" && ! -L "$state_root/.mcl/state.sqlite3" ]] \
    || die "$EXIT_INPUT" "repaired proof state is not an initialized canonical instance"
fi

output_dir="$1"
output_name="$(basename -- "$output_dir")"
[[ "$output_name" != "." && "$output_name" != ".." && -n "$output_name" \
    && ! -e "$output_dir" && ! -L "$output_dir" ]] \
  || die "$EXIT_INPUT" "publication candidate output is invalid or already exists: $output_dir"
output_parent="$(dirname -- "$output_dir")"
if [[ "$CREATES_STATE" == false ]]; then
  [[ -d "$output_parent" && ! -L "$output_parent" ]] \
    || die "$EXIT_INPUT" "repaired proof output parent is unavailable or unsafe"
else
  mkdir -p "$output_parent"
fi
output_parent="$(cd "$output_parent" && pwd -P)"
output_dir="$output_parent/$output_name"
[[ ! -e "$output_dir" && ! -L "$output_dir" ]] \
  || die "$EXIT_INPUT" "publication candidate output appeared during validation: $output_dir"
case "$output_dir/" in
  "$repo_root/"*)
    die "$EXIT_INPUT" "publication candidate output must be outside the clean checkout"
    ;;
esac
if [[ "$CREATES_STATE" == true ]]; then
  state_root="$output_dir"
else
  case "$output_dir/" in
    "$state_root/"*) ;;
    *)
      die "$EXIT_INPUT" "repaired proof output must be strictly contained by the canonical state directory"
      ;;
  esac
fi
mkdir -- "$output_dir"
output_dir="$(cd "$output_dir" && pwd -P)"
mkdir "$output_dir/closure" "$output_dir/attempt"
closure_dir="$output_dir/closure"
attempt_dir="$output_dir/attempt"

temporary_root="$(mktemp -d "${RUNNER_TEMP:-/tmp}/mathos-publication-candidate.XXXXXX")"
cleanup() {
  local status=$?
  local attempt_file attempt_name attempt_size retained_cas_count=0
  trap - EXIT
  set +e
  for attempt_file in "$temporary_root"/*; do
    [[ -f "$attempt_file" ]] || continue
    attempt_name="$(basename "$attempt_file")"
    attempt_size="$(stat -c '%s' "$attempt_file" 2>/dev/null)"
    if [[ "$attempt_size" =~ ^[0-9]+$ && "$attempt_size" -le "$MAX_OUTPUT_BYTES" ]]; then
      cp -- "$attempt_file" "$attempt_dir/$attempt_name"
    else
      printf 'attempt file omitted because it exceeded the retained diagnostic bound\n' \
        >"$attempt_dir/$attempt_name.omitted"
    fi
  done
  if [[ -d "$state_root/.mcl/artifacts/sha256" ]]; then
    mkdir -p "$attempt_dir/cas"
    while IFS= read -r -d '' attempt_file; do
      [[ "$retained_cas_count" -lt 64 ]] || break
      attempt_name="$(basename "$attempt_file")"
      attempt_size="$(stat -c '%s' "$attempt_file" 2>/dev/null)"
      if [[ "$attempt_size" =~ ^[0-9]+$ && "$attempt_size" -le "$MAX_OUTPUT_BYTES" ]]; then
        cp -- "$attempt_file" "$attempt_dir/cas/$attempt_name"
        retained_cas_count=$((retained_cas_count + 1))
      fi
    done < <(find "$state_root/.mcl/artifacts/sha256" -type f -print0)
  fi
  jq -cS -n \
    --argjson exit_code "$status" \
    --arg context_mode "$PUBLICATION_CONTEXT_MODE" \
    --arg source_commit_sha "$PUBLICATION_SOURCE_COMMIT_SHA" \
    --arg source_tree_sha "$PUBLICATION_SOURCE_TREE_SHA" \
    --argjson retained_cas_count "$retained_cas_count" \
    '{schema_version:"publication_candidate_attempt/1",exit_code:$exit_code,context_mode:$context_mode,source_commit_sha:$source_commit_sha,source_tree_sha:$source_tree_sha,retained_cas_count:$retained_cas_count,authoritative:false}' \
    | tr -d '\n' >"$attempt_dir/attempt-summary.json"
  rm -rf -- "$temporary_root"
  exit "$status"
}
trap cleanup EXIT

readonly mcl_bin="$repo_root/target/debug/mcl"
run_mcl() {
  "$mcl_bin" --root "$state_root" --json "$@"
}

toolchain_file="lean-toolchain"
project_archive_hash=""
paper_artifact_file=""
formalization_source_artifact_file=""
reproduction_source_artifact_file=""
if [[ "$PROJECT_MODE" == true ]]; then
  formalization_checkout="$temporary_root/BHFormalization"
  reproduction_checkout="$temporary_root/BH"
  git init --quiet "$formalization_checkout"
  git -C "$formalization_checkout" remote add origin "$BH_FORMALIZATION_REPOSITORY"
  git -C "$formalization_checkout" fetch --quiet --depth=1 origin "$BH_FORMALIZATION_COMMIT" \
    >"$temporary_root/formalization-fetch.stdout" \
    2>"$temporary_root/formalization-fetch.stderr" \
    || die "$EXIT_INPUT" "failed to fetch the pinned BH formalization commit"
  git -C "$formalization_checkout" checkout --quiet --detach FETCH_HEAD
  [[ "$(git -C "$formalization_checkout" rev-parse HEAD)" == "$BH_FORMALIZATION_COMMIT" \
      && "$(git -C "$formalization_checkout" rev-parse 'HEAD^{tree}')" == "$BH_FORMALIZATION_TREE" \
      && "$(git -C "$formalization_checkout" rev-parse "$BH_FORMALIZATION_COMMIT:BHFormalization")" == "$BH_PROJECT_TREE" ]] \
    || die "$EXIT_VALIDATION" "BH formalization checkout identity changed"

  git init --quiet "$reproduction_checkout"
  git -C "$reproduction_checkout" remote add origin "$BH_REPRODUCTION_REPOSITORY"
  git -C "$reproduction_checkout" fetch --quiet --depth=1 origin "$BH_REPRODUCTION_COMMIT" \
    >"$temporary_root/reproduction-fetch.stdout" \
    2>"$temporary_root/reproduction-fetch.stderr" \
    || die "$EXIT_INPUT" "failed to fetch the pinned BH reproduction commit"
  git -C "$reproduction_checkout" checkout --quiet --detach FETCH_HEAD
  [[ "$(git -C "$reproduction_checkout" rev-parse HEAD)" == "$BH_REPRODUCTION_COMMIT" \
      && "$(git -C "$reproduction_checkout" rev-parse 'HEAD^{tree}')" == "$BH_REPRODUCTION_TREE" ]] \
    || die "$EXIT_VALIDATION" "BH reproduction checkout identity changed"

  formalization_source_artifact_file="$temporary_root/BHFormalization-8875bef.tar"
  reproduction_source_artifact_file="$temporary_root/dobriban-BH-bc23a80.tar"
  paper_artifact_file="$temporary_root/arxiv-2607.12208v1.pdf"
  git -C "$formalization_checkout" archive \
    --format=tar \
    --prefix=BHFormalization-8875bef/ \
    --output="$formalization_source_artifact_file" \
    "$BH_FORMALIZATION_COMMIT"
  git -C "$reproduction_checkout" archive \
    --format=tar \
    --prefix=dobriban-BH-bc23a80/ \
    --output="$reproduction_source_artifact_file" \
    "$BH_REPRODUCTION_COMMIT"
  git -C "$formalization_checkout" archive \
    --format=tar \
    --prefix=project/ \
    --output="$closure_dir/project.tar" \
    "$BH_FORMALIZATION_COMMIT:BHFormalization" \
    BH \
    BH.lean \
    Final.lean \
    lake-manifest.json \
    lakefile.lean \
    lean-toolchain
  curl --fail --location --proto '=https' --tlsv1.2 \
    --output "$paper_artifact_file" \
    "$BH_PAPER_URL" \
    >"$temporary_root/paper-fetch.stdout" \
    2>"$temporary_root/paper-fetch.stderr" \
    || die "$EXIT_INPUT" "failed to fetch the pinned BH paper"
  assert_file_hash "$formalization_source_artifact_file" \
    "$BH_FORMALIZATION_ARCHIVE_HASH" "BH formalization source archive"
  assert_file_hash "$reproduction_source_artifact_file" \
    "$BH_REPRODUCTION_ARCHIVE_HASH" "BH reproduction source archive"
  assert_file_hash "$paper_artifact_file" "$BH_PAPER_HASH" "BH paper"
  assert_file_hash "$closure_dir/project.tar" "$BH_PROJECT_ARCHIVE_HASH" \
    "BH project archive"
  project_archive_hash="$BH_PROJECT_ARCHIVE_HASH"
  project_root="$formalization_checkout/BHFormalization"
  toolchain_file="$project_root/lean-toolchain"
  cp -- "$project_root/Final.lean" "$closure_dir/module.lean"
  assert_file_hash "$closure_dir/module.lean" "$BH_MODULE_HASH" "BH headline module"
else
  cp -- "$MODULE_FIXTURE" "$closure_dir/module.lean"
fi

toolchain="$(tr -d '\r\n' <"$toolchain_file")"
[[ "$toolchain" == "$LEAN_TOOLCHAIN" ]] \
  || die "$EXIT_CONTEXT" "checked-out Lean toolchain does not match publication policy"
toolchain_config_hash="$(sha256_file "$toolchain_file")"
expected_toolchain_config_hash="$(jq -er '.project_configuration_hashes["lean-toolchain"]' "$ENVIRONMENT_FIXTURE")"
[[ "$toolchain_config_hash" == "$expected_toolchain_config_hash" ]] \
  || die "$EXIT_CONTEXT" "lean-toolchain bytes do not match the environment manifest"
lean_version="$(lean --version)"
[[ "$lean_version" == *"$TOOLCHAIN_VERSION_FRAGMENT"* ]] \
  || die "$EXIT_CONTEXT" "active Lean executable is not the pinned publication toolchain"
lean_path="$(elan which lean)"
[[ -x "$lean_path" ]] \
  || die "$EXIT_CONTROL" "elan did not resolve an executable Lean binary"
lean_root="$(dirname "$(dirname "$lean_path")")"

make_traversable_for_namespace_setup() {
  local current="$1"
  while [[ "$current" == "$HOME"/* ]]; do
    chmod o+x "$current"
    current="$(dirname "$current")"
  done
  chmod o+x "$HOME"
}

make_traversable_for_namespace_setup "$repo_root"
make_traversable_for_namespace_setup "$state_root"
make_traversable_for_namespace_setup "$output_dir"
make_traversable_for_namespace_setup "$temporary_root"
make_traversable_for_namespace_setup "$lean_root"

environment_json="$(jq -cS . "$ENVIRONMENT_FIXTURE")"
expected_environment_hash="$(tr -d '\r\n' <"$ENVIRONMENT_HASH_FIXTURE")"
[[ "$expected_environment_hash" =~ ^[0-9a-f]{64}$ ]] \
  || die "$EXIT_INPUT" "environment identity fixture is malformed"
[[ "$(printf '%s' "$environment_json" | sha256sum | cut -d ' ' -f 1)" == "$expected_environment_hash" ]] \
  || die "$EXIT_VALIDATION" "environment fixture canonical identity does not match its sidecar"
if [[ "$PROJECT_MODE" == true ]]; then
  lake_manifest_hash="$(sha256_file "$project_root/lake-manifest.json")"
  lakefile_hash="$(sha256_file "$project_root/lakefile.lean")"
  jq -e \
    --arg toolchain "$LEAN_TOOLCHAIN" \
    --arg toolchain_hash "$toolchain_config_hash" \
    --arg lake_manifest_hash "$lake_manifest_hash" \
    --arg lakefile_hash "$lakefile_hash" '
    .schema_version == "environment/1" and
    .formal_system == "lean4" and
    .lean_toolchain == $toolchain and
    (.dependencies | length) == 9 and
    .import_manifest == [
      "BH.Certificate.GeneratedData",
      "BH.Main.CertificateImpliesCounterexample"
    ] and
    .project_configuration_hashes == {
      "lake-manifest.json": $lake_manifest_hash,
      "lakefile.lean": $lakefile_hash,
      "lean-toolchain": $toolchain_hash
    } and
    .platform == "linux_x86_64" and
    .trust_profile == "publication" and
    .verifier_command == {
      "executable": "lake",
      "arguments": ["env", "lean", "{module_path}"]
    } and
    .dependency_preparation == "mathlib_cache_get" and
    .resource_limits.timeout_seconds == 21600 and
    .resource_limits.max_output_bytes == 16777216 and
    .resource_limits.max_memory_bytes == 12884901888 and
    .resource_limits.concurrency == 1 and
    .network_access == false and
    .working_directory_policy == "temporary_workspace"
  ' "$ENVIRONMENT_FIXTURE" >/dev/null \
    || die "$EXIT_VALIDATION" "publication evidence environment is not the exact BH project profile"
else
  jq -e --arg toolchain "$LEAN_TOOLCHAIN" --arg config_hash "$toolchain_config_hash" '
    .schema_version == "environment/1" and
    .formal_system == "lean4" and
    .lean_toolchain == $toolchain and
    .dependencies == [] and
    .import_manifest == [] and
    .project_configuration_hashes == {"lean-toolchain": $config_hash} and
    .platform == "linux_x86_64" and
    .trust_profile == "local" and
    .verifier_command == {"executable":"lean","arguments":["{module_path}"]} and
    .resource_limits.timeout_seconds == 120 and
    .resource_limits.max_output_bytes == 1048576 and
    .resource_limits.max_memory_bytes == null and
    .resource_limits.concurrency == 1 and
    .network_access == false and
    .working_directory_policy == "temporary_workspace"
  ' "$ENVIRONMENT_FIXTURE" >/dev/null \
    || die "$EXIT_VALIDATION" "publication evidence environment is not the exact no-import local profile"
fi

module_hash="$(sha256_file "$closure_dir/module.lean")"

while IFS= read -r forbidden_token; do
  if LC_ALL=C grep -Eq "(^|[^[:alnum:]_'])${forbidden_token}([^[:alnum:]_']|$)" "$closure_dir/module.lean"; then
    die "$EXIT_VALIDATION" "Lean module contains forbidden source token: $forbidden_token"
  fi
done < <(jq -er '.forbidden_source_tokens[]' "$AUDIT_POLICY")

if [[ "$CREATES_STATE" == true ]]; then
  run_mcl init \
    --actor "$CANDIDATE_ACTOR" \
    --idempotency-key "$CANDIDATE_KEY_PREFIX-init" \
    >"$temporary_root/init.json"

  run_mcl environment register \
    --manifest-json "$environment_json" \
    --actor "$CANDIDATE_ACTOR" \
    --idempotency-key "$CANDIDATE_KEY_PREFIX-environment" \
    >"$temporary_root/environment.json"
  environment_hash="$(jq -er '.proposed_environment_hash' "$temporary_root/environment.json")"
  [[ "$environment_hash" == "$expected_environment_hash" ]] \
    || die "$EXIT_VALIDATION" "registered environment identity changed"
  jq -e --arg hash "$environment_hash" --argjson manifest "$environment_json" '
    .dry_run == false and
    .environment.environment_hash == $hash and
    .environment.manifest == $manifest
  ' "$temporary_root/environment.json" >/dev/null \
    || die "$EXIT_VALIDATION" "registered environment snapshot is inconsistent"

  if [[ "$PROJECT_MODE" == true ]]; then
    paper_metadata="$(jq -cn '{
      schema_version: "artifact_metadata/1",
      media_type: "application/octet-stream",
      creation_source: "user_ingest",
      license_expression: "CC-BY-4.0",
      restriction: "public",
      semantic_metadata: {
        locator: "https://arxiv.org/pdf/2607.12208v1",
        source_role: "source_content",
        source_type: "paper",
        version: "v1"
      }
    }')"
    run_mcl artifact ingest \
      --input-file "$paper_artifact_file" \
      --metadata-json "$paper_metadata" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-paper-artifact" \
      >"$temporary_root/paper-artifact.json"
    [[ "$(jq -er '.proposed_artifact_hash' "$temporary_root/paper-artifact.json")" == "$BH_PAPER_HASH" ]] \
      || die "$EXIT_VALIDATION" "registered BH paper identity changed"

    formalization_source_metadata="$(jq -cn \
      --arg commit "$BH_FORMALIZATION_COMMIT" \
      --arg tree "$BH_FORMALIZATION_TREE" '{
      schema_version: "artifact_metadata/1",
      media_type: "application/octet-stream",
      creation_source: "user_ingest",
      license_expression: "Apache-2.0",
      restriction: "public",
      semantic_metadata: {
        commit: $commit,
        repository: "Mnehmos/BHFormalization",
        source_role: "source_content",
        source_type: "repository",
        tree: $tree
      }
    }')"
    run_mcl artifact ingest \
      --input-file "$formalization_source_artifact_file" \
      --metadata-json "$formalization_source_metadata" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-formalization-source-artifact" \
      >"$temporary_root/formalization-source-artifact.json"
    [[ "$(jq -er '.proposed_artifact_hash' "$temporary_root/formalization-source-artifact.json")" == "$BH_FORMALIZATION_ARCHIVE_HASH" ]] \
      || die "$EXIT_VALIDATION" "registered BH formalization source identity changed"

    reproduction_source_metadata="$(jq -cn \
      --arg commit "$BH_REPRODUCTION_COMMIT" \
      --arg tree "$BH_REPRODUCTION_TREE" '{
      schema_version: "artifact_metadata/1",
      media_type: "application/octet-stream",
      creation_source: "user_ingest",
      license_expression: null,
      restriction: "restricted",
      semantic_metadata: {
        commit: $commit,
        repository: "dobriban/BH",
        source_role: "source_content",
        source_type: "repository",
        tree: $tree
      }
    }')"
    run_mcl artifact ingest \
      --input-file "$reproduction_source_artifact_file" \
      --metadata-json "$reproduction_source_metadata" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-reproduction-source-artifact" \
      >"$temporary_root/reproduction-source-artifact.json"
    [[ "$(jq -er '.proposed_artifact_hash' "$temporary_root/reproduction-source-artifact.json")" == "$BH_REPRODUCTION_ARCHIVE_HASH" ]] \
      || die "$EXIT_VALIDATION" "registered BH reproduction source identity changed"

    project_metadata="$(jq -cn \
      --arg commit "$BH_FORMALIZATION_COMMIT" \
      --arg source_tree "$BH_PROJECT_TREE" '{
      schema_version: "artifact_metadata/1",
      media_type: "application/octet-stream",
      creation_source: "user_ingest",
      license_expression: "Apache-2.0",
      restriction: "public",
      semantic_metadata: {
        archive_root: "project",
        artifact_role: "lean_project_archive",
        commit: $commit,
        repository: "Mnehmos/BHFormalization",
        source_scope: "BH proof project without Challenge.lean",
        source_tree: $source_tree
      }
    }')"
    run_mcl artifact ingest \
      --input-file "$closure_dir/project.tar" \
      --metadata-json "$project_metadata" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-project-artifact" \
      >"$temporary_root/project.json"
    [[ "$(jq -er '.proposed_artifact_hash' "$temporary_root/project.json")" == "$project_archive_hash" ]] \
      || die "$EXIT_VALIDATION" "registered BH project identity changed"

    module_metadata="$(jq -cn \
      --arg commit "$BH_FORMALIZATION_COMMIT" \
      --arg declaration "$DECLARATION_NAME" \
      --arg project_hash "$project_archive_hash" '{
      schema_version: "artifact_metadata/1",
      media_type: "text/x-lean",
      creation_source: "user_ingest",
      license_expression: "Apache-2.0",
      restriction: "public",
      semantic_metadata: {
        commit: $commit,
        declaration_name: $declaration,
        project_archive_hash: $project_hash,
        repository: "Mnehmos/BHFormalization",
        source_role: "formalization_module"
      }
    }')"
    run_mcl artifact ingest \
      --input-file "$closure_dir/module.lean" \
      --metadata-json "$module_metadata" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-module" \
      >"$temporary_root/module.json"
    [[ "$(jq -er '.proposed_artifact_hash' "$temporary_root/module.json")" == "$module_hash" ]] \
      || die "$EXIT_VALIDATION" "registered BH headline module identity changed"

    paper_source_payload="$(jq -cn \
      --arg content_hash "$BH_PAPER_HASH" '{
      source_type: "paper",
      title_or_label: "The Benjamini-Hochberg Procedure Can Fail to Control the FDR for Correlated Two-Sided Gaussian Tests",
      authors_or_origin: ["Edgar Dobriban"],
      canonical_locator: "https://arxiv.org/abs/2607.12208v1",
      acquisition_date: "2026-07-25",
      license_expression: "CC-BY-4.0",
      redistribution_status: "allowed",
      content_hash: $content_hash,
      citation_metadata: {
        arxiv: "2607.12208v1",
        doi: "10.48550/arXiv.2607.12208",
        submitted: "2026-07-13"
      },
      redaction_class: "public",
      provenance_notes: "Exact arXiv v1 PDF downloaded from the canonical versioned locator; SHA-256 bound in CAS.",
      original_text: "Theorem 1: for the specified correlated two-sided Gaussian factor model at BH level 0.01, FDR_N is greater than 0.0104 for all sufficiently large N. The paper uses a complete outward-rounded interval-arithmetic certificate."
    }')"
    run_mcl source create \
      --payload-json "$paper_source_payload" \
      --searchable-text "Benjamini Hochberg correlated two-sided Gaussian FDR 0.0104" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-paper-source" \
      >"$temporary_root/source.json"
    source_object_id="$(jq -er '.record.object_id' "$temporary_root/source.json")"
    source_version_hash="$(jq -er '.record.version_hash' "$temporary_root/source.json")"

    formalization_source_payload="$(jq -cn \
      --arg commit "$BH_FORMALIZATION_COMMIT" \
      --arg tree "$BH_FORMALIZATION_TREE" \
      --arg content_hash "$BH_FORMALIZATION_ARCHIVE_HASH" '{
      source_type: "repository",
      title_or_label: ("Mnehmos/BHFormalization at " + $commit),
      authors_or_origin: [
        "Mnehmos/BHFormalization contributors",
        "Edgar Dobriban (formalized result)"
      ],
      canonical_locator: ("https://github.com/Mnehmos/BHFormalization/tree/" + $commit),
      acquisition_date: "2026-07-25",
      license_expression: "Apache-2.0",
      redistribution_status: "allowed",
      content_hash: $content_hash,
      citation_metadata: {
        commit: $commit,
        repository: "Mnehmos/BHFormalization",
        toolchain: "leanprover/lean4:v4.32.0-rc1",
        tree: $tree
      },
      redaction_class: "public",
      provenance_notes: "Deterministic git archive of the exact commit and tree; includes the independent Lean proof and generated 5000-bin certificate.",
      original_text: "The Lean theorem BH.Final.GaussianBHCounterexample proves eventually (13 / 1250 : Real) < gaussianBHFDR N. Generated certificate data is produced by generator/generate_certificate.py and rechecked by Lean kernel reduction."
    }')"
    run_mcl source create \
      --payload-json "$formalization_source_payload" \
      --searchable-text "Mnehmos BHFormalization Lean certificate GaussianBHCounterexample" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-formalization-source" \
      >"$temporary_root/formalization-source.json"
    formalization_source_object_id="$(jq -er '.record.object_id' "$temporary_root/formalization-source.json")"
    formalization_source_version_hash="$(jq -er '.record.version_hash' "$temporary_root/formalization-source.json")"

    reproduction_source_payload="$(jq -cn \
      --arg commit "$BH_REPRODUCTION_COMMIT" \
      --arg tree "$BH_REPRODUCTION_TREE" \
      --arg content_hash "$BH_REPRODUCTION_ARCHIVE_HASH" '{
      source_type: "repository",
      title_or_label: ("dobriban/BH reproducibility bundle at " + $commit),
      authors_or_origin: ["Edgar Dobriban"],
      canonical_locator: ("https://github.com/dobriban/BH/tree/" + $commit),
      acquisition_date: "2026-07-25",
      license_expression: null,
      redistribution_status: "restricted",
      content_hash: $content_hash,
      citation_metadata: {
        commit: $commit,
        repository: "dobriban/BH",
        tree: $tree
      },
      redaction_class: "restricted",
      provenance_notes: "Deterministic git archive of the exact upstream reproducibility repository. No repository license was detected, so redistribution remains restricted.",
      original_text: "The repository retains the paper certificate generator and output, experiment plan, seeded Monte Carlo outputs, environment lock, and reproducibility manifest."
    }')"
    run_mcl source create \
      --payload-json "$reproduction_source_payload" \
      --searchable-text "dobriban BH reproducibility certificate experiments" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-reproduction-source" \
      >"$temporary_root/reproduction-source.json"
    reproduction_source_object_id="$(jq -er '.record.object_id' "$temporary_root/reproduction-source.json")"
    reproduction_source_version_hash="$(jq -er '.record.version_hash' "$temporary_root/reproduction-source.json")"

    claim_payload="$(jq -cn \
      --arg source_object_id "$source_object_id" \
      --arg source_version_hash "$source_version_hash" \
      --arg formalization_source_object_id "$formalization_source_object_id" \
      --arg formalization_source_version_hash "$formalization_source_version_hash" \
      --arg reproduction_source_object_id "$reproduction_source_object_id" \
      --arg reproduction_source_version_hash "$reproduction_source_version_hash" '{
      source_reference: {
        object_id: $source_object_id,
        version_hash: $source_version_hash
      },
      normalized_informal_statement: "For the paper'\''s specified three-block correlated two-sided Gaussian factor model, the Benjamini-Hochberg procedure at nominal level 0.01 has FDR_N greater than 0.0104 for all sufficiently large integers N.",
      claim_kind: "universal",
      logical_shape: "Exists N0, for every integer N >= N0, FDR_N > 13/1250 > 1/100.",
      assumptions: [
        "The common factor and all idiosyncratic variables are mutually independent standard normal random variables.",
        "For each N >= 1 there are 100N hypotheses split into 96N true nulls, N first-signal coordinates, and 3N second-signal coordinates with the exact paper coefficients.",
        "Two-sided Gaussian p-values and the ordinary Benjamini-Hochberg step-up rule are used at alpha = 1/100."
      ],
      variables: [{
        symbol: "N",
        domain: "positive integers",
        notes: "Number of base blocks; the model contains 100N hypotheses."
      }],
      concept_links: [],
      source_citations: [
        {object_id: $source_object_id, version_hash: $source_version_hash},
        {
          object_id: $formalization_source_object_id,
          version_hash: $formalization_source_version_hash
        },
        {
          object_id: $reproduction_source_object_id,
          version_hash: $reproduction_source_version_hash
        }
      ],
      ambiguity_notes: [
        "The Lean development indexes its model by N+1 over natural N while the paper states N>=1; fidelity review must check the eventual statements are equivalent under this shift.",
        "The Lean 5000-bin certificate is independent of and stronger than the paper'\''s 1000-bin interval certificate; their provenance must remain separate.",
        "The paper source, reproducibility bundle, and Lean formalization have distinct licensing and authority boundaries."
      ]
    }')"
    run_mcl claim create \
      --payload-json "$claim_payload" \
      --searchable-text "Gaussian BH FDR 0.0104 eventual counterexample" \
      --actor "$CANDIDATE_ACTOR" \
      --idempotency-key "$CANDIDATE_KEY_PREFIX-claim" \
      >"$temporary_root/claim.json"
    claim_object_id="$(jq -er '.record.object_id' "$temporary_root/claim.json")"
    claim_version_hash="$(jq -er '.record.version_hash' "$temporary_root/claim.json")"
  else
  artifact_metadata="$(jq -cn \
    --arg declaration "$DECLARATION_NAME" \
    --arg repaired_declaration "MathOS.PilotA.every_prime_other_than_two_is_odd" \
    --arg commit "$PUBLICATION_SOURCE_COMMIT_SHA" \
    --arg tree "$PUBLICATION_SOURCE_TREE_SHA" \
    '{schema_version:"artifact_metadata/1",media_type:"text/x-lean",creation_source:"user_ingest",license_expression:null,restriction:"private",semantic_metadata:{artifact_role:"publication_candidate_module",source_role:"source_content",declaration_name:$declaration,repaired_declaration_name:$repaired_declaration,source_commit_sha:$commit,source_tree_sha:$tree}}')"
  run_mcl artifact ingest \
    --input-file "$closure_dir/module.lean" \
    --metadata-json "$artifact_metadata" \
    --actor "$CANDIDATE_ACTOR" \
    --idempotency-key "$CANDIDATE_KEY_PREFIX-module" \
    >"$temporary_root/module.json"
  [[ "$(jq -er '.proposed_artifact_hash' "$temporary_root/module.json")" == "$module_hash" ]] \
    || die "$EXIT_VALIDATION" "registered Lean module identity changed"

  acquisition_date="$(git show -s --format=%cs HEAD)"
  source_payload="$(jq -cn \
    --arg locator "git:$REPOSITORY@$PUBLICATION_SOURCE_COMMIT_SHA:$MODULE_FIXTURE" \
    --arg acquisition_date "$acquisition_date" \
    --arg content_hash "$module_hash" \
    '{source_type:"repository",title_or_label:"MathOS Pilot A protected refutation candidate",authors_or_origin:["MathOS protected publication workflow"],canonical_locator:$locator,acquisition_date:$acquisition_date,license_expression:null,redistribution_status:"restricted",content_hash:$content_hash,citation_metadata:{},redaction_class:"private",provenance_notes:"Fresh canonical state for the non-authoritative Pilot A protected refutation candidate.",original_text:"Every prime number is odd."}')"
  run_mcl source create \
    --payload-json "$source_payload" \
    --searchable-text "MathOS Pilot A every prime number is odd" \
    --actor "$CANDIDATE_ACTOR" \
    --idempotency-key "$CANDIDATE_KEY_PREFIX-source" \
    >"$temporary_root/source.json"
  source_object_id="$(jq -er '.record.object_id' "$temporary_root/source.json")"
  source_version_hash="$(jq -er '.record.version_hash' "$temporary_root/source.json")"

  claim_payload="$(jq -cn \
    --arg source_object_id "$source_object_id" \
    --arg source_version_hash "$source_version_hash" \
    '{source_reference:{object_id:$source_object_id,version_hash:$source_version_hash},normalized_informal_statement:"Every prime number is odd.",claim_kind:"universal",logical_shape:"forall n : Nat, Prime(n) -> Odd(n)",assumptions:[],variables:[{symbol:"n",domain:"natural numbers",notes:"Prime and odd use the exact predicates retained in the formalization module."}],concept_links:[],source_citations:[],ambiguity_notes:[]}')"
  run_mcl claim create \
    --payload-json "$claim_payload" \
    --searchable-text "Every prime number is odd" \
    --actor "$CANDIDATE_ACTOR" \
    --idempotency-key "$CANDIDATE_KEY_PREFIX-claim" \
    >"$temporary_root/claim.json"
  claim_object_id="$(jq -er '.record.object_id' "$temporary_root/claim.json")"
  claim_version_hash="$(jq -er '.record.version_hash' "$temporary_root/claim.json")"
  fi
else
  environment_hash="$expected_environment_hash"
  run_mcl environment get \
    --environment-hash "$environment_hash" \
    >"$temporary_root/environment-snapshot.json"
  jq -e --arg hash "$environment_hash" --argjson manifest "$environment_json" '
    .environment_hash == $hash and .manifest == $manifest
  ' "$temporary_root/environment-snapshot.json" >/dev/null \
    || die "$EXIT_VALIDATION" "repaired proof did not reuse the exact registered environment"
  jq -cS '{environment:.}' "$temporary_root/environment-snapshot.json" \
    >"$temporary_root/environment.json"

  run_mcl artifact get \
    --artifact-hash "$module_hash" \
    >"$temporary_root/module.json"
  jq -e \
    --arg hash "$module_hash" \
    --argjson size "$(stat -c '%s' "$closure_dir/module.lean")" \
    --arg declaration "$DECLARATION_NAME" '
    .artifact_hash == $hash and
    .byte_size == $size and
    .semantic_metadata.artifact_role == "publication_candidate_module" and
    .semantic_metadata.source_role == "source_content" and
    .semantic_metadata.repaired_declaration_name == $declaration
  ' "$temporary_root/module.json" >/dev/null \
    || die "$EXIT_VALIDATION" "repaired proof module is not the exact original registered artifact"

  repair_file="$state_root/refutation-ingestion/counterexample-repair.json"
  [[ -f "$repair_file" && ! -L "$repair_file" \
      && "$(realpath -- "$repair_file")" == "$state_root/"* ]] \
    || die "$EXIT_INPUT" "repaired proof requires the retained atomic repair in canonical state"
  claim_object_id="$(jq -er '.repair.repaired_claim.object_id' "$repair_file")"
  claim_version_hash="$(jq -er '.repair.repaired_claim.version_hash' "$repair_file")"
  source_object_id="$(jq -er '.repair.repaired_claim.payload.source_reference.object_id' "$repair_file")"
  source_version_hash="$(jq -er '.repair.repaired_claim.payload.source_reference.version_hash' "$repair_file")"

  run_mcl source get \
    --object-id "$source_object_id" \
    --version-hash "$source_version_hash" \
    >"$temporary_root/source-snapshot.json"
  run_mcl claim get \
    --object-id "$claim_object_id" \
    --version-hash "$claim_version_hash" \
    >"$temporary_root/claim-snapshot.json"
  jq -e \
    --arg claim_object_id "$claim_object_id" \
    --arg claim_version_hash "$claim_version_hash" \
    --arg source_object_id "$source_object_id" \
    --arg source_version_hash "$source_version_hash" '
    .object_id == $claim_object_id and
    .version_hash == $claim_version_hash and
    .predecessor_hash == null and
    .payload.source_reference == {object_id:$source_object_id,version_hash:$source_version_hash} and
    .payload.normalized_informal_statement == "Every prime number other than 2 is odd." and
    .payload.claim_kind == "universal" and
    .payload.logical_shape == "forall n : Nat, Prime(n) -> n != 2 -> Odd(n)" and
    .payload.assumptions == ["n != 2"]
  ' "$temporary_root/claim-snapshot.json" >/dev/null \
    || die "$EXIT_VALIDATION" "repaired proof claim does not match the exact atomic repair"
  jq -e --slurpfile claim "$temporary_root/claim-snapshot.json" '
    .repair.repaired_claim == $claim[0]
  ' "$repair_file" >/dev/null \
    || die "$EXIT_VALIDATION" "repaired claim bytes disagree with the retained atomic repair"
  jq -e \
    --arg source_object_id "$source_object_id" \
    --arg source_version_hash "$source_version_hash" '
    .object_id == $source_object_id and .version_hash == $source_version_hash
  ' "$temporary_root/source-snapshot.json" >/dev/null \
    || die "$EXIT_VALIDATION" "repaired proof source identity changed"
  jq -cS '{record:.}' "$temporary_root/source-snapshot.json" >"$temporary_root/source.json"
  jq -cS '{record:.}' "$temporary_root/claim-snapshot.json" >"$temporary_root/claim.json"

  run_mcl verify claim-status \
    --claim-object-id "$claim_object_id" \
    --claim-version-hash "$claim_version_hash" \
    >"$temporary_root/repaired-claim-status-before-candidate.json"
  jq -e --arg object_id "$claim_object_id" --arg version_hash "$claim_version_hash" '
    .claim == {object_id:$object_id,version_hash:$version_hash} and
    .status == "not_started" and
    (.witnesses | length) == 0 and
    (.nonqualifications | length) == 0
  ' "$temporary_root/repaired-claim-status-before-candidate.json" >/dev/null \
    || die "$EXIT_VALIDATION" "repaired claim inherited evidence before its proof candidate"
fi

declaration_hash="$(printf '%s' "$DECLARATION_NAME : $EXACT_THEOREM_TYPE" | sha256sum | cut -d ' ' -f 1)"
formalization_project_json="null"
formalization_imports_json="[]"
if [[ "$PROJECT_MODE" == true ]]; then
  formalization_project_json="$(jq -cn \
    --arg archive_artifact_hash "$project_archive_hash" '{
    archive_artifact_hash: $archive_artifact_hash,
    archive_root: "project",
    module_path: "Final.lean"
  }')"
  formalization_imports_json='[
    "BH.Certificate.GeneratedData",
    "BH.Main.CertificateImpliesCounterexample"
  ]'
fi
formalization_payload="$(jq -cn \
  --arg claim_object_id "$claim_object_id" \
  --arg claim_version_hash "$claim_version_hash" \
  --arg environment_hash "$environment_hash" \
  --arg module_hash "$module_hash" \
  --arg project_hash "$project_archive_hash" \
  --arg declaration "$DECLARATION_NAME" \
  --arg theorem_type "$EXACT_THEOREM_TYPE" \
  --arg declaration_hash "$declaration_hash" \
  --arg claim_polarity "$CLAIM_POLARITY" \
  --arg formalization_notes "$FORMALIZATION_NOTES" \
  --argjson import_manifest "$formalization_imports_json" \
  --argjson project "$formalization_project_json" '
  {
    claim_version: {
      object_id: $claim_object_id,
      version_hash: $claim_version_hash
    },
    formal_system: "lean4",
    claim_polarity: $claim_polarity,
    environment_hash: $environment_hash,
    module_artifact_hash: $module_hash,
    declaration_name: $declaration,
    exact_theorem_type: $theorem_type,
    declaration_hash: $declaration_hash,
    import_manifest: $import_manifest,
    formalization_notes: $formalization_notes,
    fidelity_evidence_references: [],
    verification_evidence_references: []
  } + if $project == null then {} else {project: $project} end
  ')"
run_mcl formalization create \
  --payload-json "$formalization_payload" \
  --searchable-text "$DECLARATION_NAME $EXACT_THEOREM_TYPE $FORMALIZATION_SEARCH_SUFFIX" \
  --actor "$CANDIDATE_ACTOR" \
  --idempotency-key "$CANDIDATE_KEY_PREFIX-formalization" \
  >"$temporary_root/formalization.json"
formalization_object_id="$(jq -er '.record.object_id' "$temporary_root/formalization.json")"
formalization_version_hash="$(jq -er '.record.version_hash' "$temporary_root/formalization.json")"

verifier_arguments=(
  verify check
  --environment-hash "$environment_hash"
  --module-artifact-hash "$module_hash"
  --declaration-name "$DECLARATION_NAME"
  --actor "$CANDIDATE_ACTOR"
  --idempotency-key "$CANDIDATE_KEY_PREFIX-verifier-job"
)
if [[ "$PROJECT_MODE" == true ]]; then
  verifier_arguments+=(
    --project-archive-artifact-hash "$project_archive_hash"
    --project-archive-root project
    --project-module-path Final.lean
  )
fi
run_mcl "${verifier_arguments[@]}" >"$temporary_root/verifier-enqueue.json"
verifier_job_id="$(jq -er '.job.job_id' "$temporary_root/verifier-enqueue.json")"
run_mcl worker \
  --worker-id "$CANDIDATE_KEY_PREFIX-verifier-worker" \
  --lease-seconds 21700 \
  >"$temporary_root/verifier-work.json"
jq -e \
  --arg job_id "$verifier_job_id" \
  --arg environment_hash "$environment_hash" \
  --arg module_hash "$module_hash" \
  --arg declaration "$DECLARATION_NAME" \
  --arg project_hash "$project_archive_hash" \
  --argjson project_mode "$PROJECT_MODE" \
  --argjson expected_axioms "$EXPECTED_AXIOMS_JSON" '
  .job.job_id == $job_id and
  .job.state == "succeeded" and
  .report.job_id == $job_id and
  .report.environment_hash == $environment_hash and
  .report.module_artifact_hash == $module_hash and
  .report.declaration_name == $declaration and
  .report.classification == "elaborated" and
  .report.exit_code == 0 and
  .report.forbidden_source_token == null and
  (if $project_mode then
    .report.observed_axioms == $expected_axioms
  else
    (.report | has("observed_axioms") | not)
  end) and
  (if $project_mode then
    .report.project == {
      archive_artifact_hash: $project_hash,
      archive_root: "project",
      module_path: "Final.lean"
    }
  else
    (.report | has("project") | not)
  end) and
  .report.trust_profile == (if $project_mode then "publication" else "local" end) and
  .report.memory_limit_enforced == $project_mode and
  .report.network_isolation_enforced == $project_mode and
  .report.authoritative == false
' "$temporary_root/verifier-work.json" >/dev/null \
  || die "$EXIT_VALIDATION" "diagnostic verifier did not produce the exact accepted input"
run_mcl verify status --job-id "$verifier_job_id" >"$temporary_root/verifier-job.json"

run_mcl verify promote-diagnostic \
  --formalization-object-id "$formalization_object_id" \
  --formalization-version-hash "$formalization_version_hash" \
  --job-id "$verifier_job_id" \
  --actor "$CANDIDATE_ACTOR" \
  --idempotency-key "$CANDIDATE_KEY_PREFIX-diagnostic" \
  >"$temporary_root/diagnostic.json"
diagnostic_evidence_id="$(jq -er '.evidence.evidence_id' "$temporary_root/diagnostic.json")"
diagnostic_evidence_hash="$(jq -er '.evidence.evidence_hash' "$temporary_root/diagnostic.json")"
jq -e \
  --arg subject_id "$formalization_object_id" \
  --arg subject_hash "$formalization_version_hash" \
  --arg job_id "$verifier_job_id" \
  --arg environment_hash "$environment_hash" '
  .evidence.payload.subject == {object_id:$subject_id,version_hash:$subject_hash} and
  .evidence.payload.evidence_kind == "lean_elaboration" and
  .evidence.payload.result == "accepted" and
  .evidence.payload.authority_class == "diagnostic" and
  .evidence.payload.producing_job_id == $job_id and
  .evidence.payload.environment_hash == $environment_hash and
  .evidence.payload.stale == false
' "$temporary_root/diagnostic.json" >/dev/null \
  || die "$EXIT_VALIDATION" "diagnostic evidence is not the exact accepted local result"

run_mcl verify audit \
  --formalization-object-id "$formalization_object_id" \
  --formalization-version-hash "$formalization_version_hash" \
  --diagnostic-evidence-id "$diagnostic_evidence_id" \
  --actor "$CANDIDATE_ACTOR" \
  --idempotency-key "$CANDIDATE_KEY_PREFIX-audit-job" \
  >"$temporary_root/audit-enqueue.json"
audit_job_id="$(jq -er '.job.job_id' "$temporary_root/audit-enqueue.json")"
run_mcl worker \
  --job-kind audit \
  --worker-id "$CANDIDATE_KEY_PREFIX-audit-worker" \
  --lease-seconds 21700 \
  >"$temporary_root/audit-work.json"
jq -e \
  --arg job_id "$audit_job_id" \
  --arg subject_id "$formalization_object_id" \
  --arg subject_hash "$formalization_version_hash" \
  --arg environment_hash "$environment_hash" \
  --arg module_hash "$module_hash" \
  --arg declaration "$DECLARATION_NAME" \
  --arg project_hash "$project_archive_hash" \
  --argjson project_mode "$PROJECT_MODE" \
  --argjson expected_axioms "$EXPECTED_AXIOMS_JSON" '
  .job.job_id == $job_id and
  .job.state == "succeeded" and
  .report.job_id == $job_id and
  .report.subject == {object_id:$subject_id,version_hash:$subject_hash} and
  .report.environment_hash == $environment_hash and
  .report.module_artifact_hash == $module_hash and
  .report.declaration_name == $declaration and
  .report.classification == "passed" and
  .report.source_forbidden_token == null and
  .report.observed_axioms == $expected_axioms and
  .report.unexpected_axioms == [] and
  (if $project_mode then
    .report.project == {
      archive_artifact_hash: $project_hash,
      archive_root: "project",
      module_path: "Final.lean"
    }
  else
    (.report | has("project") | not)
  end) and
  .report.trust_profile == (if $project_mode then "publication" else "local" end) and
  .report.dependency_closure_complete == true and
  .report.memory_limit_enforced == $project_mode and
  .report.network_isolation_enforced == $project_mode and
  .report.authoritative == false
' "$temporary_root/audit-work.json" >/dev/null \
  || die "$EXIT_VALIDATION" "audit did not produce the exact accepted closure"
run_mcl verify audit-status --job-id "$audit_job_id" >"$temporary_root/audit-job.json"

run_mcl verify promote-audit \
  --formalization-object-id "$formalization_object_id" \
  --formalization-version-hash "$formalization_version_hash" \
  --job-id "$audit_job_id" \
  --actor "$CANDIDATE_ACTOR" \
  --idempotency-key "$CANDIDATE_KEY_PREFIX-audit-evidence" \
  >"$temporary_root/audit-evidence.json"
[[ "$(jq '[.evidence[] | select(.payload.evidence_kind == "proof_closure_scan")] | length' "$temporary_root/audit-evidence.json")" == "1" \
    && "$(jq '[.evidence[] | select(.payload.evidence_kind == "axiom_audit")] | length' "$temporary_root/audit-evidence.json")" == "1" ]] \
  || die "$EXIT_VALIDATION" "audit promotion did not create the exact evidence pair"
proof_closure_evidence_id="$(jq -er '.evidence[] | select(.payload.evidence_kind == "proof_closure_scan") | .evidence_id' "$temporary_root/audit-evidence.json")"
proof_closure_evidence_hash="$(jq -er '.evidence[] | select(.payload.evidence_kind == "proof_closure_scan") | .evidence_hash' "$temporary_root/audit-evidence.json")"
axiom_audit_evidence_id="$(jq -er '.evidence[] | select(.payload.evidence_kind == "axiom_audit") | .evidence_id' "$temporary_root/audit-evidence.json")"
axiom_audit_evidence_hash="$(jq -er '.evidence[] | select(.payload.evidence_kind == "axiom_audit") | .evidence_hash' "$temporary_root/audit-evidence.json")"
jq -e \
  --arg job_id "$audit_job_id" \
  --arg subject_id "$formalization_object_id" \
  --arg subject_hash "$formalization_version_hash" \
  --arg environment_hash "$environment_hash" '
  (.evidence | length) == 2 and
  all(.evidence[];
    .payload.subject == {object_id:$subject_id,version_hash:$subject_hash} and
    .payload.result == "accepted" and
    .payload.authority_class == "diagnostic" and
    .payload.producing_job_id == $job_id and
    .payload.environment_hash == $environment_hash and
    .payload.stale == false
  )
' "$temporary_root/audit-evidence.json" >/dev/null \
  || die "$EXIT_VALIDATION" "audit evidence pair is not bound to the terminal local audit"

run_mcl verify prepare-publication \
  --formalization-object-id "$formalization_object_id" \
  --formalization-version-hash "$formalization_version_hash" \
  --outcome "$PUBLICATION_OUTCOME" \
  --diagnostic-evidence-id "$diagnostic_evidence_id" \
  --proof-closure-evidence-id "$proof_closure_evidence_id" \
  --axiom-audit-evidence-id "$axiom_audit_evidence_id" \
  --source-commit-sha "$PUBLICATION_SOURCE_COMMIT_SHA" \
  --source-tree-sha "$PUBLICATION_SOURCE_TREE_SHA" \
  --actor "$CANDIDATE_ACTOR" \
  --idempotency-key "$CANDIDATE_KEY_PREFIX-request" \
  >"$temporary_root/publication-request.json"
request_hash="$(jq -er '.proposed_request_hash' "$temporary_root/publication-request.json")"
jq -e \
  --arg request_hash "$request_hash" \
  --arg subject_id "$formalization_object_id" \
  --arg subject_hash "$formalization_version_hash" \
  --arg diagnostic_id "$diagnostic_evidence_id" \
  --arg diagnostic_hash "$diagnostic_evidence_hash" \
  --arg proof_id "$proof_closure_evidence_id" \
  --arg proof_hash "$proof_closure_evidence_hash" \
  --arg axiom_id "$axiom_audit_evidence_id" \
  --arg axiom_hash "$axiom_audit_evidence_hash" \
  --arg environment_hash "$environment_hash" \
  --arg module_hash "$module_hash" \
  --arg declaration "$DECLARATION_NAME" \
  --arg policy_hash "$(tr -d '\r\n' <"$PUBLICATION_POLICY_HASH")" \
  --arg commit "$PUBLICATION_SOURCE_COMMIT_SHA" \
  --arg tree "$PUBLICATION_SOURCE_TREE_SHA" \
  --arg outcome "$PUBLICATION_OUTCOME" \
  --arg project_hash "$project_archive_hash" \
  --argjson project_mode "$PROJECT_MODE" '
  .dry_run == false and
  .proposed_artifact_hash == $request_hash and
  .artifact.artifact_hash == $request_hash and
  .request.schema_version == "publication_request/1" and
  .request.subject == {object_id:$subject_id,version_hash:$subject_hash} and
  .request.outcome == $outcome and
  .request.diagnostic_evidence_id == $diagnostic_id and
  .request.diagnostic_evidence_hash == $diagnostic_hash and
  .request.proof_closure_evidence_id == $proof_id and
  .request.proof_closure_evidence_hash == $proof_hash and
  .request.axiom_audit_evidence_id == $axiom_id and
  .request.axiom_audit_evidence_hash == $axiom_hash and
  .request.environment_hash == $environment_hash and
  .request.module_artifact_hash == $module_hash and
  (if $project_mode then
    .request.project == {
      archive_artifact_hash: $project_hash,
      archive_root: "project",
      module_path: "Final.lean"
    }
  else
    (.request | has("project") | not)
  end) and
  .request.declaration_name == $declaration and
  .request.policy_hash == $policy_hash and
  .request.source_commit_sha == $commit and
  .request.source_tree_sha == $tree and
  (.request | has("authoritative") | not)
' "$temporary_root/publication-request.json" >/dev/null \
  || die "$EXIT_VALIDATION" "canonical publication request did not preserve every exact binding"

cas_path() {
  local hash="$1"
  printf '%s/.mcl/artifacts/sha256/%s/%s/%s' "$state_root" "${hash:0:2}" "${hash:2:2}" "$hash"
}

copy_cas() {
  local hash="$1"
  local destination="$2"
  local source
  source="$(cas_path "$hash")"
  [[ -f "$source" ]] \
    || die "$EXIT_VALIDATION" "registered CAS object is missing: $hash"
  assert_file_hash "$source" "$hash" "registered CAS object"
  cp -- "$source" "$destination"
  assert_file_hash "$destination" "$hash" "retained CAS object"
}

run_protected_lean() {
  local label="$1"
  shift
  local input_directory="$1"
  local input_file="$2"
  local stdout_file="$3"
  local stderr_file="$4"
  shift 4
  local -a lean_arguments=("$@")
  local status classification
  # The unprivileged runner intentionally owns these bounded output files.
  # shellcheck disable=SC2024
  if sudo -n /usr/bin/bwrap \
    --unshare-all \
    --die-with-parent \
    --new-session \
    --cap-drop ALL \
    --clearenv \
    --setenv HOME /tmp \
    --setenv PATH /opt/bin:/usr/bin:/bin \
    --setenv LANG C.UTF-8 \
    --setenv LC_ALL C.UTF-8 \
    --ro-bind / / \
    --ro-bind "$input_directory" /mnt \
    --ro-bind "$lean_root" /opt \
    --proc /proc \
    --dev /dev \
    --tmpfs /tmp \
    --chdir /mnt \
    /usr/bin/timeout --signal=TERM --kill-after=5s 120s \
    /usr/bin/prlimit --as="$MEMORY_LIMIT_BYTES" --fsize="$MAX_OUTPUT_BYTES" -- \
    /opt/bin/lean -M "$LEAN_MEMORY_LIMIT_MEGABYTES" -j 1 "${lean_arguments[@]}" "$input_file" \
    >"$stdout_file" 2>"$stderr_file"; then
    status=0
  else
    status=$?
  fi
  case "$status" in
    0) classification=passed ;;
    124) classification=timeout ;;
    137) classification=resource_killed ;;
    153) classification=output_exhausted ;;
    1) classification=lean_rejected ;;
    *) classification=execution_failed ;;
  esac
  jq -cS -n \
    --arg label "$label" \
    --arg input_file "$input_file" \
    --arg classification "$classification" \
    --argjson exit_code "$status" \
    --arg stdout_hash "$(sha256_file "$stdout_file")" \
    --arg stderr_hash "$(sha256_file "$stderr_file")" \
    '{schema_version:"protected_lean_execution_attempt/1",label:$label,input_file:$input_file,classification:$classification,exit_code:$exit_code,stdout_hash:$stdout_hash,stderr_hash:$stderr_hash,authoritative:false}' \
    | tr -d '\n' >"$temporary_root/$label-execution.json"
  if [[ "$status" -ne 0 ]]; then
    sed -n '1,120p' "$stderr_file" >&2
    die "$EXIT_EXECUTION" "protected isolated Lean execution failed for $input_file ($classification)"
  fi
}

if [[ "$PROJECT_MODE" == true ]]; then
  # The project verifier report is the protected execution: the publication
  # environment requires one Bubblewrap-isolated, memory-bounded build and driver.
  # The project audit reuses those exact immutable diagnostic streams.
  :
else
  run_protected_lean \
    protected-rebuild \
    "$closure_dir" \
    module.lean \
    "$closure_dir/protected.stdout" \
    "$closure_dir/protected.stderr"

  run_protected_lean \
    protected-dependency \
    "$closure_dir" \
    module.lean \
    "$closure_dir/protected-dependency.stdout" \
    "$closure_dir/protected-dependency.stderr" \
    --deps
  [[ ! -s "$closure_dir/protected-dependency.stderr" ]] \
    || die "$EXIT_VALIDATION" "protected dependency discovery wrote unexpected stderr"
  [[ "$(LC_ALL=C sort -u "$closure_dir/protected-dependency.stdout")" == "/opt/lib/lean/Init.olean" ]] \
    || die "$EXIT_VALIDATION" "protected dependency discovery found an undeclared import"

  cp -- "$closure_dir/module.lean" "$temporary_root/ProtectedAudit.lean"
  printf '\n#print axioms %s\n' "$DECLARATION_NAME" >>"$temporary_root/ProtectedAudit.lean"
  run_protected_lean \
    protected-audit \
    "$temporary_root" \
    ProtectedAudit.lean \
    "$closure_dir/protected-audit.stdout" \
    "$closure_dir/protected-audit.stderr"

  no_axioms_marker="'$DECLARATION_NAME' does not depend on any axioms"
  no_axioms_count="$((
    $(grep -Foc "$no_axioms_marker" "$closure_dir/protected-audit.stdout" || true) +
    $(grep -Foc "$no_axioms_marker" "$closure_dir/protected-audit.stderr" || true)
  ))"
  [[ "$no_axioms_count" -eq 1 ]] \
    || die "$EXIT_VALIDATION" "protected axiom driver did not emit one exact no-axioms result"
  if grep -Fq "'$DECLARATION_NAME' depends on axioms:" \
    "$closure_dir/protected-audit.stdout" "$closure_dir/protected-audit.stderr"; then
    die "$EXIT_VALIDATION" "protected axiom driver observed an unexpected axiom surface"
  fi
fi

if ! verifier_report_hash="$(jq -er '.result_artifact_hash | strings' "$temporary_root/verifier-job.json")" \
    || [[ ! "$verifier_report_hash" =~ ^[0-9a-f]{64}$ ]]; then
  die "$EXIT_VALIDATION" "terminal verifier job has no canonical report artifact hash"
fi
if ! audit_report_hash="$(jq -er '.result_artifact_hash | strings' "$temporary_root/audit-job.json")" \
    || [[ ! "$audit_report_hash" =~ ^[0-9a-f]{64}$ ]]; then
  die "$EXIT_VALIDATION" "terminal audit job has no canonical report artifact hash"
fi
if ! audit_policy_hash="$(jq -er '.request.policy_hash | strings' "$temporary_root/audit-job.json")" \
    || [[ ! "$audit_policy_hash" =~ ^[0-9a-f]{64}$ ]]; then
  die "$EXIT_VALIDATION" "terminal audit job has no canonical policy hash"
fi

copy_cas "$request_hash" "$closure_dir/publication-request.json"
copy_cas "$verifier_report_hash" "$closure_dir/verifier-report.json"
copy_cas "$audit_report_hash" "$closure_dir/audit-report.json"

retain_optional_cas_log() {
  local source_json="$1"
  local filter="$2"
  local destination="$3"
  local hash
  hash="$(jq -r "$filter // empty" "$source_json")"
  if [[ -n "$hash" ]]; then
    copy_cas "$hash" "$destination"
  else
    : >"$destination"
  fi
}

retain_optional_cas_log "$closure_dir/verifier-report.json" '.stdout_artifact_hash' \
  "$closure_dir/verifier.stdout"
retain_optional_cas_log "$closure_dir/verifier-report.json" '.stderr_artifact_hash' \
  "$closure_dir/verifier.stderr"
retain_optional_cas_log "$closure_dir/audit-report.json" '.stdout_artifact_hash' \
  "$closure_dir/audit.stdout"
retain_optional_cas_log "$closure_dir/audit-report.json" '.stderr_artifact_hash' \
  "$closure_dir/audit.stderr"

canonical_select "$temporary_root/source.json" '.record' "$closure_dir/source-version.json"
canonical_select "$temporary_root/claim.json" '.record' "$closure_dir/claim-version.json"
canonical_select "$temporary_root/formalization.json" '.record' "$closure_dir/formalization-version.json"
canonical_select "$temporary_root/environment.json" '.environment.manifest' "$closure_dir/environment-manifest.json"
canonical_select "$PUBLICATION_POLICY" '.' "$closure_dir/publication-policy.json"
canonical_select "$AUDIT_POLICY" '.' "$closure_dir/audit-policy.json"
canonical_select "$temporary_root/diagnostic.json" '.evidence' "$closure_dir/diagnostic-evidence.json"
canonical_select "$temporary_root/audit-evidence.json" \
  '.evidence[] | select(.payload.evidence_kind == "proof_closure_scan")' \
  "$closure_dir/proof-closure-evidence.json"
canonical_select "$temporary_root/audit-evidence.json" \
  '.evidence[] | select(.payload.evidence_kind == "axiom_audit")' \
  "$closure_dir/axiom-audit-evidence.json"
canonical_select "$temporary_root/verifier-job.json" '.' "$closure_dir/verifier-job.json"
canonical_select "$temporary_root/audit-job.json" '.' "$closure_dir/audit-job.json"

assert_file_hash "$closure_dir/publication-request.json" "$request_hash" "publication request"
assert_file_hash "$closure_dir/module.lean" "$module_hash" "Lean module"
if [[ "$PROJECT_MODE" == true ]]; then
  assert_file_hash "$closure_dir/project.tar" "$project_archive_hash" "Lean project archive"
fi
assert_file_hash "$closure_dir/environment-manifest.json" "$environment_hash" "environment manifest"
publication_policy_hash="$(tr -d '\r\n' <"$PUBLICATION_POLICY_HASH")"
assert_file_hash "$closure_dir/publication-policy.json" "$publication_policy_hash" "publication policy"
assert_file_hash "$closure_dir/audit-policy.json" "$audit_policy_hash" "audit policy"

if ! verifier_job_input_hash="$(jq -er '.canonical_input_hash | strings' "$closure_dir/verifier-job.json")" \
    || [[ ! "$verifier_job_input_hash" =~ ^[0-9a-f]{64}$ ]]; then
  die "$EXIT_VALIDATION" "retained verifier job has no canonical input hash"
fi
if ! audit_job_input_hash="$(jq -er '.canonical_input_hash | strings' "$closure_dir/audit-job.json")" \
    || [[ ! "$audit_job_input_hash" =~ ^[0-9a-f]{64}$ ]]; then
  die "$EXIT_VALIDATION" "retained audit job has no canonical input hash"
fi

entries_file="$temporary_root/closure-entries.jsonl"
: >"$entries_file"
add_entry() {
  local role="$1"
  local path="$2"
  local identity_hash="$3"
  local artifact_hash
  artifact_hash="$(sha256_file "$output_dir/$path")"
  [[ "$identity_hash" =~ ^[0-9a-f]{64}$ ]] \
    || die "$EXIT_VALIDATION" "invalid semantic identity for retained role $role"
  jq -cn \
    --arg role "$role" \
    --arg path "$path" \
    --arg identity_hash "$identity_hash" \
    --arg artifact_hash "$artifact_hash" \
    '{role:$role,path:$path,identity_hash:$identity_hash,artifact_hash:$artifact_hash}' \
    >>"$entries_file"
}

add_entry audit_job closure/audit-job.json \
  "$audit_job_input_hash"
add_entry audit_policy closure/audit-policy.json "$audit_policy_hash"
add_entry audit_report closure/audit-report.json \
  "$(sha256_file "$closure_dir/audit-report.json")"
add_entry audit_stderr closure/audit.stderr \
  "$(sha256_file "$closure_dir/audit.stderr")"
add_entry audit_stdout closure/audit.stdout \
  "$(sha256_file "$closure_dir/audit.stdout")"
add_entry axiom_audit_evidence closure/axiom-audit-evidence.json "$axiom_audit_evidence_hash"
add_entry claim_version closure/claim-version.json "$claim_version_hash"
add_entry diagnostic_evidence closure/diagnostic-evidence.json "$diagnostic_evidence_hash"
add_entry environment_manifest closure/environment-manifest.json "$environment_hash"
add_entry formalization_version closure/formalization-version.json "$formalization_version_hash"
add_entry lean_module closure/module.lean "$module_hash"
if [[ "$PROJECT_MODE" == true ]]; then
add_entry lean_project_archive closure/project.tar "$project_archive_hash"
fi
add_entry proof_closure_evidence closure/proof-closure-evidence.json "$proof_closure_evidence_hash"
if [[ "$PROJECT_MODE" != true ]]; then
  add_entry protected_audit_stderr closure/protected-audit.stderr \
    "$(sha256_file "$closure_dir/protected-audit.stderr")"
  add_entry protected_audit_stdout closure/protected-audit.stdout \
    "$(sha256_file "$closure_dir/protected-audit.stdout")"
  add_entry protected_dependency_stderr closure/protected-dependency.stderr \
    "$(sha256_file "$closure_dir/protected-dependency.stderr")"
  add_entry protected_dependency_stdout closure/protected-dependency.stdout \
    "$(sha256_file "$closure_dir/protected-dependency.stdout")"
  add_entry protected_stderr closure/protected.stderr \
    "$(sha256_file "$closure_dir/protected.stderr")"
  add_entry protected_stdout closure/protected.stdout \
    "$(sha256_file "$closure_dir/protected.stdout")"
fi
add_entry publication_policy closure/publication-policy.json "$publication_policy_hash"
add_entry publication_request closure/publication-request.json "$request_hash"
add_entry source_version closure/source-version.json "$source_version_hash"
add_entry verifier_job closure/verifier-job.json \
  "$verifier_job_input_hash"
add_entry verifier_report closure/verifier-report.json \
  "$(sha256_file "$closure_dir/verifier-report.json")"
add_entry verifier_stderr closure/verifier.stderr \
  "$(sha256_file "$closure_dir/verifier.stderr")"
add_entry verifier_stdout closure/verifier.stdout \
  "$(sha256_file "$closure_dir/verifier.stdout")"

jq -cS -n \
  --arg subject_id "$formalization_object_id" \
  --arg subject_hash "$formalization_version_hash" \
  --arg request_hash "$request_hash" \
  --slurpfile artifacts "$entries_file" \
  '{schema_version:"publication_retained_closure/1",subject:{object_id:$subject_id,version_hash:$subject_hash},request_hash:$request_hash,artifacts:$artifacts}' \
  | tr -d '\n' >"$output_dir/publication-retained-closure.json"

expected_closure_roles="$(jq -cn --argjson project "$PROJECT_MODE" '
  [
    "audit_job",
    "audit_policy",
    "audit_report",
    "audit_stderr",
    "audit_stdout",
    "axiom_audit_evidence",
    "claim_version",
    "diagnostic_evidence",
    "environment_manifest",
    "formalization_version",
    "lean_module"
  ] +
  (if $project then ["lean_project_archive"] else [] end) +
  [
    "proof_closure_evidence"
  ] +
  (if $project then [] else [
    "protected_audit_stderr",
    "protected_audit_stdout",
    "protected_dependency_stderr",
    "protected_dependency_stdout",
    "protected_stderr",
    "protected_stdout"
  ] end) +
  [
    "publication_policy",
    "publication_request",
    "source_version",
    "verifier_job",
    "verifier_report",
    "verifier_stderr",
    "verifier_stdout"
  ]
')"
jq -e --argjson expected_roles "$expected_closure_roles" '
  .schema_version == "publication_retained_closure/1" and
  (.artifacts | map(.role)) == $expected_roles
' "$output_dir/publication-retained-closure.json" >/dev/null \
  || die "$EXIT_VALIDATION" "retained publication closure roles are incomplete or out of order"

while IFS=$'\t' read -r role relative_path expected_hash; do
  [[ -f "$output_dir/$relative_path" ]] \
    || die "$EXIT_VALIDATION" "retained closure member is missing: $role"
  assert_file_hash "$output_dir/$relative_path" "$expected_hash" "retained closure member $role"
done < <(jq -r '.artifacts[] | [.role,.path,.artifact_hash] | @tsv' \
  "$output_dir/publication-retained-closure.json")

closure_hash="$(sha256_file "$output_dir/publication-retained-closure.json")"
retained_hashes="$(jq -c --arg closure_hash "$closure_hash" \
  '[.artifacts[].artifact_hash, $closure_hash] | sort | unique' \
  "$output_dir/publication-retained-closure.json")"

require_clean_checkout

jq -cS -n \
  --slurpfile request "$closure_dir/publication-request.json" \
  --arg request_hash "$request_hash" \
  --argjson run_id "$PUBLICATION_WORKFLOW_RUN_ID" \
  --argjson run_attempt "$PUBLICATION_WORKFLOW_RUN_ATTEMPT" \
  --argjson retained_hashes "$retained_hashes" \
  --arg toolchain "$LEAN_TOOLCHAIN" \
  --argjson observed_axioms "$EXPECTED_AXIOMS_JSON" \
  '{schema_version:"publication_report/1",request_hash:$request_hash,request:$request[0],classification:"passed",repository:"Mnehmos/MathOS",workflow_path:".github/workflows/publication.yml",source_ref:"refs/heads/main",workflow_run_id:$run_id,workflow_run_attempt:$run_attempt,runner_environment:"github_hosted",observed_lean_toolchain:$toolchain,observed_axioms:$observed_axioms,retained_artifact_hashes:$retained_hashes,clean_checkout:true,dependency_closure_complete:true,network_isolation_enforced:true,memory_limit_enforced:true,authoritative:false}' \
  | tr -d '\n' >"$output_dir/publication-report.json"

jq -e \
  --arg request_hash "$request_hash" \
  --arg commit "$PUBLICATION_SOURCE_COMMIT_SHA" \
  --arg tree "$PUBLICATION_SOURCE_TREE_SHA" \
  --argjson run_id "$PUBLICATION_WORKFLOW_RUN_ID" \
  --argjson run_attempt "$PUBLICATION_WORKFLOW_RUN_ATTEMPT" \
  --argjson retained_hashes "$retained_hashes" \
  --arg toolchain "$LEAN_TOOLCHAIN" \
  --argjson observed_axioms "$EXPECTED_AXIOMS_JSON" '
  .schema_version == "publication_report/1" and
  .request_hash == $request_hash and
  .request.source_commit_sha == $commit and
  .request.source_tree_sha == $tree and
  .classification == "passed" and
  .repository == "Mnehmos/MathOS" and
  .workflow_path == ".github/workflows/publication.yml" and
  .source_ref == "refs/heads/main" and
  .workflow_run_id == $run_id and
  .workflow_run_attempt == $run_attempt and
  .runner_environment == "github_hosted" and
  .observed_lean_toolchain == $toolchain and
  .observed_axioms == $observed_axioms and
  .retained_artifact_hashes == $retained_hashes and
  .clean_checkout == true and
  .dependency_closure_complete == true and
  .network_isolation_enforced == true and
  .memory_limit_enforced == true and
  .authoritative == false
' "$output_dir/publication-report.json" >/dev/null \
  || die "$EXIT_VALIDATION" "publication report lost a required protected candidate binding"

run_mcl verify validate-publication-candidate \
  --report-file "$output_dir/publication-report.json" \
  --retained-closure-file "$output_dir/publication-retained-closure.json" \
  --retained-root "$output_dir" \
  >"$temporary_root/candidate-validation.json"
jq -e \
  --arg request_hash "$request_hash" \
  --arg report_hash "$(sha256_file "$output_dir/publication-report.json")" \
  --arg closure_hash "$closure_hash" '
  .request_hash == $request_hash and
  .report_content_hash == $report_hash and
  .report_artifact_hash == $report_hash and
  .retained_closure_hash == $closure_hash and
  .retained_closure_artifact_hash == $closure_hash and
  .authoritative == false
' "$temporary_root/candidate-validation.json" >/dev/null \
  || die "$EXIT_VALIDATION" "shared application service rejected the protected publication candidate"

jq -cS . "$temporary_root/candidate-validation.json"
