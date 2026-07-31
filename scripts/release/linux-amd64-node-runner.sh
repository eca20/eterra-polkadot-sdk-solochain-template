#!/usr/bin/env bash
set -euo pipefail

IMAGE="docker.io/library/rust:1.89-bookworm@sha256:948f9b08a66e7fe01b03a98ef1c7568292e07ec2e4fe90d88c07bb14563c84ff"
workspace=""
node=""

while [[ $# -gt 0 ]]; do
	case "$1" in
		--workspace) workspace="${2:?missing workspace}"; shift ;;
		--node) node="${2:?missing node name}"; shift ;;
		--) shift; break ;;
		*) echo "unknown linux/amd64 runner argument: $1" >&2; exit 2 ;;
	esac
	shift
done

[[ -d "${workspace}" && ! -L "${workspace}" ]] || { echo "runner workspace must be a regular directory" >&2; exit 2; }
[[ "${node}" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "runner node must be a basename" >&2; exit 2; }
[[ -x "${workspace}/${node}" && ! -L "${workspace}/${node}" ]] || { echo "runner node is unavailable" >&2; exit 2; }
[[ $# -gt 0 ]] || { echo "runner requires a node command" >&2; exit 2; }
command -v docker >/dev/null 2>&1 || { echo "docker is required for the linux/amd64 runner" >&2; exit 2; }

exec docker run --rm \
	--platform linux/amd64 \
	--network none \
	--read-only \
	--security-opt no-new-privileges \
	--cap-drop ALL \
	--tmpfs /tmp:rw,noexec,nosuid,nodev,size=64m \
	--mount "type=bind,src=${workspace},dst=/work,readonly" \
	--workdir /work \
	"${IMAGE}" \
	"/work/${node}" "$@"
