#!/usr/bin/env bash
# Build, validate and package the plugin. Requires `openidea-plugin` on PATH
# (cargo install --path <openidea>/crates/ide-host --bin openidea-plugin).
# The package is signed when the publisher key sits next to this script
# (gitignored); without it the package is named -unsigned and only installs
# from disk, not from the marketplace.
set -euo pipefail
cd "$(dirname "$0")"

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_commit_message_helper.wasm dist/plugin.wasm
openidea-plugin check .

publisher=qszfy666
key="$publisher.key"
package="dist/$publisher.commit-message-helper-0.7.1"
if [ -f "$key" ]; then
  openidea-plugin pack . --key "$key" --publisher "$publisher" --out "$package.oiplugin"
  shasum -a 256 "$package.oiplugin"
else
  openidea-plugin pack . --out "$package-unsigned.oiplugin"
fi
