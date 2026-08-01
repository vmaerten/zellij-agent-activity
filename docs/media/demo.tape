# README demo. Regenerate with:  task wasm && vhs docs/media/demo.tape
#
# The commands and their output are real — a clone of this repo in a throwaway
# $HOME, real starship prompt, real git. What is staged is the hook events, which
# a live agent would send instead (docs/media/demo/agent.sh). The decoration is
# the plugin itself, in `mode "rename"` so the demo needs nothing else installed.

Output docs/media/demo.gif

Set Shell bash
Set FontFamily "JetBrainsMono Nerd Font"
Set FontSize 18
Set Width 1200
Set Height 520
Set Padding 20
Set Theme "Catppuccin Frappe"
Set Framerate 20

Hide
Type "docs/media/demo/run.sh" Enter
Sleep 3s
# Grant the plugin's permissions. Harmless when already cached: the panes run
# with echo off, so the keystroke leaves no trace.
Type "y"
Sleep 3500ms
Show

# `api` works a turn: read, bash, edit. Meanwhile `web` goes to ⚠ and `infra`
# finishes — visible in the tab bar without leaving this tab, which is the pitch.
Sleep 9s

# Go see why `web` warns: two agents in it, and the one on the left is still
# working. The tab shows the blocked one anyway — a background task never buries
# the one that wants you.
Ctrl+t
Sleep 400ms
Type "2"
Escape
Sleep 5s

Ctrl+t
Sleep 400ms
Type "1"
Escape
Sleep 2500ms
