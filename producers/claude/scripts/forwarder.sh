#!/usr/bin/env bash
# Claude Code hook → zellij pipe bridge. Registered by hooks/hooks.json.

[ -z "$ZELLIJ_SESSION_NAME" ] && exit 0
[ -z "$ZELLIJ_PANE_ID" ] && exit 0

command -v jq >/dev/null 2>&1 || exit 0
TS_MS=$(jq -nc 'now * 1000 | floor')

INPUT=$(cat)
HOOK_EVENT=$(printf '%s' "$INPUT" | jq -r '.hook_event_name // empty')
TOOL_NAME=$(printf '%s' "$INPUT" | jq -r '.tool_name // empty')

# Never dump the payload: `tool_input` can hold secrets, and short lines keep the
# append atomic across parallel hook subprocesses.
if [ -n "$ZELLIJ_AGENT_ACTIVITY_LOG" ]; then
  mkdir -p "$(dirname "$ZELLIJ_AGENT_ACTIVITY_LOG")" 2>/dev/null
  printf '%s' "$INPUT" | jq -c --argjson ts "$TS_MS" --arg pane "$ZELLIJ_PANE_ID" '{
    at: ($ts / 1000 | todate), ts_ms: $ts, pane_id: $pane,
    hook_event: .hook_event_name, tool: .tool_name,
    session_id: .session_id, transcript: .transcript_path,
    message: .message, permission_mode: .permission_mode, keys: keys
  }' >>"$ZELLIJ_AGENT_ACTIVITY_LOG" 2>/dev/null
fi

[ -z "$HOOK_EVENT" ] && exit 0

NOTIFICATION=""
if [ "$HOOK_EVENT" = "Notification" ]; then
  MESSAGE=$(printf '%s' "$INPUT" | jq -r '(.message // "") | ascii_downcase')
  case "$MESSAGE" in
    *"waiting for your input"*) NOTIFICATION="idle" ;;
    # Unknown wording means permission on purpose: a spurious ⚠ costs far less
    # than swallowing a real one.
    *) NOTIFICATION="permission" ;;
  esac
fi

ARGS="pane_id=${ZELLIJ_PANE_ID},hook_event=${HOOK_EVENT},tool_name=${TOOL_NAME},ts_ms=${TS_MS}"
[ -n "$NOTIFICATION" ] && ARGS="${ARGS},notification=${NOTIFICATION}"

if [ -n "$ZELLIJ_AGENT_ACTIVITY_DRY_RUN" ]; then
  printf '%s\n' "$ARGS"
  exit 0
fi

# `zellij pipe` blocks until the plugin consumes the message, so without this
# watchdog a stuck plugin leaks a file descriptor per hook until the zellij
# server hits EMFILE and crashes.
zellij pipe --name agent_activity.v1 --args "$ARGS" &
pipe_pid=$!
( sleep 5; kill "$pipe_pid" 2>/dev/null ) &
watchdog_pid=$!
wait "$pipe_pid" 2>/dev/null
kill "$watchdog_pid" 2>/dev/null
wait "$watchdog_pid" 2>/dev/null
exit 0
