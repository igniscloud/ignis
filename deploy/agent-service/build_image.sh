#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../.." && pwd)"
image="ghcr.io/igniscloud/agents/agent-service:latest"

cargo build --manifest-path "${repo_root}/Cargo.toml" -p agent-service --release

mkdir -p "${script_dir}/artifacts"
cp "${repo_root}/target/release/agent-service" "${script_dir}/artifacts/agent-service"

podman build \
  -f "${script_dir}/Containerfile" \
  -t "${image}" \
  "${script_dir}"

echo "Built ${image}"
