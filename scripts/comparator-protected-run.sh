#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 5 ]]; then
  printf 'usage: %s <boundary-artifact-root> <expected-package-verification-hash> <expected-release-manifest-hash> <expected-plan-hash> <output-root>\n' "$0" >&2
  exit 64
fi

boundary_root="$1"
expected_package_hash="$2"
expected_release_hash="$3"
expected_plan_hash="$4"
output_root="$5"
mcl_bin="${MCL_BIN:-target/debug/mcl}"
runner_script_source="$0"
network_probe_source="$(dirname -- "$0")/comparator-network-probe.py"
package_source="${boundary_root}/pilot-a-comparator-package"
release_source="${boundary_root}/pilot-a-portable-release"
plan_source="${boundary_root}/publication-candidate/release-build-evidence/comparator-package-plan.json"
bundle="${output_root}/bundle"
work="${output_root}/work"
tools_dir="${work}/tools"
comparator_source="${work}/comparator-source"
landrun_source="${work}/landrun-source"
harness="${work}/harness"
gate_dir="${work}/gate"

comparator_commit="68a064109f01c08f47c8edc9f51d6a2bbffaa188"
comparator_tree="0bb408593d6e5f625db53b3be16e3f1cc91a7524"
lean4export_commit="af5aa64bb914c3c2c781f378088dbd38acf4f804"
lean4export_tree="5058a7945d24656600ca05917e3c8c174485bcf5"
landrun_commit="5ed4a3db3a4ad930d577215c6b9abaa19df7f99f"
landrun_tree="890013a5099a92792cbacd2cfff91af3f13cec9c"
lean_toolchain="leanprover/lean4:v4.32.0"

hash_pattern='^[0-9a-f]{64}$'
commit_pattern='^[0-9a-f]{40}$'
[[ "$expected_package_hash" =~ $hash_pattern \
  && "$expected_release_hash" =~ $hash_pattern \
  && "$expected_plan_hash" =~ $hash_pattern ]] || {
  printf 'trusted Comparator input identities must be lowercase SHA-256 values\n' >&2
  exit 65
}
[[ "${GITHUB_REF_PROTECTED:-}" == "true" \
  && "${GITHUB_REPOSITORY:-}" == "Mnehmos/MathOS" \
  && "${GITHUB_REPOSITORY_ID:-}" == "1305399818" \
  && "${GITHUB_REF:-}" == "refs/heads/main" \
  && "${GITHUB_WORKFLOW_REF:-}" == "Mnehmos/MathOS/.github/workflows/publication.yml@refs/heads/main" \
  && "${GITHUB_JOB:-}" == "comparator" \
  && "${RUNNER_OS:-}" == "Linux" \
  && "${RUNNER_ARCH:-}" == "X64" \
  && "${COMPARATOR_RUNNER_ENVIRONMENT:-}" == "github-hosted" ]] || {
  printf 'official Comparator execution requires the protected GitHub-hosted main job\n' >&2
  exit 65
}
[[ "${GITHUB_SHA:-}" =~ $commit_pattern \
  && "${GITHUB_RUN_ID:-}" =~ ^[0-9]{1,32}$ \
  && "${GITHUB_RUN_ATTEMPT:-}" =~ ^[1-9][0-9]*$ ]] || {
  printf 'protected Comparator workflow identity is incomplete\n' >&2
  exit 65
}
[[ "$(id -u)" -ne 0 ]] || {
  printf 'official Comparator execution refuses root\n' >&2
  exit 65
}
[[ "$(go version)" == "go version go1.24.2 linux/amd64" ]] || {
  printf 'official Comparator execution requires the pinned Go 1.24.2 landrun builder\n' >&2
  exit 65
}
[[ -x "$mcl_bin" \
  && -f "$runner_script_source" && ! -L "$runner_script_source" \
  && -f "$network_probe_source" && ! -L "$network_probe_source" \
  && -d "$boundary_root" && ! -L "$boundary_root" \
  && -d "$package_source" && ! -L "$package_source" \
  && -d "$release_source" && ! -L "$release_source" \
  && -f "$plan_source" && ! -L "$plan_source" ]] || {
  printf 'protected Comparator inputs are incomplete or unsafe\n' >&2
  exit 66
}
[[ ! -e "$output_root" && ! -L "$output_root" ]] || {
  printf 'protected Comparator output root already exists\n' >&2
  exit 66
}
mkdir --parents "$bundle/package" "$tools_dir" "$harness" "$gate_dir"
cp -- "$runner_script_source" "$bundle/runner-script.sh"
cp -- "$network_probe_source" "$bundle/network-probe.py"

