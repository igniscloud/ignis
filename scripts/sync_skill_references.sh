#!/bin/sh

set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

copy_file() {
    src="$1"
    dst="$2"
    mkdir -p "$(dirname "$dst")"
    cp "$src" "$dst"
}

reset_dir() {
    dir="$1"
    rm -rf "$dir"
    mkdir -p "$dir"
}

copy_example_project() {
    example_name="$1"
    skill_name="$2"
    skill_example_dir="$ROOT_DIR/skills/$skill_name/references/examples/$example_name"

    reset_dir "$skill_example_dir"
    copy_file "$ROOT_DIR/examples/$example_name/README.md" "$skill_example_dir/README.md"
    copy_file "$ROOT_DIR/examples/$example_name/ignis.hcl" "$skill_example_dir/ignis.hcl"
    if [ -f "$ROOT_DIR/examples/$example_name/services/api/Cargo.toml" ]; then
        copy_file "$ROOT_DIR/examples/$example_name/services/api/Cargo.toml" "$skill_example_dir/services/api/Cargo.toml"
    fi
    if [ -d "$ROOT_DIR/examples/$example_name/services/api/src" ]; then
        reset_dir "$skill_example_dir/services/api/src"
        cp -R "$ROOT_DIR/examples/$example_name/services/api/src/." "$skill_example_dir/services/api/src/"
    fi
    if [ -f "$ROOT_DIR/examples/$example_name/services/api/wit/world.wit" ]; then
        copy_file "$ROOT_DIR/examples/$example_name/services/api/wit/world.wit" "$skill_example_dir/services/api/wit/world.wit"
    fi
    if [ -f "$ROOT_DIR/examples/$example_name/services/web/src/index.html" ]; then
        copy_file "$ROOT_DIR/examples/$example_name/services/web/src/index.html" "$skill_example_dir/services/web/src/index.html"
    fi
}

copy_agent_example_files() {
    example_name="$1"
    skill_name="$2"
    service_name="$3"
    skill_example_dir="$ROOT_DIR/skills/$skill_name/references/examples/$example_name"

    if [ -f "$ROOT_DIR/examples/$example_name/services/$service_name/AGENTS.md" ]; then
        copy_file "$ROOT_DIR/examples/$example_name/services/$service_name/AGENTS.md" "$skill_example_dir/services/$service_name/AGENTS.md"
    fi
    if [ -f "$ROOT_DIR/examples/$example_name/services/$service_name/opencode.json.example" ]; then
        copy_file "$ROOT_DIR/examples/$example_name/services/$service_name/opencode.json.example" "$skill_example_dir/services/$service_name/opencode.json.example"
    fi
}

copy_file "$ROOT_DIR/docs/cli.md" "$ROOT_DIR/skills/ignis/references/cli.md"
copy_file "$ROOT_DIR/docs/ignis-hcl.md" "$ROOT_DIR/skills/ignis/references/ignis-hcl.md"
copy_file "$ROOT_DIR/docs/integration.md" "$ROOT_DIR/skills/ignis/references/integration.md"
copy_file "$ROOT_DIR/docs/object-store-presign.md" "$ROOT_DIR/skills/ignis/references/object-store-presign.md"
copy_file "$ROOT_DIR/docs/jobs-and-schedules.md" "$ROOT_DIR/skills/ignis/references/jobs-and-schedules.md"
copy_file "$ROOT_DIR/docs/taskplan.md" "$ROOT_DIR/skills/ignis/references/taskplan.md"
copy_file "$ROOT_DIR/docs/system-api.md" "$ROOT_DIR/skills/ignis/references/system-api.md"

reset_dir "$ROOT_DIR/skills/ignis/references/ignis-sdk"
cp -R "$ROOT_DIR/docs/ignis-sdk/." "$ROOT_DIR/skills/ignis/references/ignis-sdk/"

copy_example_project "hello-fullstack" "ignis"
copy_example_project "sqlite-example" "ignis"
copy_example_project "cos-and-jobs-example" "ignis"
copy_example_project "opencode-agent-e2e" "ignis"
copy_agent_example_files "opencode-agent-e2e" "ignis" "coordinator-agent"
copy_agent_example_files "opencode-agent-e2e" "ignis" "elementary-agent"
copy_agent_example_files "opencode-agent-e2e" "ignis" "bridge-agent"
copy_agent_example_files "opencode-agent-e2e" "ignis" "modularity-agent"
copy_agent_example_files "opencode-agent-e2e" "ignis" "teacher-agent"
copy_agent_example_files "opencode-agent-e2e" "ignis" "rigor-agent"
copy_example_project "ignis-login-example" "ignis-login"
