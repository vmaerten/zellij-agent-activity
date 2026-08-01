#!/usr/bin/env bash
# A staged agent session for the README demo: prints plausible output and reports
# its own activity on the pipe, using its own $ZELLIJ_PANE_ID so no id has to be
# guessed from outside. One of these runs per tab.
set -u

say() { printf '  \033[38;5;245m%s\033[0m\n' "$1"; }

report() {
  local args="pane_id=${ZELLIJ_PANE_ID},hook_event=$1,ts_ms=$(($(date +%s) * 1000))"
  [ -n "${2:-}" ] && args="${args},tool_name=$2"
  [ -n "${3:-}" ] && args="${args},notification=$3"
  zellij pipe --name agent_activity.v1 --args "$args" >/dev/null 2>&1 &
  sleep 0.05
}

step() { report "$1" "${2:-}" "${3:-}"; say "$4"; sleep "$5"; }

# The tape answers zellij's permission prompt with a keystroke; without this it
# would echo into whichever pane has focus. Also gives that grant time to land
# before the first event, so the timeline starts with the plugin listening.
stty -echo 2>/dev/null || true

printf '\033[38;5;110m❯\033[0m claude\n\n'
sleep 5
report SessionStart
sleep 1.2

case "${1:-worker}" in
  worker)
    step UserPromptSubmit ""     "" "Thinking…"                        1.6
    step PreToolUse       Read   "" "Read · src/main.rs"               1.8
    step PreToolUse       Bash   "" "Bash · cargo test"                2.6
    step PostToolUse      ""     "" "40 passed"                        1.2
    step PreToolUse       Edit   "" "Edit · src/main.rs"               2.4
    step PostToolUse      ""     "" "Thinking…"                        1.6
    step PreToolUse       Bash   "" "Bash · cargo clippy"              2.4
    step Stop             ""     "" "Done."                            8.0
    ;;
  blocked)
    step UserPromptSubmit ""     "" "Thinking…"                        1.0
    step PreToolUse       Bash   "" "Bash · git push"                  1.8
    step Notification     ""     permission "Waiting for your approval" 12.0
    ;;
  done)
    step UserPromptSubmit ""     "" "Thinking…"                        1.0
    step PreToolUse       Read   "" "Read · README.md"                 1.6
    step Stop             ""     "" "Done."                            12.0
    ;;
esac

sleep 30