observed_package_hash="$(sha256sum "$package_source/verification.json" | cut -d ' ' -f 1)"
observed_release_hash="$(sha256sum "$release_source/manifest.json" | cut -d ' ' -f 1)"
observed_plan_hash="$(sha256sum "$plan_source" | cut -d ' ' -f 1)"
[[ "$observed_package_hash" == "$expected_package_hash" \
  && "$observed_release_hash" == "$expected_release_hash" \
  && "$observed_plan_hash" == "$expected_plan_hash" ]] || {
  printf 'downloaded boundary artifact differs from trusted same-run identities\n' >&2
  exit 70
}

nonexistent_root="${work}/nonexistent-offline-root"
"$mcl_bin" --root "$nonexistent_root" --json release verify-comparator-package \
  --package-dir "$package_source" \
  --expected-verification-hash "$expected_package_hash" \
  --plan "$plan_source" \
  --bundle-dir "$release_source" \
  --expected-release-manifest-hash "$expected_release_hash" \
  >"$bundle/package-reprojection.json"
[[ ! -e "$nonexistent_root" && ! -L "$nonexistent_root" ]] || {
  printf 'offline Comparator package reprojection touched the MathOS root\n' >&2
  exit 71
}
jq -e \
  --arg package_hash "$expected_package_hash" \
  --arg release_hash "$expected_release_hash" '
  .verification_hash == $package_hash and
  .source_release_manifest_hash == $release_hash and
  .status == "ready" and
  .comparator_verified == false and
  .authoritative == false and
  .member_count == 5 and
  .database_independent == true and
  .inventory_verified == true and
  .hashes_verified == true and
  .bindings_verified == true and
  .deterministic_reprojection == true
' "$bundle/package-reprojection.json" >/dev/null
for member in Challenge.lean Solution.lean config.json formalization.yaml verification.json; do
  cp -- "$package_source/$member" "$bundle/package/$member"
done
[[ "$(find "$bundle/package" -mindepth 1 -maxdepth 1 -type f | wc -l)" -eq 5 \
  && -z "$(find "$bundle/package" -mindepth 1 ! -type f -print -quit)" ]] || {
  printf 'retained Comparator package inventory is not exact\n' >&2
  exit 71
}

git clone --no-checkout https://github.com/leanprover/comparator "$comparator_source"
git -C "$comparator_source" checkout --detach "$comparator_commit"
[[ "$(git -C "$comparator_source" rev-parse HEAD)" == "$comparator_commit" \
  && "$(git -C "$comparator_source" rev-parse 'HEAD^{tree}')" == "$comparator_tree" ]] || {
  printf 'Comparator source identity mismatch\n' >&2
  exit 70
}
(
  cd "$comparator_source"
  ELAN_TOOLCHAIN="$lean_toolchain" lake build lean4export comparator
)
lean4export_source="${comparator_source}/.lake/packages/lean4export"
[[ "$(git -C "$lean4export_source" rev-parse HEAD)" == "$lean4export_commit" \
  && "$(git -C "$lean4export_source" rev-parse 'HEAD^{tree}')" == "$lean4export_tree" \
  && -z "$(git -C "$comparator_source" diff --name-only)" \
  && -z "$(git -C "$comparator_source" diff --cached --name-only)" \
  && -z "$(git -C "$lean4export_source" diff --name-only)" \
  && -z "$(git -C "$lean4export_source" diff --cached --name-only)" ]] || {
  printf 'lean4export source identity mismatch\n' >&2
  exit 70
}
install --mode=0555 "$comparator_source/.lake/build/bin/comparator" "$bundle/comparator.bin"
install --mode=0555 "$lean4export_source/.lake/build/bin/lean4export" "$bundle/lean4export.bin"

