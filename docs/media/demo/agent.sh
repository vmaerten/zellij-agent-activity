#!/usr/bin/env bash
# One agent session for the README demo. The commands and their output are real;
# what is staged is the hook events, which a live agent would send instead. Each
# pane reports with its own $ZELLIJ_PANE_ID, so no id is guessed from outside.
set -u

# The tape answers zellij's permission prompt with keystrokes; without this they
# would echo into whichever pane has focus.
stty -echo 2>/dev/null || true

report() {
  local args="pane_id=${ZELLIJ_PANE_ID},hook_event=$1,ts_ms=$(($(date +%s) * 1000))"
  [ -n "${2:-}" ] && args="${args},tool_name=$2"
  [ -n "${3:-}" ] && args="${args},notification=$3"
  zellij pipe --name agent_activity.v1 --args "$args" >/dev/null 2>&1 &
  sleep 0.05
}

# Real starship, real cwd, real branch — only the typing is scripted. Without
# STARSHIP_SHELL it guesses the parent shell and wraps the prompt in that shell's
# escapes, which print literally here as `%{%}`.
export STARSHIP_SHELL=bash
# `\[` / `\]` mark non-printing runs for PS1; printed as-is they show literally.
prompt() { starship prompt --status=0 2>/dev/null | sed $'s/\\\\\\[//g; s/\\\\\\]//g'; }

# Show a tool call the way an agent would make it, then actually make it.
call() {
  local tool=$1 pause=$2
  shift 2
  prompt
  printf ' %s\n' "$*"
  report PreToolUse "$tool"
  sleep 0.4
  "$@" 2>&1
  sleep "$pause"
  report PostToolUse
}

think() { sleep "$1"; }

# Let the tape grant permissions before the first event lands.
sleep 5
report SessionStart
sleep 1.2
report UserPromptSubmit

case "${1:-work}" in
  work)
    think 1.2
    call Read 1.0 eza --tree --level=2 src docs
    think 0.6
    call Bash 1.4 git log --oneline --no-decorate -3
    think 0.6
    call Edit 1.6 git diff --stat HEAD~1
    think 0.5
    report Stop
    ;;
  busy)
    # Keeps working the whole time: this is the pane that is *not* blocked, and
    # the tab still shows the other one's warning. Output stays narrow — this
    # pane is half a screen wide.
    think 1.0
    call Bash 2.2 git status --short --branch
    think 0.5
    call Read 2.2 eza -1 src docs
    think 0.5
    call Bash 3.0 git log --oneline --no-decorate -2
    ;;
  blocked)
    # An agent asks *before* running, so the command is proposed and never
    # executed — which is also why nothing here touches the network.
    think 1.4
    prompt
    printf ' %s\n' "git push origin main"
    report PreToolUse Bash
    sleep 1.2
    printf '\n \033[38;5;215m%s\033[0m\n' "Allow this command? (y/n)"
    report Notification "" permission
    ;;
  done)
    think 1.0
    call Read 0.8 eza --tree --level=1 docs
    report Stop
    ;;
esac

sleep 60
