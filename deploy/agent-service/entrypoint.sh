#!/usr/bin/env bash

set -euo pipefail

default_config_path="/app/config/agent-service.toml"
config_path="${AGENT_SERVICE_CONFIG:-}"
listen_addr="${AGENT_SERVICE_LISTEN_ADDR:-0.0.0.0:3900}"
mcp_url="${AGENT_SERVICE_MCP_URL:-http://127.0.0.1:3900/mcp}"
runtime="${AGENT_SERVICE_RUNTIME:-codex}"
opencode_config_path="${OPENCODE_CONFIG:-/agent-home/.config/opencode/opencode.json}"

export AGENT_SERVICE_LISTEN_ADDR="${listen_addr}"
export AGENT_SERVICE_DATABASE_PATH="${AGENT_SERVICE_DATABASE_PATH:-/app/data/agent-service.sqlite3}"
export AGENT_SERVICE_RUNTIME="${runtime}"
export AGENT_SERVICE_WORKSPACE_DIR="${AGENT_SERVICE_WORKSPACE_DIR:-/app/work}"
export OPENCODE_CONFIG="${opencode_config_path}"

mkdir -p /agent-home/.codex /agent-home/.config/opencode /agent-home/.agents/skills /app/config /app/data /app/work

if [[ "${runtime}" == "codex" ]]; then
  if [[ -f /agent-home/.codex/config.toml && ! -w /agent-home/.codex/config.toml ]]; then
    mkdir -p /app/work/codex-home
    cp -f /agent-home/.codex/auth.json /app/work/codex-home/auth.json 2>/dev/null || true
    cp -f /agent-home/.codex/config.toml /app/work/codex-home/config.toml
    chmod 600 /app/work/codex-home/auth.json /app/work/codex-home/config.toml 2>/dev/null || true
    export CODEX_HOME=/app/work/codex-home
  fi
  codex mcp remove agent-service >/dev/null 2>&1 || true
  codex mcp add agent-service \
    --url "${mcp_url}" >/dev/null
elif [[ "${runtime}" == "opencode" ]]; then
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
else
  echo "unsupported AGENT_SERVICE_RUNTIME: ${runtime}" >&2
  exit 1
fi

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
  --runtime "${runtime}" \
  --listen-addr "${listen_addr}"