git clone --no-checkout https://github.com/Zouuup/landrun "$landrun_source"
git -C "$landrun_source" checkout --detach "$landrun_commit"
[[ "$(git -C "$landrun_source" rev-parse HEAD)" == "$landrun_commit" \
  && "$(git -C "$landrun_source" rev-parse 'HEAD^{tree}')" == "$landrun_tree" ]] || {
  printf 'landrun source identity mismatch\n' >&2
  exit 70
}
(
  cd "$landrun_source"
  go build -mod=readonly -trimpath -buildvcs=false -o "$bundle/landrun.bin" ./cmd/landrun
)
[[ -z "$(git -C "$landrun_source" diff --name-only)" \
  && -z "$(git -C "$landrun_source" diff --cached --name-only)" ]] || {
  printf 'landrun build changed tracked source\n' >&2
  exit 70
}
chmod 0555 "$bundle/landrun.bin"
ELAN_TOOLCHAIN="$lean_toolchain" lean --version | grep --fixed-strings 'Lean (version 4.32.0,' >/dev/null

strict_probe_raw="${work}/landlock-probe.raw.stdout"
if ! "$bundle/landrun.bin" \
  --log-level debug \
  --ro / \
  --rw /dev \
  --ldd \
  --add-exec \
  /usr/bin/true \
  >"$strict_probe_raw" \
  2>"$bundle/landlock-probe.stderr"; then
  printf 'strict Landlock V5 capability probe failed\n' >&2
  exit 70
fi
[[ ! -s "$strict_probe_raw" ]] || {
  printf 'strict Landlock probe produced unexpected stdout\n' >&2
  exit 71
}
grep --fixed-strings 'BestEffort:false' "$bundle/landlock-probe.stderr" >/dev/null
grep --fixed-strings 'Landlock restrictions applied successfully' "$bundle/landlock-probe.stderr" >/dev/null
if grep --fixed-strings 'BestEffort:true' "$bundle/landlock-probe.stderr" >/dev/null; then
  printf 'strict Landlock probe silently enabled best-effort mode\n' >&2
  exit 71
fi
printf '%s\n' 'MATHOS_LANDLOCK_STRICT_PROBE=passed' >"$bundle/landlock-probe.stdout"

printf '%s\n' \
  'name = "mathos_comparator_pilot_a"' \
  'version = "0.1.0"' \
  '' \
  '[[lean_lib]]' \
  'name = "Challenge"' \
  '' \
  '[[lean_lib]]' \
  'name = "Solution"' \
  >"$harness/lakefile.toml"
printf '%s\n' "$lean_toolchain" >"$harness/lean-toolchain"
(
  cd "$harness"
  lake update >"$work/lake-manifest-init.stdout" 2>"$work/lake-manifest-init.stderr"
)
[[ ! -e "$harness/.lake" && -z "$(find "$harness" -name '*.olean' -print -quit)" ]] || {
  printf 'Lake manifest initialization contaminated the fresh harness\n' >&2
  exit 71
}
cp -- "$harness/lake-manifest.json" "$bundle/lake-manifest.json"
cp -- "$harness/lakefile.toml" "$bundle/lakefile.toml"
cp -- "$harness/lean-toolchain" "$bundle/lean-toolchain"
cp -- "$package_source/Challenge.lean" "$harness/Challenge.lean"
cp -- "$package_source/Solution.lean" "$harness/Solution.lean"
cp -- "$package_source/config.json" "$harness/config.json"
[[ "$(find "$harness" -mindepth 1 -maxdepth 1 -type f | wc -l)" -eq 6 \
  && -z "$(find "$harness" -mindepth 1 -maxdepth 1 ! -type f -print -quit)" \
  && ! -e "$harness/.lake" \
  && -z "$(find "$harness" -name '*.olean' -print -quit)" ]] || {
  printf 'Comparator harness is not pristine before official execution\n' >&2
  exit 71
}

systemctl --user show-environment >/dev/null
unit="mathos-comparator-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"
ready="${gate_dir}/systemd-ready"
go="${gate_dir}/systemd-go"
listener_port_file="${gate_dir}/tcp-listener-port"
python3 "$bundle/network-probe.py" listen "$listener_port_file" \
  >"$work/tcp-listener.stdout" \
  2>"$work/tcp-listener.stderr" &
listener_pid="$!"
cleanup_listener() {
  if kill -0 "$listener_pid" 2>/dev/null; then
    kill "$listener_pid" 2>/dev/null || true
    wait "$listener_pid" 2>/dev/null || true
  fi
}
trap cleanup_listener EXIT
for _ in $(seq 1 200); do
  [[ -s "$listener_port_file" ]] && break
  kill -0 "$listener_pid" 2>/dev/null || {
    printf 'local TCP challenge listener exited before publishing its port\n' >&2
    exit 70
  }
  sleep 0.05
