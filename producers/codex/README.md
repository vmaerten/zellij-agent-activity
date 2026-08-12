# zellij-agent-activity: Codex producer

Reports what [Codex](https://github.com/openai/codex) is doing to the
[`zellij-agent-activity`](https://github.com/vmaerten/zellij-agent-activity) Zellij plugin, which
turns it into a symbol in front of the tab name: `⚡` running a command, `✎` editing, `⚠` waiting
for you, `✓` done.

This half is only the **producer**. It does nothing on its own: install the Zellij plugin too. For
Claude Code see [`producers/claude`](../claude), for opencode [`producers/opencode`](../opencode),
instead of this one, or as well, they coexist.

```sh
codex plugin marketplace add vmaerten/zellij-agent-activity
codex plugin add zellij-agent-activity@zellij-agent-activity
```

Hooks are read at launch, so start a **new** Codex session afterwards, and **approve the hooks**
when Codex asks. Plugin hooks arrive untrusted, so they stay inert until you accept them once.

That review only happens in the interactive TUI. `codex exec` skips untrusted hooks in silence, so
run `codex` once and approve before expecting anything from a headless run.

## What it does

`hooks/hooks.json` registers `scripts/forwarder.sh` on seven events (`SessionStart`,
`UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `PostToolUse`, `Stop`, `SessionEnd`) and the
script forwards each one to the plugin:

```sh
zellij pipe --name agent_activity.v1 --args "pane_id=3,hook_event=PreToolUse,tool_name=Bash,ts_ms=…"
```

Outside Zellij (no `$ZELLIJ_PANE_ID`), or without `jq`, it exits 0 immediately.

Codex runs hooks **synchronously** (`"async": true` is parsed but skipped with a warning as of
0.146.0) so every event costs the agent whatever the forwarder takes. That is milliseconds when the
Zellij plugin is loaded and consuming the pipe. When it isn't, `zellij pipe` blocks instead, which is
why the watchdog here is 2s rather than the Claude producer's 5s, with Codex's own `timeout` at 3s
behind it. If Codex feels sluggish, that is the symptom of a producer installed without its consumer.

`SubagentStart` and `SubagentStop` are **not** registered, on purpose: a subagent says nothing about
the agent that owns the pane, which may well be mid-tool or blocked. Only the main agent's `Stop`
ends the turn, see ADR-0007. `PreCompact`/`PostCompact` are left out for the same reason: they
describe the context window, not the activity.

## The two translations

Codex has no idle nudge, which is the ambiguity that cost the Claude producer a whole ADR:
`PermissionRequest` only fires on the approval path, so it always means someone is being asked.
It becomes the wire's `Notification` with `notification=permission`:

| Codex event | wire | symbol |
|---|---|---|
| `SessionStart` | `SessionStart` | `◆` |
| `UserPromptSubmit` | `UserPromptSubmit` | `●` |
| `PreToolUse` | `PreToolUse` + translated `tool_name` | per tool |
| `PermissionRequest` | `Notification` · `notification=permission` | `⚠` |
| `PostToolUse` | `PostToolUse` | `●` |
| `Stop` | `Stop` | `✓` |
| `SessionEnd` | `SessionEnd` | clears the prefix |

The second translation is the tool name. The wire carries a canonical vocabulary and each producer
translates into it, so supporting a harness never means rebuilding the wasm, see ADR-0010:

| Codex tool | wire `tool_name` | symbol |
|---|---|---|
| `Bash` (shell, unified exec) | `Bash` | `⚡` |
| `apply_patch` | `Edit` | `✎` |
| `spawn_agent` | `Agent` | `⊜` |
| `web_search` | `WebSearch` | `◈` |
| `view_image` | `Read` | `◉` |
| `request_user_input` | `Notification` · `notification=permission` | `⚠` |
| anything else (`update_plan`, MCP, …) | unchanged | `⚙` |

`request_user_input` is the agent stopping to ask you something outside the approval path, so it
gets the same `⚠`, and its `PostToolUse` carries the answer, which clears it.

`◉` is rare here: Codex has no read tool, it reads files by running shell commands.

Requires `bash`, `jq` and `zellij` on the `PATH`.

## Debugging

```sh
export ZELLIJ_AGENT_ACTIVITY_LOG=~/.local/state/zellij-agent-activity/events.jsonl
```

One JSON object per event, appended as it happens. `tool_input` is intentionally **not** logged:
it can be large and can contain secrets.

To see the args without piping anything to Zellij:

```sh
printf '{"hook_event_name":"PreToolUse","tool_name":"apply_patch"}' \
  | ZELLIJ_SESSION_NAME=x ZELLIJ_PANE_ID=7 ZELLIJ_AGENT_ACTIVITY_DRY_RUN=1 bash scripts/forwarder.sh
```

`scripts/test-forwarder.sh` runs the normalization assertions on top of that flag.
