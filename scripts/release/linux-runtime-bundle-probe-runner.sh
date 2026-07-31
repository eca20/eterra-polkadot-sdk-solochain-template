#!/usr/bin/env bash
set -euo pipefail

IMAGE="docker.io/library/rust:1.89-bookworm@sha256:948f9b08a66e7fe01b03a98ef1c7568292e07ec2e4fe90d88c07bb14563c84ff"
workspace=""
probe=""

while [[ $# -gt 0 ]]; do
	case "$1" in
		--workspace) workspace="${2:?missing workspace}"; shift ;;
		--probe) probe="${2:?missing probe name}"; shift ;;
		--) shift; break ;;
		*) echo "unknown Linux runtime-probe runner argument: $1" >&2; exit 2 ;;
	esac
	shift
done

[[ -d "${workspace}" && ! -L "${workspace}" ]] || { echo "probe workspace must be a regular directory" >&2; exit 2; }
[[ "${probe}" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "probe must be a basename" >&2; exit 2; }
[[ -x "${workspace}/${probe}" && ! -L "${workspace}/${probe}" ]] || { echo "probe is unavailable" >&2; exit 2; }
[[ $# -gt 0 ]] || { echo "probe arguments are required" >&2; exit 2; }
command -v docker >/dev/null 2>&1 || { echo "docker is required for the Linux runtime probe" >&2; exit 2; }

exec docker run --rm \
	--platform linux/amd64 \
	--network none \
	--read-only \
	--security-opt no-new-privileges \
	--cap-drop ALL \
	--tmpfs /tmp:rw,nosuid,nodev,size=512m \
	--mount "type=bind,src=${workspace},dst=/work" \
	--workdir /work \
	"${IMAGE}" \
	"/work/${probe}" "$@"
