#!/usr/bin/env bash

set -euo pipefail

default_config_path="/app/config/opencode-agent-service.toml"
config_path="${AGENT_SERVICE_CONFIG:-}"
listen_addr="${AGENT_SERVICE_LISTEN_ADDR:-0.0.0.0:3900}"
mcp_url="${AGENT_SERVICE_MCP_URL:-http://127.0.0.1:3900/mcp}"
opencode_config_path="${OPENCODE_CONFIG:-/agent-home/.config/opencode/opencode.json}"

export AGENT_SERVICE_LISTEN_ADDR="${listen_addr}"
export AGENT_SERVICE_DATABASE_PATH="${AGENT_SERVICE_DATABASE_PATH:-/app/data/opencode-agent-service.sqlite3}"
export AGENT_SERVICE_RUNTIME="${AGENT_SERVICE_RUNTIME:-opencode}"
export AGENT_SERVICE_WORKSPACE_DIR="${AGENT_SERVICE_WORKSPACE_DIR:-/app/work}"
export OPENCODE_CONFIG="${opencode_config_path}"

mkdir -p /agent-home/.config/opencode /agent-home/.agents/skills /app/data /app/work

if [[ ! -f "${opencode_config_path}" ]]; then
  echo "OpenCode config file is required at ${opencode_config_path}" >&2
  exit 1
fi

cat > /app/work/opencode.json <<EOF
{
  "\$schema": "https://opencode.ai/config.json",
  "mcp": {
    "agent-service": {
      "type": "remote",
      "url": "${mcp_url}",
      "enabled": true
    }
  }
}
EOF

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
  --runtime opencode \
  --listen-addr "${listen_addr}"
