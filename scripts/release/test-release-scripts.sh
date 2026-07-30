#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD="${ROOT_DIR}/scripts/release/build-runtime-upgrade-bundle.sh"
REHEARSE="${ROOT_DIR}/scripts/release/rehearse-runtime-upgrade.sh"
MEDIA_DEPLOY="${ROOT_DIR}/deploy/alpha/macmini2010/deploy-media.sh"
DEPLOY_LIB="${ROOT_DIR}/deploy/alpha/macmini2010/lib.sh"
CLEAN_BUILD="${ROOT_DIR}/scripts/clean-build-artifacts.sh"
DEPLOY="${ROOT_DIR}/scripts/deploy.sh"

bash -n "$BUILD" "$REHEARSE" "$MEDIA_DEPLOY" "$DEPLOY_LIB" "$CLEAN_BUILD" "$DEPLOY" \
	"${ROOT_DIR}/deploy/alpha/macmini2010/deploy-node.sh" \
	"${ROOT_DIR}/deploy/macmini2010/deploy-node.sh"
"$BUILD" --help >/dev/null
"$REHEARSE" --help >/dev/null

rg -q -- '--expected-source-commit' "$BUILD"
rg -q 'status --porcelain --untracked-files=all' "$BUILD"
rg -q 'release_branch="release/' "$BUILD"
rg -q 'ls-remote origin' "$BUILD"
rg -q 'CARGO_INCREMENTAL=0' "$BUILD"
rg -q 'SOURCE_DATE_EPOCH' "$BUILD"
rg -q 'cargo build --locked' "$BUILD"
if rg -q -- '--allow-dirty' "$BUILD"; then
	echo "release runtime bundles must not permit dirty source" >&2
	exit 1
fi

rg -q -- '--try-runtime-revision' "$REHEARSE"
rg -q -- '--try-runtime-sha256' "$REHEARSE"
rg -q 'try-runtime binary hash mismatch' "$REHEARSE"
rg -q 'media_node_env="production"' "$DEPLOY_LIB"
rg -q 'media_node_env="development"' "$DEPLOY_LIB"
rg -q 'snapshotSha256' "$REHEARSE"
rg -q 'runtimeUpgradePlanSha256' "$REHEARSE"
if rg -q '\b(set_code|sudo|submit)\b' "$REHEARSE"; then
	echo "copied-state rehearsal must not contain live submission paths" >&2
	exit 1
fi

rg -q -- '--build-candidate' "$MEDIA_DEPLOY"
rg -q -- '--promote-candidate' "$MEDIA_DEPLOY"
rg -q -- '--no-build --pull never' "$MEDIA_DEPLOY"
rg -q 'release media deploys require --build-candidate or --promote-candidate' "$MEDIA_DEPLOY"
rg -q 'release deploys must preserve media/IPFS state; --fresh is forbidden' "$MEDIA_DEPLOY"
rg -q 'MEDIA_RELEASE_CONTENT_SMOKE_URL' "$MEDIA_DEPLOY"
rg -q 'status --porcelain --untracked-files=all' "$DEPLOY_LIB"
rg -q 'release deploy requires KUBO_IMAGE pinned by registry digest' "$DEPLOY_LIB"
rg -q 'ALLOW_DIRTY_DEPLOY is forbidden for release deploys' "$DEPLOY_LIB"
rg -q 'ETERRA_KEEP_BUILD_ARTIFACTS' "$DEPLOY"
rg -q 'scripts/clean-build-artifacts.sh' "$DEPLOY"
rg -q 'REMOTE_CARGO_CLEAN_AFTER_DEPLOY' "${ROOT_DIR}/deploy/alpha/macmini2010/deploy-node.sh"
rg -q 'refusing unsafe Cargo cleanup path' "${ROOT_DIR}/deploy/alpha/macmini2010/deploy-node.sh"

echo "release script safety checks passed"
