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

notification() {
  printf '{"hook_event_name":"Notification","message":"%s"}' "$1"
}

expect 'permission prompt -> permission' \
  "$(notification 'Claude needs your permission to use Bash')" \
  'hook_event=Notification' 'notification=permission'

expect 'idle nudge -> idle' \
  "$(notification 'Claude is waiting for your input')" \
  'notification=idle'

expect 'unknown wording -> permission' \
  "$(notification 'Something we have never seen')" \
  'notification=permission'

expect 'absent message -> permission' \
  '{"hook_event_name":"Notification"}' \
  'notification=permission'

expect 'idle nudge is case-insensitive' \
  "$(notification 'Claude Is Waiting For Your Input')" \
  'notification=idle'

expect 'tool event carries pane, event and tool' \
  '{"hook_event_name":"PreToolUse","tool_name":"Bash"}' \
  'pane_id=7' 'hook_event=PreToolUse' 'tool_name=Bash'

expect_absent 'non-notification events carry no notification field' \
  '{"hook_event_name":"PreToolUse","tool_name":"Bash"}' \
  'notification='

expect_absent 'event without a name emits nothing' \
  '{"tool_name":"Bash"}' \
  'pane_id='

if [ "$failures" -ne 0 ]; then
  printf '\n%d failing\n' "$failures"
  exit 1
fi
printf '\nall producer tests passed\n'
