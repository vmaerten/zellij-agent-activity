#!/usr/bin/env bash
# Codex hook → zellij pipe bridge. Registered by hooks/hooks.json.

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
    permission_mode: .permission_mode, keys: keys
  }' >>"$ZELLIJ_AGENT_ACTIVITY_LOG" 2>/dev/null
fi

[ -z "$HOOK_EVENT" ] && exit 0

NOTIFICATION=""
case "$HOOK_EVENT" in
  # Codex has no idle nudge: this event only fires on the approval path, so it
  # always means the user (or the guardian) is being asked. ADR-0007's Claude-side
  # wording rule has nothing to disambiguate here.
  PermissionRequest)
    HOOK_EVENT="Notification"
    NOTIFICATION="permission"
    TOOL_NAME=""
    ;;
esac

# Codex names its tools its own way; the wire carries the canonical vocabulary and
# the producer translates into it, so the wasm never grows a harness branch
# (ADR-0010). Unlisted names ride through and render as the generic tool symbol.
# Only PreToolUse is translated — it is the one event whose tool_name the wire uses.
if [ "$HOOK_EVENT" = "PreToolUse" ]; then
  case "$TOOL_NAME" in
    apply_patch) TOOL_NAME="Edit" ;;
    spawn_agent) TOOL_NAME="Agent" ;;
    web_search) TOOL_NAME="WebSearch" ;;
    view_image) TOOL_NAME="Read" ;;
    # An agent stopping to ask you a question is the very thing ⚠ exists for. Its
    # PostToolUse carries the answer and lands as `thinking`, which clears it.
    request_user_input)
      HOOK_EVENT="Notification"
      NOTIFICATION="permission"
      TOOL_NAME=""
      ;;
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
#
# Shorter than the Claude producer's 5s on purpose: Codex does not run hooks
# asynchronously yet, so every event stalls the agent for however long this
# takes. A consumed pipe answers in milliseconds; 2s only ever caps the case
# where the Zellij plugin was never loaded.
zellij pipe --name agent_activity.v1 --args "$ARGS" &
pipe_pid=$!
( sleep 2; kill "$pipe_pid" 2>/dev/null ) &
watchdog_pid=$!
wait "$pipe_pid" 2>/dev/null
kill "$watchdog_pid" 2>/dev/null
wait "$watchdog_pid" 2>/dev/null
exit 0
