# zellij-agent-activity — Claude Code producer

Reports what Claude Code is doing to the
[`zellij-agent-activity`](https://github.com/vmaerten/zellij-agent-activity) Zellij plugin, which
turns it into a symbol in front of the tab name — `⚡` running a command, `✎` editing, `⚠` waiting
for you, `✓` done.

This half is only the **producer**. It does nothing on its own: install the Zellij plugin too. For
Codex see [`producers/codex`](../codex), for opencode [`producers/opencode`](../opencode) — instead
of this one, or as well, they coexist.

```sh
claude plugin marketplace add vmaerten/zellij-agent-activity
claude plugin install zellij-agent-activity@zellij-agent-activity
```

Hooks are read at launch, so start a **new** Claude session afterwards.

## What it does

`hooks/hooks.json` registers `scripts/forwarder.sh` on seven events — `SessionStart`,
`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Notification`, `Stop`, `SessionEnd` — and the
script forwards each one to the plugin:

```sh
zellij pipe --name agent_activity.v1 --args "pane_id=3,hook_event=PreToolUse,tool_name=Bash,ts_ms=…"
```

Outside Zellij (no `$ZELLIJ_PANE_ID`), or without `jq`, it exits 0 immediately.

`SubagentStop` is **not** registered, on purpose: a subagent finishing says nothing about the agent
that owns the pane, which may well be mid-tool or blocked. Only the main agent's `Stop` ends the
turn — see ADR-0007.

The one piece of judgement here is `Notification`, which Claude fires both for a permission prompt
and for an idle nudge after about a minute without input. The forwarder tells them apart and tags
the message `notification=permission|idle`; unknown wording counts as `permission`, because a
spurious `⚠` costs far less than swallowing a real one. The plugin decides what to do with it.

Requires `bash`, `jq` and `zellij` on the `PATH`.

## Debugging

```sh
export ZELLIJ_AGENT_ACTIVITY_LOG=~/.local/state/zellij-agent-activity/events.jsonl
```

One JSON object per event, appended as it happens. `tool_input` is intentionally **not** logged —
it can be large and can contain secrets.

To see the args without piping anything to Zellij:

```sh
printf '{"hook_event_name":"PreToolUse","tool_name":"Bash"}' \
  | ZELLIJ_SESSION_NAME=x ZELLIJ_PANE_ID=7 ZELLIJ_AGENT_ACTIVITY_DRY_RUN=1 bash scripts/forwarder.sh
```

`scripts/test-forwarder.sh` runs the normalization assertions on top of that flag.
