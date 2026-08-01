#!/usr/bin/env bash
# Launches the demo session. Called from demo.tape — running it by hand just
# shows you what the GIF records.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
wasm="$root/target/wasm32-wasip1/release/zellij-agent-activity.wasm"

[ -f "$wasm" ] || { echo "no wasm at $wasm — run 'task wasm' first"; exit 1; }
for tool in starship eza; do
  command -v "$tool" >/dev/null ||
    { echo "$tool is not on PATH — the demo prompt and listing need it (brew install $tool)"; exit 1; }
done

# A throwaway $HOME: nothing personal reaches the screen and the demo replays the
# same anywhere. `pwd -P` because macOS mktemp hands back /var/folders/…, a
# symlink to /private/var/folders/…, and zellij reports the resolved cwd.
tmp=$(cd "$(mktemp -d)" && pwd -P)
home="$tmp/home"
mkdir -p "$home/code" "$home/.config"

# The repo the sessions work in is this one, cloned: the git log, the diffstat and
# the branch in the prompt are all real output.
git clone --quiet --local --branch main "$root" "$home/code/zellij-agent-activity"
git -C "$home/code/zellij-agent-activity" remote set-url origin \
  https://github.com/vmaerten/zellij-agent-activity.git

cp "$here/starship.toml" "$home/.config/starship.toml"

sed -e "s|@WASM@|$wasm|" -e "s|@HERE@|$tmp|" "$here/config.kdl.in" > "$tmp/config.kdl"
sed -e "s|@HERE@|$here|" -e "s|@HOME@|$home|" "$here/layout.kdl" > "$tmp/layout.kdl"

# Its own socket dir, not just its own $HOME. Zellij keeps session sockets under
# $TMPDIR, and a demo server outlives the recording: vhs closes the terminal, the
# client is logged out, the server tears its plugins down but keeps running. A
# shared socket dir lets the next take re-attach to that one and come up
# plugin-less — a recording of tabs that never get a symbol.
export ZELLIJ_SOCKET_DIR="$tmp/sockets"
export HOME="$home"
export STARSHIP_CONFIG="$home/.config/starship.toml"
mkdir -p "$ZELLIJ_SOCKET_DIR"

exec zellij --config "$tmp/config.kdl" -s demo
