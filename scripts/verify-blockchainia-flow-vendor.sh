#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
vendor_dir="${repo_root}/vendor/blockchainia-flow"
expected_commit="c078422c18459aa82657d5c8f761c205c5fe93f1"
expected_tree="4db0dddae74f04f073b2bde04268fccb896903a7"
expected_version="0.1.0-alpha.1"
expected_vendor_path="vendor/blockchainia-flow"
lock_file="${repo_root}/vendor/blockchainia-flow.lock.json"
scratch_dir="$(mktemp -d)"

cleanup() {
  rm -rf "${scratch_dir}"
}
trap cleanup EXIT

test -f "${vendor_dir}/Cargo.toml"
test -f "${vendor_dir}/LICENSE"
test -f "${lock_file}"

python3 - "${lock_file}" "${expected_commit}" "${expected_tree}" "${expected_version}" "${expected_vendor_path}" <<'PY'
import json
import sys

lock_path, expected_commit, expected_tree, expected_version, expected_vendor_path = sys.argv[1:]
with open(lock_path, encoding="utf-8") as handle:
    lock = json.load(handle)

expected = {
    "schema": "blockchainia.flow.vendor-lock.v1",
    "sourceCommit": expected_commit,
    "sourceTree": expected_tree,
    "version": expected_version,
    "vendorPath": expected_vendor_path,
}
for key, value in expected.items():
    if lock.get(key) != value:
        raise SystemExit(
            f"vendored Blockchainia Flow lock mismatch for {key}: "
            f"expected {value!r}, got {lock.get(key)!r}"
        )
PY

git -C "${scratch_dir}" init --quiet
git -C "${scratch_dir}" config core.autocrlf false
git -C "${scratch_dir}" config core.filemode true
cp -R "${vendor_dir}/." "${scratch_dir}/"
git -C "${scratch_dir}" add --all
actual_tree="$(git -C "${scratch_dir}" write-tree)"

if [[ "${actual_tree}" != "${expected_tree}" ]]; then
  echo "vendored Blockchainia Flow tree mismatch" >&2
  echo "expected: ${expected_tree}" >&2
  echo "actual:   ${actual_tree}" >&2
  exit 1
fi

if [[ $# -gt 0 ]]; then
  source_repo="$1"
  if [[ -n "$(git -C "${source_repo}" status --porcelain --untracked-files=all)" ]]; then
    echo "Blockchainia Flow source repository is dirty" >&2
    exit 1
  fi
  actual_commit="$(git -C "${source_repo}" rev-parse HEAD)"
  source_tree="$(git -C "${source_repo}" rev-parse HEAD^{tree})"
  test "${actual_commit}" = "${expected_commit}"
  test "${source_tree}" = "${expected_tree}"
fi

echo "verified Blockchainia Flow ${expected_commit} (${expected_tree})"
