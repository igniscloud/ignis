#!/usr/bin/env bash

set -euo pipefail

default_config_path="/app/config/agent-service.toml"
config_path="${AGENT_SERVICE_CONFIG:-}"
listen_addr="${AGENT_SERVICE_LISTEN_ADDR:-0.0.0.0:3900}"
mcp_url="${AGENT_SERVICE_MCP_URL:-http://127.0.0.1:3900/mcp}"

export AGENT_SERVICE_LISTEN_ADDR="${listen_addr}"
export AGENT_SERVICE_DATABASE_PATH="${AGENT_SERVICE_DATABASE_PATH:-/app/data/agent-service.sqlite3}"
export AGENT_SERVICE_WORKSPACE_DIR="${AGENT_SERVICE_WORKSPACE_DIR:-/app/work}"

mkdir -p /agent-home/.agents/skills /app/data /app/work

codex mcp remove agent-service >/dev/null 2>&1 || true
codex mcp add agent-service \
  --url "${mcp_url}" >/dev/null

config_args=()
if [[ -n "${config_path}" ]]; then
  if [[ ! -f "${config_path}" ]]; then
    echo "AGENT_SERVICE_CONFIG points to missing file: ${config_path}" >&2
    exit 1
  fi
  config_args=(--config "${config_path}")
elif [[ -f "${default_config_path}" ]]; then
  config_args=(--config "${default_config_path}")
fi

exec /usr/local/bin/agent-service \
  "${config_args[@]}" \
  --listen-addr "${listen_addr}"
