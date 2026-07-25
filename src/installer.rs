// Idempotent, versioned, backup-first auto-install of the Claude Code hook into
// `~/.claude/settings.json` (adapted from zellaude's installer). Pure: it only
// *builds* the shell command; the wasm adapter runs it via `Effect::RunCommand`,
// so this stays host-free and native-testable (ADR-0004).

use std::collections::BTreeMap;

const HOOK_VERSION_TAG: &str = concat!("# zellij-agent-activity v", env!("CARGO_PKG_VERSION"));

/// The hook script content with the version tag inserted after the shebang, so a
/// reinstall can detect whether the on-disk hook is already current.
fn hook_script_content() -> String {
    let original = include_str!("../scripts/zellij-agent-activity-hook.sh");
    if let Some(pos) = original.find('\n') {
        let (shebang, rest) = original.split_at(pos);
        format!("{shebang}\n{HOOK_VERSION_TAG}{rest}")
    } else {
        original.to_string()
    }
}

const INSTALL_TEMPLATE: &str = r##"set -e
HOOK_PATH="$HOME/.config/zellij/plugins/zellij-agent-activity-hook.sh"
HOOK_CMD='${HOME}/.config/zellij/plugins/zellij-agent-activity-hook.sh'
SETTINGS="$HOME/.claude/settings.json"

resolve_file_symlink() {
  path=$1
  while [ -L "$path" ]; do
    dir=$(cd "$(dirname "$path")" && pwd -P)
    target=$(readlink "$path")
    case "$target" in
      /*) path=$target ;;
      *) path=$dir/$target ;;
    esac
  done
  dir=$(cd "$(dirname "$path")" && pwd -P)
  printf '%s/%s\n' "$dir" "$(basename "$path")"
}

# Resolve symlink so mv doesn't replace the link with a regular file
if [ -L "$SETTINGS" ]; then
  SETTINGS="$(resolve_file_symlink "$SETTINGS")"
fi

# Already current? (hook file carries the version tag AND settings references it)
if grep -qF '__VERSION_TAG__' "$HOOK_PATH" 2>/dev/null; then
  if [ -f "$SETTINGS" ] && grep -qF "$HOOK_CMD" "$SETTINGS" 2>/dev/null; then
    echo "current"
    exit 0
  fi
fi

# Write hook script
mkdir -p "$(dirname "$HOOK_PATH")"
cat > "$HOOK_PATH" << 'CTI_HOOK_EOF'
__HOOK_SCRIPT__
CTI_HOOK_EOF
chmod +x "$HOOK_PATH"

# Register hooks (requires jq)
if ! command -v jq >/dev/null 2>&1; then
  echo "no_jq"
  exit 0
fi

if [ ! -f "$SETTINGS" ]; then
  mkdir -p "$HOME/.claude"
  echo '{}' > "$SETTINGS"
fi

# Back up settings before modifying
cp "$SETTINGS" "$SETTINGS.bak"

# Remove ALL existing zellij-agent-activity hook entries (any path ending in our script)
tmp=$(mktemp)
jq '
  if .hooks and (.hooks | type == "object") then
    .hooks |= with_entries(
      .value |= [
        .[] | . as $group |
        ($group.hooks // []) | map(select((.command // "") | endswith("zellij-agent-activity-hook.sh") | not)) |
        . as $filtered |
        if length > 0 then ($group | .hooks = $filtered) else empty end
      ]
    ) | .hooks |= with_entries(select(.value | length > 0)) |
    if .hooks == {} then del(.hooks) else . end
  else . end
' "$SETTINGS" > "$tmp" && mv "$tmp" "$SETTINGS"

# Register only the events the plugin consumes (fewer forked hook subprocesses).
# `SubagentStop` is absent on purpose — see activity_from_event.
EVENTS='["SessionStart","PreToolUse","PostToolUse","UserPromptSubmit","Notification","Stop","SessionEnd"]'
ENTRY=$(jq -nc --arg cmd "$HOOK_CMD" '[{"hooks": [{"type": "command", "command": $cmd, "timeout": 5, "async": true}]}]')
tmp=$(mktemp)
jq --argjson events "$EVENTS" --argjson entry "$ENTRY" '
  .hooks //= {} |
  reduce ($events[]) as $event (.; .hooks[$event] = (.hooks[$event] // []) + $entry)
' "$SETTINGS" > "$tmp" && mv "$tmp" "$SETTINGS"

echo "installed"
"##;

/// Build the idempotent install command for `Effect::RunCommand`. Checks whether
/// hooks are current, writes the hook script, and registers the events.
pub fn install_command() -> (Vec<String>, BTreeMap<String, String>) {
    let cmd = INSTALL_TEMPLATE
        .replace("__VERSION_TAG__", HOOK_VERSION_TAG)
        .replace("__HOOK_SCRIPT__", &hook_script_content());
    let ctx = BTreeMap::from([("type".to_string(), "install_hooks".to_string())]);
    (vec!["sh".to_string(), "-c".to_string(), cmd], ctx)
}