done
[[ -s "$listener_port_file" ]] || {
  printf 'local TCP challenge listener did not publish its port\n' >&2
  exit 70
}
listener_port="$(tr -d '\n' <"$listener_port_file")"
[[ "$listener_port" =~ ^[0-9]+$ \
  && "$listener_port" -ge 1 \
  && "$listener_port" -le 65535 ]] || {
  printf 'local TCP challenge listener published an invalid port\n' >&2
  exit 71
}
# Positional parameters expand in the gated child shell, not this parent shell.
# shellcheck disable=SC2016
/usr/bin/timeout --signal=TERM --kill-after=5s 300 \
  /usr/bin/systemd-run \
  --user \
  --quiet \
  --wait \
  --pipe \
  --unit="$unit" \
  --property=RestrictAddressFamilies=~AF_UNIX \
  --property=NoNewPrivileges=yes \
  --working-directory="$harness" \
  --setenv="PATH=${PATH}" \
  --setenv="HOME=${HOME}" \
  --setenv="COMPARATOR_LANDRUN=${bundle}/landrun.bin" \
  --setenv="COMPARATOR_LEAN4EXPORT=${bundle}/lean4export.bin" \
  -- \
  bash -c 'set -euo pipefail; touch "$2"; while [[ ! -e "$3" ]]; do sleep 0.05; done; python3 "$4" unix; "$5" --best-effort --ro / --rw /dev -ldd -add-exec /usr/bin/python3 "$4" tcp "$6"; printf "MATHOS_COMPARATOR_UID=%s\n" "$(id -u)"; exec lake env "$1" config.json' \
  bash "$bundle/comparator.bin" "$ready" "$go" "$bundle/network-probe.py" "$bundle/landrun.bin" "$listener_port" \
  >"$bundle/comparator.stdout" \
  2>"$bundle/comparator.stderr" &
systemd_pid="$!"
cleanup_comparator_unit() {
  touch "$go" 2>/dev/null || true
  systemctl --user stop "${unit}.service" >/dev/null 2>&1 || true
  if kill -0 "$systemd_pid" 2>/dev/null; then
    kill "$systemd_pid" 2>/dev/null || true
    wait "$systemd_pid" 2>/dev/null || true
  fi
  cleanup_listener
}
trap cleanup_comparator_unit EXIT
for _ in $(seq 1 300); do
  [[ -e "$ready" ]] && break
  if ! kill -0 "$systemd_pid" 2>/dev/null; then
    set +e
    wait "$systemd_pid"
    early_exit="$?"
    set -e
    printf 'systemd Comparator unit exited before its live controls were inspected: %s\n' "$early_exit" >&2
    exit 70
  fi
  sleep 0.05
done
[[ -e "$ready" ]] || {
  printf 'systemd Comparator unit did not reach its inspection gate\n' >&2
  exit 70
}
systemctl --user show "${unit}.service" \
  --property=User \
  --property=NoNewPrivileges \
  --property=RestrictAddressFamilies \
  >"$bundle/systemd.properties"
grep --fixed-strings --line-regexp 'User=' "$bundle/systemd.properties" >/dev/null
grep --fixed-strings --line-regexp 'NoNewPrivileges=yes' "$bundle/systemd.properties" >/dev/null
grep --fixed-strings --line-regexp 'RestrictAddressFamilies=~AF_UNIX' "$bundle/systemd.properties" >/dev/null
[[ "$(wc -l <"$bundle/systemd.properties")" -eq 3 \
  && "$(find "$harness" -mindepth 1 -maxdepth 1 -type f | wc -l)" -eq 6 \
  && ! -e "$harness/.lake" \
  && -z "$(find "$harness" -name '*.olean' -print -quit)" ]] || {
  printf 'Comparator harness changed before the protected execution gate opened\n' >&2
  exit 71
}
touch "$go"
set +e
wait "$systemd_pid"
comparator_exit="$?"
set -e
trap - EXIT
cleanup_listener
timed_out=false
if [[ "$comparator_exit" -eq 124 ]]; then
  timed_out=true
  systemctl --user stop "${unit}.service" >/dev/null 2>&1 || true
