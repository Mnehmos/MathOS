#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf 'usage: %s <output-directory>\n' "$0" >&2
  exit 64
fi

output_dir="$1"
module="fixtures/publication/Smoke.lean"

test -f "$module"
test -x /usr/bin/bwrap
test -x /usr/bin/prlimit
test -z "$(git status --porcelain)"
mkdir -p "$output_dir"

commit_sha="$(git rev-parse HEAD)"
tree_sha="$(git rev-parse HEAD^{tree})"
toolchain="$(tr -d '\r\n' < lean-toolchain)"
lean_path="$(command -v lean)"

sudo /usr/bin/bwrap \
  --unshare-all \
  --die-with-parent \
  --new-session \
  --ro-bind / / \
  --proc /proc \
  --dev /dev \
  --tmpfs /tmp \
  --chdir "$PWD" \
  /usr/bin/prlimit --as=1073741824 -- "$lean_path" "$module" \
  >"${output_dir}/lean.stdout" \
  2>"${output_dir}/lean.stderr"

stdout_hash="$(sha256sum "${output_dir}/lean.stdout" | cut -d ' ' -f 1)"
stderr_hash="$(sha256sum "${output_dir}/lean.stderr" | cut -d ' ' -f 1)"
module_hash="$(sha256sum "$module" | cut -d ' ' -f 1)"

jq -n \
  --arg schema_version "publication_boundary_smoke/1" \
  --arg commit_sha "$commit_sha" \
  --arg tree_sha "$tree_sha" \
  --arg source_ref "${GITHUB_REF:-local}" \
  --arg toolchain "$toolchain" \
  --arg module_hash "$module_hash" \
  --arg stdout_hash "$stdout_hash" \
  --arg stderr_hash "$stderr_hash" \
  '{schema_version:$schema_version,source_commit_sha:$commit_sha,source_tree_sha:$tree_sha,source_ref:$source_ref,lean_toolchain:$toolchain,module_artifact_hash:$module_hash,stdout_artifact_hash:$stdout_hash,stderr_artifact_hash:$stderr_hash,runner_environment:"github_hosted",clean_checkout:true,network_isolation_enforced:true,memory_limit_enforced:true,authoritative:false}' \
  >"${output_dir}/publication-smoke-report.json"

jq -e '
  .schema_version == "publication_boundary_smoke/1" and
  .runner_environment == "github_hosted" and
  .clean_checkout == true and
  .network_isolation_enforced == true and
  .memory_limit_enforced == true and
  .authoritative == false
' "${output_dir}/publication-smoke-report.json" >/dev/null
