#!/usr/bin/env bash
# Tests the producer half of the contract: given a harness payload on stdin, does
# the forwarder emit the right pipe args? The core is tested by `cargo test`; this
# covers the one piece of logic that lives in shell — normalizing `Notification`
# into `permission` / `idle` (ADR-0007). Every future producer gets one of these.
#
# Run directly or via `task ci`.

set -u
HOOK="$(cd "$(dirname "$0")" && pwd)/zellij-agent-activity-hook.sh"
failures=0

# A fake Zellij environment (the hook exits early outside a session) plus the
# dry-run switch, which prints the args instead of piping them to a plugin.
run() {
  printf '%s' "$1" | ZELLIJ_SESSION_NAME=test ZELLIJ_PANE_ID=7 \
    ZELLIJ_AGENT_ACTIVITY_DRY_RUN=1 ZELLIJ_AGENT_ACTIVITY_LOG= bash "$HOOK"
}

# expect <name> <json> <substring…> — every substring must appear in the output.
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

# expect_absent <name> <json> <substring> — the substring must NOT appear.
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

# The fallback direction is deliberate: unknown wording must not silently
# downgrade a real "come unblock me" into something the core ignores.
expect 'unknown wording -> permission' \
  "$(notification 'Something we have never seen')" \
  'notification=permission'

expect 'absent message -> permission' \
  '{"hook_event_name":"Notification"}' \
  'notification=permission'

# Casing is not part of the contract — Claude could capitalize differently.
expect 'idle nudge is case-insensitive' \
  "$(notification 'Claude Is Waiting For Your Input')" \
  'notification=idle'

expect 'tool event carries pane, event and tool' \
  '{"hook_event_name":"PreToolUse","tool_name":"Bash"}' \
  'pane_id=7' 'hook_event=PreToolUse' 'tool_name=Bash'

# Only `Notification` gets the field — anything else would make the core's
# tolerance branch unreachable and hide a producer bug.
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
