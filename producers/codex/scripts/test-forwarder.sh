#!/usr/bin/env bash
# Tests the forwarder: given a harness payload on stdin, are the pipe args right?

set -u
HOOK="$(cd "$(dirname "$0")" && pwd)/forwarder.sh"
failures=0

run() {
  printf '%s' "$1" | ZELLIJ_SESSION_NAME=test ZELLIJ_PANE_ID=7 \
    ZELLIJ_AGENT_ACTIVITY_DRY_RUN=1 ZELLIJ_AGENT_ACTIVITY_LOG= bash "$HOOK"
}

expect() {
  name=$1 json=$2
  shift 2
  got=$(run "$json")
  for want in "$@"; do
    case "$got" in
      *"$want"*) ;;
      *)
        printf 'FAIL %s\n  want substring: %s\n  got: %s\n' "$name" "$want" "$got"
        failures=$((failures + 1))
        return
        ;;
    esac
  done
  printf 'ok   %s\n' "$name"
}

expect_absent() {
  got=$(run "$2")
  case "$got" in
    *"$3"*)
      printf 'FAIL %s\n  unexpected substring: %s\n  got: %s\n' "$1" "$3" "$got"
      failures=$((failures + 1))
      ;;
    *) printf 'ok   %s\n' "$1" ;;
  esac
}

pre_tool_use() {
  printf '{"hook_event_name":"PreToolUse","tool_name":"%s"}' "$1"
}

expect 'approval request -> permission' \
  '{"hook_event_name":"PermissionRequest","tool_name":"Bash"}' \
  'hook_event=Notification' 'notification=permission'

expect 'approval request carries no tool' \
  '{"hook_event_name":"PermissionRequest","tool_name":"Bash"}' \
  'tool_name=,'

expect 'shell keeps the canonical name' \
  "$(pre_tool_use Bash)" \
  'pane_id=7' 'hook_event=PreToolUse' 'tool_name=Bash'

expect 'apply_patch -> Edit' "$(pre_tool_use apply_patch)" 'tool_name=Edit'
expect 'spawn_agent -> Agent' "$(pre_tool_use spawn_agent)" 'tool_name=Agent'
expect 'web_search -> WebSearch' "$(pre_tool_use web_search)" 'tool_name=WebSearch'
expect 'view_image -> Read' "$(pre_tool_use view_image)" 'tool_name=Read'

expect 'an untranslated tool rides through' \
  "$(pre_tool_use update_plan)" \
  'hook_event=PreToolUse' 'tool_name=update_plan'

expect 'request_user_input -> permission' \
  "$(pre_tool_use request_user_input)" \
  'hook_event=Notification' 'notification=permission'

expect_absent 'request_user_input is not reported as a tool' \
  "$(pre_tool_use request_user_input)" \
  'hook_event=PreToolUse'

expect 'the answer to request_user_input clears the warning' \
  '{"hook_event_name":"PostToolUse","tool_name":"request_user_input"}' \
  'hook_event=PostToolUse'

expect_absent 'a finished turn carries no notification field' \
  '{"hook_event_name":"Stop"}' \
  'notification='

expect_absent 'a closed session carries no notification field' \
  '{"hook_event_name":"SessionEnd","reason":"other"}' \
  'notification='

expect_absent 'event without a name emits nothing' \
  '{"tool_name":"Bash"}' \
  'pane_id='

if [ "$failures" -ne 0 ]; then
  printf '\n%d failing\n' "$failures"
  exit 1
fi
printf '\nall producer tests passed\n'