fi

markers_json="$(jq -Rs '
  [split("\n")[] |
    select(
      . == "Building Challenge" or
      (startswith("Exporting #[") and endswith("] from Challenge")) or
      . == "Building Solution" or
      (startswith("Exporting #[") and endswith("] from Solution")) or
      . == "Running Lean default kernel on solution." or
      . == "Lean default kernel accepts the solution" or
      . == "Your solution is okay!"
    )]
' "$bundle/comparator.stdout")"
markers_ok=false
if jq -e '
  length == 7 and
  .[0] == "Building Challenge" and
  (.[1] | startswith("Exporting #[") and endswith("] from Challenge")) and
  .[2] == "Building Solution" and
  (.[3] | startswith("Exporting #[") and endswith("] from Solution")) and
  ((.[1] | sub(" from Challenge$"; "")) == (.[3] | sub(" from Solution$"; ""))) and
  .[4] == "Running Lean default kernel on solution." and
  .[5] == "Lean default kernel accepts the solution" and
  .[6] == "Your solution is okay!"
' <<<"$markers_json" >/dev/null; then
  markers_ok=true
fi
uid_ok=false
if [[ "$(grep --count --extended-regexp '^MATHOS_COMPARATOR_UID=[0-9]+$' "$bundle/comparator.stdout" || true)" -eq 1 \
  && "$(grep --extended-regexp '^MATHOS_COMPARATOR_UID=[0-9]+$' "$bundle/comparator.stdout" | cut -d= -f2)" -eq "$(id -u)" ]]; then
  uid_ok=true
fi
tcp_ok=false
unix_ok=false
network_ok=false
if [[ "$(grep --count --fixed-strings --line-regexp 'MATHOS_LANDRUN_TCP_DENIED=passed' "$bundle/comparator.stdout" || true)" -eq 1 ]]; then
  tcp_ok=true
fi
if [[ "$(grep --count --fixed-strings --line-regexp 'MATHOS_SYSTEMD_AF_UNIX_DENIED=passed' "$bundle/comparator.stdout" || true)" -eq 1 ]]; then
  unix_ok=true
fi
if [[ "$tcp_ok" == "true" && "$unix_ok" == "true" ]]; then
  network_ok=true
fi
output_bounds=false
if [[ "$(stat --format=%s "$bundle/comparator.stdout")" -le 262144 \
  && "$(stat --format=%s "$bundle/comparator.stderr")" -le 65536 ]]; then
  output_bounds=true
fi
stderr_empty=false
[[ ! -s "$bundle/comparator.stderr" ]] && stderr_empty=true
classification="rejected"
comparator_verified=false
if [[ "$timed_out" == "true" ]]; then
  classification="failed"
elif [[ "$comparator_exit" -eq 0 \
  && "$markers_ok" == "true" \
  && "$uid_ok" == "true" \
  && "$network_ok" == "true" \
  && "$output_bounds" == "true" \
  && "$stderr_empty" == "true" ]]; then
  classification="accepted"
  comparator_verified=true
fi

binding() {
  local logical_path="$1"
  local physical_path="$2"
  jq -cnS \
    --arg path "$logical_path" \
    --arg content_hash "$(sha256sum "$physical_path" | cut -d ' ' -f 1)" \
    --argjson byte_size "$(stat --format=%s "$physical_path")" \
    '{path:$path,content_hash:$content_hash,byte_size:$byte_size}'
}

package_members="$(jq -cn \
  --argjson challenge "$(binding package/Challenge.lean "$bundle/package/Challenge.lean")" \
  --argjson solution "$(binding package/Solution.lean "$bundle/package/Solution.lean")" \
  --argjson config "$(binding package/config.json "$bundle/package/config.json")" \
  --argjson formalization "$(binding package/formalization.yaml "$bundle/package/formalization.yaml")" \
  --argjson verification "$(binding package/verification.json "$bundle/package/verification.json")" \
  '[$challenge,$solution,$config,$formalization,$verification]')"
harness_files="$(jq -cn \
  --argjson manifest "$(binding lake-manifest.json "$bundle/lake-manifest.json")" \
  --argjson lakefile "$(binding lakefile.toml "$bundle/lakefile.toml")" \
  --argjson toolchain "$(binding lean-toolchain "$bundle/lean-toolchain")" \
  '[$manifest,$lakefile,$toolchain]')"
