#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf 'usage: %s <output-directory>\n' "$0" >&2
  exit 64
fi

output_dir="$1"
module="fixtures/publication/Smoke.lean"

if [[ ! -f "$module" ]]; then
  printf 'publication module is missing: %s\n' "$module" >&2
  exit 66
fi
if [[ ! -x /usr/bin/bwrap ]]; then
  printf 'publication isolation control is missing: /usr/bin/bwrap\n' >&2
  exit 69
fi
if [[ ! -x /usr/bin/prlimit ]]; then
  printf 'publication resource control is missing: /usr/bin/prlimit\n' >&2
  exit 69
fi
if [[ -n "$(git status --porcelain)" ]]; then
  printf 'publication boundary requires a clean checkout\n' >&2
  exit 65
fi
mkdir -p "$output_dir"

commit_sha="$(git rev-parse HEAD)"
tree_sha="$(git rev-parse HEAD^{tree})"
toolchain="$(tr -d '\r\n' < lean-toolchain)"
lean_path="$(elan which lean)"
lean_root="$(dirname "$(dirname "$lean_path")")"
bwrap_version="$(/usr/bin/bwrap --version)"

make_traversable_for_namespace_setup() {
  local current="$1"
  while [[ "$current" == "$HOME"/* ]]; do
    chmod o+x "$current"
    current="$(dirname "$current")"
  done
  chmod o+x "$HOME"
}

make_traversable_for_namespace_setup "$PWD"
make_traversable_for_namespace_setup "$lean_root"

if ! sudo /usr/bin/bwrap \
  --unshare-all \
  --die-with-parent \
  --new-session \
  --cap-drop ALL \
  --ro-bind / / \
  --ro-bind "$PWD" /mnt \
  --ro-bind "$lean_root" /opt \
  --proc /proc \
  --dev /dev \
  --tmpfs /tmp \
  --chdir /mnt \
  /usr/bin/prlimit --as=1073741824 -- /opt/bin/lean "$module" \
  >"${output_dir}/lean.stdout" \
  2>"${output_dir}/lean.stderr"; then
  printf 'isolated Lean execution failed\n' >&2
  sed -n '1,120p' "${output_dir}/lean.stderr" >&2
  exit 70
fi

stdout_hash="$(sha256sum "${output_dir}/lean.stdout" | cut -d ' ' -f 1)"
stderr_hash="$(sha256sum "${output_dir}/lean.stderr" | cut -d ' ' -f 1)"
module_hash="$(sha256sum "$module" | cut -d ' ' -f 1)"

jq -n \
  --arg schema_version "publication_boundary_smoke/1" \
  --arg commit_sha "$commit_sha" \
  --arg tree_sha "$tree_sha" \
  --arg source_ref "${GITHUB_REF:-local}" \
  --arg toolchain "$toolchain" \
  --arg bwrap_version "$bwrap_version" \
  --arg module_hash "$module_hash" \
  --arg stdout_hash "$stdout_hash" \
  --arg stderr_hash "$stderr_hash" \
  '{schema_version:$schema_version,source_commit_sha:$commit_sha,source_tree_sha:$tree_sha,source_ref:$source_ref,lean_toolchain:$toolchain,isolation_control:$bwrap_version,module_artifact_hash:$module_hash,stdout_artifact_hash:$stdout_hash,stderr_artifact_hash:$stderr_hash,runner_environment:"github_hosted",clean_checkout:true,network_isolation_enforced:true,memory_limit_enforced:true,authoritative:false}' \
  >"${output_dir}/publication-smoke-report.json"

jq -e '
  .schema_version == "publication_boundary_smoke/1" and
  .runner_environment == "github_hosted" and
  .clean_checkout == true and
  .network_isolation_enforced == true and
  .memory_limit_enforced == true and
  .authoritative == false
' "${output_dir}/publication-smoke-report.json" >/dev/null
