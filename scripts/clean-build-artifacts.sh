#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${ROOT_DIR}/target"

if [[ "${TARGET_DIR}" != "${ROOT_DIR}/target" ]] || [[ "${TARGET_DIR}" == "/" ]]; then
	echo "[clean-build] refusing unsafe target path: ${TARGET_DIR}" >&2
	exit 1
fi

if [[ ! -d "${TARGET_DIR}" ]]; then
	echo "[clean-build] no Cargo build directory to remove: ${TARGET_DIR}"
	exit 0
fi

echo "[clean-build] removing reproducible Cargo build artifacts: ${TARGET_DIR}"
rm -rf -- "${TARGET_DIR}"
echo "[clean-build] cleanup complete"