source_tree="$(git rev-parse 'HEAD^{tree}')"
systemd_version="$(systemd --version | sed -n '1p')"
runner_image="${ImageOS:-ubuntu}"
go_version="$(go version)"

jq -cnS \
  --arg classification "$classification" \
  --argjson comparator_verified "$comparator_verified" \
  --arg package_hash "$expected_package_hash" \
  --arg input_fingerprint "$(jq -er '.input_fingerprint' "$package_source/verification.json")" \
  --arg plan_hash "$expected_plan_hash" \
  --arg release_hash "$expected_release_hash" \
  --arg formalization_object_id "$(jq -er '.source_formalization.object_id' "$package_source/verification.json")" \
  --arg formalization_version_hash "$(jq -er '.source_formalization.version_hash' "$package_source/verification.json")" \
  --arg declaration "$(jq -er '.declaration_name' "$package_source/verification.json")" \
  --argjson package_members "$package_members" \
  --arg source_commit "${GITHUB_SHA}" \
  --arg source_tree "$source_tree" \
  --arg run_id "${GITHUB_RUN_ID}" \
  --argjson run_attempt "${GITHUB_RUN_ATTEMPT}" \
  --arg runner_image "$runner_image" \
  --arg kernel_release "$(uname -r)" \
  --arg systemd_version "$systemd_version" \
  --arg go_version "$go_version" \
  --argjson runner_uid "$(id -u)" \
  --argjson comparator_binary "$(binding comparator.bin "$bundle/comparator.bin")" \
  --argjson lean4export_binary "$(binding lean4export.bin "$bundle/lean4export.bin")" \
  --argjson landrun_binary "$(binding landrun.bin "$bundle/landrun.bin")" \
  --argjson harness_files "$harness_files" \
  --argjson exit_code "$comparator_exit" \
  --argjson timed_out "$timed_out" \
  --argjson stdout "$(binding comparator.stdout "$bundle/comparator.stdout")" \
  --argjson stderr "$(binding comparator.stderr "$bundle/comparator.stderr")" \
  --argjson systemd_properties "$(binding systemd.properties "$bundle/systemd.properties")" \
  --argjson landlock_stdout "$(binding landlock-probe.stdout "$bundle/landlock-probe.stdout")" \
  --argjson landlock_stderr "$(binding landlock-probe.stderr "$bundle/landlock-probe.stderr")" \
  --argjson reprojection "$(binding package-reprojection.json "$bundle/package-reprojection.json")" \
  --argjson runner_script "$(binding runner-script.sh "$bundle/runner-script.sh")" \
  --argjson network_probe "$(binding network-probe.py "$bundle/network-probe.py")" \
  --argjson markers "$markers_json" \
  --argjson output_bounds "$output_bounds" \
  --argjson stderr_empty "$stderr_empty" \
  --argjson markers_ok "$markers_ok" \
  --argjson uid_ok "$uid_ok" \
  --argjson tcp_ok "$tcp_ok" \
  --argjson unix_ok "$unix_ok" \
  --argjson network_ok "$network_ok" '
  {
    schema_version:"comparator_run_report/1",
    classification:$classification,
    comparator_verified:$comparator_verified,
    authoritative:false,
    attestation_required:true,
    package:{
      verification_hash:$package_hash,
      input_fingerprint:$input_fingerprint,
      plan_hash:$plan_hash,
      source_release_manifest_hash:$release_hash,
      source_formalization:{object_id:$formalization_object_id,version_hash:$formalization_version_hash},
      declaration_name:$declaration,
      lean_toolchain:"leanprover/lean4:v4.32.0",
      members:$package_members
    },
    workflow:{
      repository:"Mnehmos/MathOS",
      repository_id:"1305399818",
      workflow_path:".github/workflows/publication.yml",
      workflow_ref:"Mnehmos/MathOS/.github/workflows/publication.yml@refs/heads/main",
      source_ref:"refs/heads/main",
      source_commit_sha:$source_commit,
      source_tree_sha:$source_tree,
      run_id:$run_id,
      run_attempt:$run_attempt,
      job:"comparator",
      protected_ref:true,
      github_hosted:true,
      runner_os:"Linux",
      runner_arch:"X64",
      runner_image:$runner_image,
      kernel_release:$kernel_release,
      systemd_version:$systemd_version,
      runner_uid:$runner_uid
    },
    tools:[
      {name:"comparator",repository:"https://github.com/leanprover/comparator",commit:"68a064109f01c08f47c8edc9f51d6a2bbffaa188",source_tree:"0bb408593d6e5f625db53b3be16e3f1cc91a7524",build_toolchain:"leanprover/lean4:v4.32.0",binary:$comparator_binary},
      {name:"lean4export",repository:"https://github.com/leanprover/lean4export",commit:"af5aa64bb914c3c2c781f378088dbd38acf4f804",source_tree:"5058a7945d24656600ca05917e3c8c174485bcf5",build_toolchain:"leanprover/lean4:v4.32.0",binary:$lean4export_binary},
      {name:"landrun",repository:"https://github.com/Zouuup/landrun",commit:"5ed4a3db3a4ad930d577215c6b9abaa19df7f99f",source_tree:"890013a5099a92792cbacd2cfff91af3f13cec9c",build_toolchain:$go_version,binary:$landrun_binary}
    ],
    harness:{
      project_name:"mathos_comparator_pilot_a",
      challenge_module:"Challenge",
      solution_module:"Solution",
      files:$harness_files,
      manifest_created_before_solution_copy:true,
      source_file_count:3,
      no_lake_directory_before_run:true,
      no_olean_before_run:true
    },
    sandbox:{
      real_landrun:true,
      fake_landrun:false,
      landlock_abi:5,
      strict_probe_without_best_effort:true,
      comparator_best_effort_after_strict_probe:true,
      systemd_user_manager:true,
      restrict_address_families:"~AF_UNIX",
      no_new_privileges:true,
      non_root:true,
      tcp_network_denied:$tcp_ok,
      unix_socket_denied:$unix_ok,
      network_isolated:$network_ok
    },
    execution:{
      command_profile:"official_comparator_systemd_landrun_v1",
      exit_code:$exit_code,
      timed_out:$timed_out,
      stdout:$stdout,
      stderr:$stderr,
      systemd_properties:$systemd_properties,
      landlock_probe_stdout:$landlock_stdout,
      landlock_probe_stderr:$landlock_stderr,
      package_reprojection:$reprojection,
      runner_script:$runner_script,
      network_probe:$network_probe,
      success_markers:$markers
    },
    predicates:{
      package_reprojected:true,
      tool_sources_and_binaries_verified:true,
      fresh_harness_verified:true,
      landlock_strict_probe_passed:true,
      systemd_controls_verified:true,
      network_isolation_verified:$network_ok,
      non_root_verified:$uid_ok,
      output_bounds_verified:$output_bounds,
      unexpected_stderr_absent:$stderr_empty,
      success_markers_ordered_unique:$markers_ok,
      statement_match_verified:$markers_ok,
      axioms_verified:$markers_ok,
      lean_kernel_verified:$markers_ok
    }
  }' >"$bundle/report.json"
