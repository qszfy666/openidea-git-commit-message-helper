#!/usr/bin/env bash
# Build, validate and package the plugin. Requires `openidea-plugin` on PATH
# (cargo install --path <openidea>/crates/ide-host --bin openidea-plugin).
set -euo pipefail
cd "$(dirname "$0")"

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_commit_message_helper.wasm dist/plugin.wasm
openidea-plugin check .
openidea-plugin pack . --out "dist/qszfy666.commit-message-helper-0.5.1.oiplugin"
