# README demo. Regenerate with:  vhs docs/media/demo.tape
# Needs the wasm built first:     task wasm
#
# The session is staged, not a recording of a live agent: each pane reports its
# own activity on the pipe (docs/media/demo/agent.sh). What you see decorating
# the tabs is the real plugin, in `mode "rename"` so the demo needs nothing else.

Output docs/media/demo.gif

Set Shell bash
Set FontFamily "JetBrainsMono Nerd Font"
Set FontSize 20
Set Width 1100
Set Height 460
Set Padding 20
Set Theme "Catppuccin Frappe"
Set Framerate 20
Set PlaybackSpeed 1.0

Hide
Type "docs/media/demo/run.sh" Enter
Sleep 3s
# Grant the plugin's permissions. Harmless when they are already cached: the
# panes run with echo off, so the keystroke leaves no trace.
Type "y"
Sleep 2500ms
Show

# `api` works through a turn: read, bash, edit, bash. Meanwhile `web` blocks on a
# permission and `infra` finishes — the whole point is seeing that from here.
Sleep 12s

# Ctrl+t is tab mode; the digit picks the tab, Escape leaves the mode.
Ctrl+t
Sleep 400ms
Type "2"
Escape
Sleep 3s

Ctrl+t
Sleep 400ms
Type "1"
Escape
Sleep 4s
