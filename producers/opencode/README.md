# zellij-agent-activity — opencode producer

Reports what [opencode](https://opencode.ai) is doing to the
[`zellij-agent-activity`](https://github.com/vmaerten/zellij-agent-activity) Zellij plugin, which
turns it into a symbol in front of the tab name — `⚡` running a command, `✎` editing, `⚠` waiting
for you, `✓` done.

This half is only the **producer**. It does nothing on its own: install the Zellij plugin too.

```sh
mkdir -p ~/.config/opencode/plugins
curl -fsSL -o ~/.config/opencode/plugins/zellij-agent-activity.js \
  https://raw.githubusercontent.com/vmaerten/zellij-agent-activity/main/producers/opencode/zellij-agent-activity.js
```

Files in that directory are loaded at startup, so start a **new** opencode session afterwards. There
is nothing to add to `opencode.json`, and nothing to approve. Upgrading is the same `curl` again.

Working from a clone instead? Symlink it, and the file tracks your checkout:

```sh
ln -s "$PWD/producers/opencode/zellij-agent-activity.js" ~/.config/opencode/plugins/
```

For one project only, use `.opencode/plugins/` in that project instead of the global directory.

## What it does

opencode has no command hooks: its extension point is a program loaded into the opencode server. So
this producer is a single self-contained `.js` file rather than the `forwarder.sh` the Claude Code
and Codex producers use — it spawns `zellij pipe` itself, with a 5s cap and without waiting for it,
so reporting never slows the agent down:

```sh
zellij pipe --name agent_activity.v1 --args "pane_id=3,hook_event=PreToolUse,tool_name=Bash,ts_ms=…"
```

Outside Zellij (no `$ZELLIJ_PANE_ID`), it registers no hooks at all.

The pane comes from the environment. Each opencode TUI starts its own server as a child process,
which inherits the pane it was launched in, so this is the pane you are looking at. The exception is
a standalone `opencode serve` with a client attached from elsewhere: the activity then lands on the
pane the *server* was started in, or nowhere if that was outside Zellij.

## Subagents stay silent

opencode has no turn boundary that tells the main agent from a subagent — the `task` tool spawns a
child session, and that session's `session.idle` would post a premature `✓` while the main agent is
still working. So the producer keeps the set of session ids it has seen born with a `parentID`, and
drops everything they emit. A session it never saw being born is the resumed root one, and is
reported. That makes this the only producer with state; see ADR-0011 and ADR-0007.

## The mapping

| opencode | wire | symbol |
|---|---|---|
| `session.created` (no `parentID`) | `SessionStart` | `◆` |
| `chat.message` | `UserPromptSubmit` | `●` |
| `tool.execute.before` | `PreToolUse` + translated `tool` | per tool |
| `tool.execute.after` | `PostToolUse` | `●` |
| `permission.asked` | `Notification` · `notification=permission` | `⚠` |
| `permission.replied` | `PostToolUse` | `●` |
| `session.idle` | `Stop` | `✓` |
| `dispose` | `SessionEnd` | clears the prefix |

opencode has no idle nudge, the ambiguity that cost the Claude producer a whole ADR: `Permission.ask`
returns before publishing anything when its allow rules already cover the request, so
`permission.asked` only ever fires when someone is genuinely being asked.

Loading the plugin deliberately sends **nothing**. opencode instantiates it more than once, and an
instance born during shutdown would leave a `◆` behind after the `dispose` that cleared the pane.
The cost is that a resumed session (`opencode --continue`) shows no `◆` until your first prompt.

The second translation is the tool name. The wire carries a canonical vocabulary and each producer
translates into it, so supporting a harness never means rebuilding the wasm — see ADR-0010:

| opencode tool | wire `tool_name` | symbol |
|---|---|---|
| `bash` | `Bash` | `⚡` |
| `edit`, `apply_patch` | `Edit` | `✎` |
| `write` | `Write` | `✎` |
| `read` | `Read` | `◉` |
| `glob` | `Glob` | `◉` |
| `grep` | `Grep` | `◉` |
| `task` | `Agent` | `⊜` |
| `webfetch` | `WebFetch` | `◈` |
| `websearch` | `WebSearch` | `◈` |
| `question` | `Notification` · `notification=permission` | `⚠` |
| anything else (`todowrite`, `skill`, MCP, …) | unchanged | `⚙` |

`question` is the agent stopping to ask you something outside the approval path, so it gets the same
`⚠` — and its `tool.execute.after` carries the answer, which clears it.

Those are opencode's runtime tool ids, which are not always the names its docs use. The list a given
build actually registers comes from the server itself:

```sh
opencode serve --port 4097 &
curl -s http://127.0.0.1:4097/experimental/tool/ids
```

Requires `zellij` on the `PATH`. Unlike the shell producers, no `jq` and no `bash`.

## Debugging

```sh
export ZELLIJ_AGENT_ACTIVITY_LOG=~/.local/state/zellij-agent-activity/events.jsonl
```

One JSON object per hook, appended as it happens: the opencode `source` that fired it, the raw
`tool` name, the `session_id`, whether it was `dropped` as a subagent's, and the `args` that went on
the wire. Tool arguments are intentionally **not** logged — they can be large and can contain
secrets.

A line carrying `failed` is a `zellij pipe` that did not exit cleanly, so the message never reached
the plugin. That is what to grep for when the producer looks busy and the tab does not move.

To see the args without piping anything to Zellij, set `ZELLIJ_AGENT_ACTIVITY_DRY_RUN=1` before
launching opencode: each line is printed instead of sent.

`zellij-agent-activity.test.js` drives the real hooks through that same flag —
`node --test producers/opencode/zellij-agent-activity.test.js`.