truncate --size=-1 "$bundle/report.json"

report_hash="$(sha256sum "$bundle/report.json" | cut -d ' ' -f 1)"
"$mcl_bin" --root "$nonexistent_root" --json release verify-comparator-run \
  --run-dir "$bundle" \
  --expected-report-hash "$report_hash" \
  --expected-package-verification-hash "$expected_package_hash" \
  >"$output_root/offline-verification.json"
[[ ! -e "$nonexistent_root" && ! -L "$nonexistent_root" ]] || {
  printf 'offline Comparator run verification touched the MathOS root\n' >&2
  exit 71
}
jq -e \
  --arg report_hash "$report_hash" \
  --arg package_hash "$expected_package_hash" '
  .report_hash == $report_hash and
  .package_verification_hash == $package_hash and
  .classification == "accepted" and
  .comparator_verified == true and
  .authoritative == false and
  .database_independent == true and
  .inventory_verified == true and
  .hashes_verified == true and
  .package_bindings_verified == true and
  .tool_bindings_verified == true and
  .harness_verified == true and
  .sandbox_verified == true and
  .official_success_path_verified == true
' "$output_root/offline-verification.json" >/dev/null
printf '%s\n' "$report_hash" >"$output_root/report.sha256"
printf 'Protected official Comparator report: %s\n' "$report_hash"
