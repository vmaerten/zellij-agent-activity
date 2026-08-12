#!/usr/bin/env bash
# Launches the demo session. Called from demo.tape; running it by hand just
# shows you what the GIF records.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
wasm="$root/target/wasm32-wasip1/release/zellij-agent-activity.wasm"

[ -f "$wasm" ] || { echo "no wasm at $wasm, run 'task wasm' first"; exit 1; }

# The config and layout carry absolute paths, so they are rendered into a temp
# dir that doubles as the layout_dir.
tmp=$(mktemp -d)
sed -e "s|@WASM@|$wasm|" -e "s|@HERE@|$tmp|" "$here/config.kdl.in" > "$tmp/config.kdl"
sed -e "s|@HERE@|$here|" "$here/layout.kdl" > "$tmp/layout.kdl"

zellij delete-session demo --force >/dev/null 2>&1 || true
exec zellij --config "$tmp/config.kdl" -s demo
