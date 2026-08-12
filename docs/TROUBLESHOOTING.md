# Troubleshooting

**Nothing happens at all?** Check `mode` first: it is mandatory, and a missing or misspelled value
makes the plugin do nothing on purpose rather than guess. It always says so, no `debug` needed:

```sh
grep zellij-agent-activity "${TMPDIR:-/tmp}/zellij-$(id -u)/zellij-log/zellij.log"
```

A *wrong* symbol on a tab has two possible causes, and they need different tools: either the harness
never fired the event you expected, or it did and the plugin decided something else. Both traces are
opt-in and off by default.

## 1. What the harness fired

Point the hook at a log file, in the shell you launch the agent from:

```sh
export ZELLIJ_AGENT_ACTIVITY_LOG=~/.local/state/zellij-agent-activity/events.jsonl
```

One JSON object per event, appended as it happens:

```jsonc
{"at":"2026-07-24T20:37:16Z","ts_ms":1784925436039,"pane_id":"3","hook_event":"PreToolUse",
 "tool":"Bash","session_id":"e14716c9…","transcript":"…/subagents/agent-a67d44….jsonl", …}
```

`transcript` tells main-agent events from subagent ones, since subagents get their own transcript
file, and `keys` lists every field the payload carried. `tool_input` is deliberately not logged: it
can be large, and it can contain secrets.

Every producer honours that variable. The opencode one logs its own fields, including whether an
event was dropped as a subagent's, see
[`producers/opencode/README.md`](../producers/opencode/README.md).

## 2. What the plugin decided

Load it with `debug true` in `~/.config/zellij/config.kdl`:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/zellij-tab-namer.wasm";
    "file:~/.config/zellij/plugins/zellij-agent-activity.wasm" {
        mode "pipe"
        debug true
    }
}
```

Every pipe received, every pane to tab mapping, every event mapped, ignored or dropped along with
the reason, and every prefix emitted goes to Zellij's own log:

```sh
tail -f "${TMPDIR:-/tmp}/zellij-$(id -u)/zellij-log/zellij.log" | grep zellij-agent-activity
```

```
[zellij-agent-activity] pane 3 (tab 2): PreToolUse/Bash -> Tool("Bash")
[zellij-agent-activity] tab 2: prefix -> Some("⚡ ")
[zellij-agent-activity] pane 3 (tab 2): SubagentStop/ unmapped, state kept
```

## Installing the Claude Code producer fails

`owner/repo` clones over SSH **for Claude Code**, so `claude plugin marketplace add` fails if you
don't have a GitHub key loaded. Use the URL form instead
(`claude plugin marketplace add https://github.com/vmaerten/zellij-agent-activity.git`), or set
`CLAUDE_CODE_PLUGIN_PREFER_HTTPS=1`. Codex clones over HTTPS and needs neither.

## Why two installs, plugin and producer

The producer and the consumer really are two processes in two different tools. Each harness owns the
merge into its own config, so nothing here ever writes to a file it doesn't own, and uninstalling is
`claude plugin uninstall`, `codex plugin remove`, or deleting the one opencode file. See
[ADR-0005](adr/0005-producer-per-harness-native-distribution.md).

## Upgrading

The halves upgrade separately. For the consumer, `curl` the new wasm and restart Zellij. For a
producer, run `claude plugin update zellij-agent-activity@zellij-agent-activity`,
`codex plugin marketplace upgrade zellij-agent-activity`, or the same `curl` again for opencode,
then start a new session. They only need to agree on the pipe protocol major (`agent_activity.v1`),
so a version drift within a major is harmless. See [`COMPATIBILITY.md`](COMPATIBILITY.md).

## Replacing zellij-attention

Remove [`zellij-attention`](https://github.com/KiryuuLight/zellij-attention) from `load_plugins`
*and* delete its hook entries from `~/.claude/settings.json`. Hooks left piping to an unloaded plugin
can block. This plugin's producer replaces it.
