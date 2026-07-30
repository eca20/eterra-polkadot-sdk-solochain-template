#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
vendor_dir="${repo_root}/vendor/blockchainia-flow"
expected_commit="e04225009eb33f1ee4fabb0666d4eae50e443962"
expected_tree="7f3cc9a0a2a40c8d04f0b1148220f908adf55f7c"
scratch_dir="$(mktemp -d)"

cleanup() {
  rm -rf "${scratch_dir}"
}
trap cleanup EXIT

test -f "${vendor_dir}/Cargo.toml"
test -f "${vendor_dir}/LICENSE"
test -f "${repo_root}/vendor/blockchainia-flow.lock.json"

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
  actual_commit="$(git -C "${source_repo}" rev-parse HEAD)"
  source_tree="$(git -C "${source_repo}" rev-parse HEAD^{tree})"
  test "${actual_commit}" = "${expected_commit}"
  test "${source_tree}" = "${expected_tree}"
fi

echo "verified Blockchainia Flow ${expected_commit} (${expected_tree})"
