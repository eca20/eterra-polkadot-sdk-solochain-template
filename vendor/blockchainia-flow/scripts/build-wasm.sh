#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${repo_root}/apps/builder/public/manifest-wasm"

wasm-pack build \
  "${repo_root}/crates/blockchainia-flow-manifest-wasm" \
  --target web \
  --release \
  --out-dir "${output_dir}"
