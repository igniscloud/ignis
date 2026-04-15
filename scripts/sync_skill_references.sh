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
    if [ -f "$ROOT_DIR/examples/$example_name/services/api/src/lib.rs" ]; then
        copy_file "$ROOT_DIR/examples/$example_name/services/api/src/lib.rs" "$skill_example_dir/services/api/src/lib.rs"
    fi
    if [ -f "$ROOT_DIR/examples/$example_name/services/api/wit/world.wit" ]; then
        copy_file "$ROOT_DIR/examples/$example_name/services/api/wit/world.wit" "$skill_example_dir/services/api/wit/world.wit"
    fi
    if [ -f "$ROOT_DIR/examples/$example_name/services/web/src/index.html" ]; then
        copy_file "$ROOT_DIR/examples/$example_name/services/web/src/index.html" "$skill_example_dir/services/web/src/index.html"
    fi
}

copy_file "$ROOT_DIR/docs/cli.md" "$ROOT_DIR/skills/ignis/references/cli.md"
copy_file "$ROOT_DIR/docs/ignis-hcl.md" "$ROOT_DIR/skills/ignis/references/ignis-hcl.md"
copy_file "$ROOT_DIR/docs/integration.md" "$ROOT_DIR/skills/ignis/references/integration.md"
copy_file "$ROOT_DIR/docs/object-store-presign.md" "$ROOT_DIR/skills/ignis/references/object-store-presign.md"

reset_dir "$ROOT_DIR/skills/ignis/references/ignis-sdk"
cp -R "$ROOT_DIR/docs/ignis-sdk/." "$ROOT_DIR/skills/ignis/references/ignis-sdk/"

copy_example_project "hello-fullstack" "ignis"
copy_example_project "sqlite-example" "ignis"
copy_example_project "object-store-presign-example" "ignis"
copy_example_project "google-cos-upload-example" "ignis"
copy_example_project "ignis-login-example" "ignis-login"
