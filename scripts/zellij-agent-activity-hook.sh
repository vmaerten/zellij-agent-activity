#!/usr/bin/env bash
# zellij-agent-activity-hook.sh — Claude Code hook → zellij pipe bridge.
# Forwards Claude hook events to the zellij-agent-activity plugin, which shows the
# activity as a prefix on the tab (via zellij-tab-namer). Minimal forwarder only
# (ADR-0003): no desktop notifications, no bell.
#
# Register in ~/.claude/settings.json (the plugin's installer does this for you):
#   "command": "${HOME}/.config/zellij/plugins/zellij-agent-activity-hook.sh"

# Do nothing outside a Zellij session — there is no pane to decorate.
[ -z "$ZELLIJ_SESSION_NAME" ] && exit 0
[ -z "$ZELLIJ_PANE_ID" ] && exit 0

# Stamp send-time immediately so the plugin can order events that race in through
# parallel hook subprocesses. jq is a hard dependency (macOS `date` has no ms).
command -v jq >/dev/null 2>&1 || exit 0
TS_MS=$(jq -nc 'now * 1000 | floor')

# Read the hook JSON from stdin and pull the two fields the plugin maps on.
INPUT=$(cat)
HOOK_EVENT=$(printf '%s' "$INPUT" | jq -r '.hook_event_name // empty')
TOOL_NAME=$(printf '%s' "$INPUT" | jq -r '.tool_name // empty')

# Opt-in trace of what the harness actually fired, and in what order — the one
# thing you cannot reconstruct after the fact when the tab shows a wrong symbol.
# Off (and free) unless ZELLIJ_AGENT_ACTIVITY_LOG names a file. Fields are picked,
# not dumped: `tool_input` can be huge and can carry secrets, and short lines keep
# the O_APPEND write atomic when parallel hook subprocesses log at once. `keys`
# still surfaces any field the harness has that we don't map yet.
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

# hook_event and tool_name are Claude identifiers (e.g. Bash, mcp__x__y) — no
# commas or equals — so the comma-separated --args form is safe.
ARGS="pane_id=${ZELLIJ_PANE_ID},hook_event=${HOOK_EVENT},tool_name=${TOOL_NAME},ts_ms=${TS_MS}"

# `zellij pipe` is NOT fire-and-forget: it blocks until the plugin consumes the
# message. If the plugin is absent or stuck (e.g. on its own permission prompt),
# an unbounded pipe would leak a file descriptor per hook until the zellij server
# hits EMFILE and crashes (the zellij-smart-tabs failure). So self-limit it: run
# the pipe with a ~5s watchdog that kills it if the plugin never answers. The
# plugin normally unblocks in milliseconds; this only bites when it can't.
zellij pipe --name agent_activity --args "$ARGS" &
pipe_pid=$!
( sleep 5; kill "$pipe_pid" 2>/dev/null ) &
watchdog_pid=$!
wait "$pipe_pid" 2>/dev/null
kill "$watchdog_pid" 2>/dev/null
wait "$watchdog_pid" 2>/dev/null
exit 0
