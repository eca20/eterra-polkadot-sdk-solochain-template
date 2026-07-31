#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD="${ROOT_DIR}/scripts/release/build-runtime-upgrade-bundle.sh"
NEXUS_V2_BUNDLE="${ROOT_DIR}/scripts/release/build-nexus-v2-alpha-runtime-bundle.sh"
REHEARSE="${ROOT_DIR}/scripts/release/rehearse-runtime-upgrade.sh"
MEDIA_DEPLOY="${ROOT_DIR}/deploy/alpha/macmini2010/deploy-media.sh"
NODE_DEPLOY="${ROOT_DIR}/deploy/alpha/macmini2010/deploy-node.sh"
DEPLOY_ALL="${ROOT_DIR}/deploy/alpha/macmini2010/deploy-all.sh"
DEPLOY_LIB="${ROOT_DIR}/deploy/alpha/macmini2010/lib.sh"
RESET_NODE="${ROOT_DIR}/deploy/alpha/macmini2010/reset-node.sh"
RESET_MEDIA="${ROOT_DIR}/deploy/alpha/macmini2010/reset-media.sh"
READINESS_VERIFIER="${ROOT_DIR}/scripts/nexus-v2-private-alpha/verify_reset_readiness.py"
POST_CUTOVER_COORDINATOR="${ROOT_DIR}/deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py"
POST_CUTOVER_COORDINATOR_TEST="${ROOT_DIR}/deploy/alpha/macmini2010/test_nexus_v2_post_cutover_coordinator.py"
CLEAN_BUILD="${ROOT_DIR}/scripts/clean-build-artifacts.sh"
DEPLOY="${ROOT_DIR}/scripts/deploy.sh"

bash -n "$BUILD" "$NEXUS_V2_BUNDLE" "$REHEARSE" "$MEDIA_DEPLOY" "$NODE_DEPLOY" "$DEPLOY_ALL" \
	"$DEPLOY_LIB" "$RESET_NODE" "$RESET_MEDIA" "$CLEAN_BUILD" "$DEPLOY" \
	"${ROOT_DIR}/deploy/macmini2010/deploy-node.sh"
python3 -m unittest \
	"${ROOT_DIR}/scripts/nexus-v2-private-alpha/test_verify_reset_readiness.py"
python3 -m unittest "${POST_CUTOVER_COORDINATOR_TEST}"
"$BUILD" --help >/dev/null
"$NEXUS_V2_BUNDLE" --help >/dev/null
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

rg -q 'state_getStorage' "$NEXUS_V2_BUNDLE"
rg -q '0x3a636f6465' "$NEXUS_V2_BUNDLE"
rg -Fq '\"method\":\"state_getStorage\",\"params\":[\"${CODE_STORAGE_KEY}\"]' "$NEXUS_V2_BUNDLE"
rg -q 'bytes.fromhex' "$NEXUS_V2_BUNDLE"
rg -Fq '[[ "${temporary_node_embedded_wasm_sha}" == "${staged_production_wasm_sha}" ]] || {' "$NEXUS_V2_BUNDLE"
rg -q 'temporary node embedded :code does not match staged production compact-compressed Wasm' "$NEXUS_V2_BUNDLE"
rg -q 'stagedProductionWasmSha256' "$NEXUS_V2_BUNDLE"
rg -q 'temporaryNodeEmbeddedWasmSha256' "$NEXUS_V2_BUNDLE"
rg -q 'tryRuntimeWasmSha256' "$NEXUS_V2_BUNDLE"
rg -q -- '--features runtime-production' "$NEXUS_V2_BUNDLE"
rg -q -- '--features try-runtime,runtime-production' "$NEXUS_V2_BUNDLE"
rg -q 'temporary-node-embedded-code.rpc.json' "$NEXUS_V2_BUNDLE"
rg -q 'runtime-spec-106.temporary-node-embedded-code.wasm' "$NEXUS_V2_BUNDLE"

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
rg -q -- '--fresh-reset-readiness' "$MEDIA_DEPLOY"
rg -q 'release deploys preserve media/IPFS state unless --fresh is paired with --fresh-reset-readiness' "$MEDIA_DEPLOY"
rg -q 'a guarded release media reset requires --promote-candidate' "$MEDIA_DEPLOY"
rg -q 'readiness packet was already consumed for the media reset' "$MEDIA_DEPLOY"
rg -q 'archive/nexus-v2-fresh-reset' "$MEDIA_DEPLOY"
rg -q 'dry-run: guarded media/IPFS reset' "$MEDIA_DEPLOY"
rg -q 'MEDIA_RELEASE_CONTENT_SMOKE_URL' "$MEDIA_DEPLOY"
if rg -q '\.runtime\.specVersion == 104' "$MEDIA_DEPLOY"; then
	echo "media health validation must use the pinned spec-106 environment value" >&2
	exit 1
fi
rg -q 'status --porcelain --untracked-files=all' "$DEPLOY_LIB"
rg -q 'NEXUS_V2_LOCAL_ONLY_RELEASE' "$DEPLOY_LIB"
rg -q 'guarded release reset requires NEXUS_V2_LOCAL_ONLY_RELEASE=1' "$DEPLOY_LIB"
rg -q 'NEXUS_V2_RESET_READINESS_SHA256' "$DEPLOY_LIB"
rg -q 'verify_reset_readiness.py' "$DEPLOY_LIB"
rg -q 'release deploy requires KUBO_IMAGE pinned by registry digest' "$DEPLOY_LIB"
rg -q 'ALLOW_DIRTY_DEPLOY is forbidden for release deploys' "$DEPLOY_LIB"
rg -q 'ETERRA_KEEP_BUILD_ARTIFACTS' "$DEPLOY"
rg -q 'scripts/clean-build-artifacts.sh' "$DEPLOY"
rg -q -- '--fresh-reset-readiness' "$NODE_DEPLOY"
rg -q 'release deploys preserve live state unless --purge-state is paired with --fresh-reset-readiness' "$NODE_DEPLOY"
rg -q 'readiness packet was already consumed for the node reset' "$NODE_DEPLOY"
rg -q 'dry-run: guarded node purge validated' "$NODE_DEPLOY"
rg -q 'REMOTE_CARGO_CLEAN_AFTER_DEPLOY' "$NODE_DEPLOY"
rg -q 'refusing unsafe Cargo cleanup path' "$NODE_DEPLOY"
rg -q 'direct release reset is forbidden' "$RESET_NODE"
rg -q 'direct release reset is forbidden' "$RESET_MEDIA"
rg -q 'nexus-v2-private-alpha-reset-readiness' "$READINESS_VERIFIER"
rg -q 'pre-v16-fresh-reset-frozen' "$READINESS_VERIFIER"
rg -q 'NEXUS_V2_ROLLBACK_PLAN_SHA256' "$POST_CUTOVER_COORDINATOR"
rg -q 'post-acceptance-pause-and-forward-fix' "$POST_CUTOVER_COORDINATOR"
rg -q 'archive-failed-v2' "$POST_CUTOVER_COORDINATOR"
rg -q 'dry-run' "$POST_CUTOVER_COORDINATOR"
if rg -q "['\"](ssh|scp|rsync|docker|curl)['\"]" "$POST_CUTOVER_COORDINATOR"; then
	echo "post-cutover coordinator must delegate remote operations to pinned component drivers" >&2
	exit 1
fi
rg -q 'v2AcceptanceAssetsExist' "$READINESS_VERIFIER"
rg -q 'resetExecuted' "$READINESS_VERIFIER"

echo "release script safety checks passed"
