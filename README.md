# zellij-agent-activity

A small [Zellij](https://zellij.dev) plugin that shows what your AI coding agent is doing as a
symbol in front of the tab it runs in: `⚡` running a command, `✎` editing, `⚠` waiting for you,
`✓` done. In a session with a dozen tabs, you can see which agent needs you without switching to it.

The plugin is harness-neutral. Claude Code is wired up today, and Codex, Gemini CLI or opencode are
each a hook script away.

<p align="center">
  <img alt="Zellij plugin" src="https://img.shields.io/badge/zellij-plugin-8A2BE2">
  <img alt="Claude Code" src="https://img.shields.io/badge/Claude%20Code-supported-orange">
  <img alt="Built with Rust" src="https://img.shields.io/badge/built%20with-Rust-000000?logo=rust">
  <img alt="Status" src="https://img.shields.io/badge/status-alpha-yellow">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-blue">
</p>

<p align="center">
  <a href="#what-is-it">What is it?</a> ·
  <a href="#requirements">Requirements</a> ·
  <a href="#install">Install</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#activity-reference">Activity reference</a> ·
  <a href="#debugging">Debugging</a> ·
  <a href="#how-is-this-different">How is this different?</a>
</p>

<!-- Drop a screen recording here: docs/media/demo.gif -->

```
◆ starting   ● thinking   ⚡ bash   ✎ edit   ◉ read   ⊜ subagent   ◈ web   ⚠ needs you   ✓ done
```

## What is it?

AI coding agents work for long stretches, then quietly block on a permission prompt, or finish,
while you're looking at another tab. Past a handful of tabs it gets hard to keep track of which one
is waiting on you.

This plugin puts a single symbol in the tab name that follows the live activity of the agent session
running in that tab's pane.

What it deliberately doesn't do is rename your tabs. Zellij lets only one plugin own a tab's name,
and two plugins fighting over it produce flickering renames that clear your focus. So this one
computes the symbol and hands it to
[`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer) through its decoration pipe. The
namer stays the only owner of the name and wraps the symbol around it:

```
⚡ myrepo
```

No new status bar, no extra column, no rename war. Your existing tab name, with a live prefix.

## Requirements

- Zellij 0.44.3 or later (`zellij --version`).
- [`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer) loaded. This plugin drives it
  and does nothing on its own; a standalone mode is on the roadmap.
- `jq` and bash, used by the forwarder that reports the agent's events.
- Claude Code 2.1.120 or later, the source of those events. Its producer installs through
  `claude plugin install`.

| Plugin | `zellij-tile` | Zellij tested | Pipe protocol |
|---|---|---|---|
| 0.1.x | 0.44.3 | 0.44.3 | `agent_activity.v1` |

A plugin is wasm compiled against `zellij-tile`, and that pins the ABI the Zellij host expects, so a
newer Zellij *minor* may need a rebuild. The full matrix, the policy and the migration notes are in
[`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md).

## Install

```sh
# Download the wasm from the latest release into your plugins dir
curl -L -o ~/.config/zellij/plugins/zellij-agent-activity.wasm \
  https://github.com/vmaerten/zellij-agent-activity/releases/latest/download/zellij-agent-activity.wasm
```

> Rather build it yourself? `cargo wasm` produces the same file, see
> [Development](#development). Drop it into `~/.config/zellij/plugins/`.

Load it alongside the namer in `~/.config/zellij/config.kdl`:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/zellij-tab-namer.wasm";
    "file:~/.config/zellij/plugins/zellij-agent-activity.wasm";
}
```

Restart Zellij and grant the plugin's permissions when prompted (`ReadApplicationState`,
`MessageAndLaunchOtherPlugins`, `ReadCliPipes`).

Then install the producer, the piece that reports what your agent is doing. For Claude Code it ships
as a Claude Code plugin, from this same repo:

```sh
claude plugin marketplace add vmaerten/zellij-agent-activity
claude plugin install zellij-agent-activity@zellij-agent-activity
```

Start a new Claude session, since Claude reads its hooks at launch, and the tab prefix starts moving.

> `owner/repo` clones over SSH, so the first line fails if you don't have a GitHub key loaded. Use
> the URL form instead
> (`claude plugin marketplace add https://github.com/vmaerten/zellij-agent-activity.git`), or set
> `CLAUDE_CODE_PLUGIN_PREFER_HTTPS=1`.

> Two steps, on purpose: the producer and the consumer really are two processes in two different
> tools. Claude Code owns the merge into your `settings.json`, so nothing here ever writes to a
> config file it doesn't own, and uninstalling is `claude plugin uninstall`. See
> [`docs/adr/0005-producer-per-harness-native-distribution.md`](docs/adr/0005-producer-per-harness-native-distribution.md).

> Replacing [`zellij-attention`](https://github.com/KiryuuLight/zellij-attention)? Remove it from
> `load_plugins` *and* delete its hook entries from `~/.claude/settings.json`. Hooks left piping to
> an unloaded plugin can block. This plugin's producer replaces it.

## How it works

```
Claude Code hook            producers/claude/forwarder.sh        zellij-agent-activity (wasm)
(Claude Code plugin)      ─►  $ZELLIJ_PANE_ID + event + ts  ─►   pane → tab · event → symbol
   PreToolUse/Bash          zellij pipe --name agent_activity.v1  highest-priority per tab
                                                                          │
                                                          pipe_message_to_plugin("set_prefix")
                                                                  routed by name, by tab_id
                                                                          ▼
                                              zellij-tab-namer  ─►  tab renders  "⚡ myrepo"
```

A plugin is the only thing that can see both the tab list and the pane manifest, so mapping the
reporting `pane_id` onto a stable `tab_id` is the one job a shell hook can't do. That mapping is
what this plugin adds. The symbol, the wrapping and the name itself are left to the namer.

### The wire format

Everything between a harness and the plugin goes through a single `zellij pipe`. This is the whole
contract, and it's enough to write a producer for any other agent.

| arg | meaning |
|---|---|
| `pane_id` | `$ZELLIJ_PANE_ID` of the pane the agent runs in. Required |
| `hook_event` | normalized event name (`SessionStart`, `PreToolUse`, `Stop`, …). Required |
| `tool_name` | tool being invoked, for `PreToolUse` |
| `ts_ms` | send time in ms, so events racing through parallel hooks stay ordered |
| `notification` | for `Notification` only: `permission` (needs the user) or `idle` (just a nudge) |

```sh
zellij pipe --name agent_activity.v1 --args "pane_id=3,hook_event=PreToolUse,tool_name=Bash,ts_ms=…"
```

The producer normalizes and the plugin decides. Harnesses word things differently: Claude says
"Claude needs your permission", opencode raises `permission.asked`. Translating that into the values
above is the producer's job, which keeps harness vocabulary out of the plugin entirely. That's why
supporting a new harness means writing a script rather than touching the wasm. Unknown values
degrade safely: an unrecognized `hook_event` leaves the pane alone, an unknown `tool_name` renders
`⚙`, and anything unexpected in `notification` counts as needing you rather than silently dropping
a `⚠`.

Cleanup is purely event-driven. A pane's state clears on `SessionEnd` and is garbage-collected when
the pane closes. No timers, no wakeups. A crash leaves a stale prefix that heals itself on the next
prompt.

Design decisions are recorded as ADRs in [`docs/adr/`](docs/adr).

## Activity reference

| Claude hook event | Meaning | Symbol |
|---|---|---|
| `SessionStart` | starting up | `◆` |
| `UserPromptSubmit`, `PostToolUse` | thinking | `●` |
| `PreToolUse` · `Bash` | running a command | `⚡` |
| `PreToolUse` · `Edit` / `Write` / `MultiEdit` | editing | `✎` |
| `PreToolUse` · `Read` / `Glob` / `Grep` | reading | `◉` |
| `PreToolUse` · `Agent` / `Task` | subagent | `⊜` |
| `PreToolUse` · `WebSearch` / `WebFetch` | web | `◈` |
| `PreToolUse` · other / MCP | tool | `⚙` |
| `Notification` · permission prompt | needs you | `⚠` |
| `Notification` · idle nudge | ignored, see below | |
| `Stop` | done | `✓` |
| `SubagentStop` | ignored, see below | |
| `SessionEnd` | clears the prefix | |

When a tab holds several Claude panes, the highest-priority state wins
(`⚠ waiting > tool > ● thinking > ◆ init > ✓ done`), so a pending permission request is never hidden
behind a background pane's activity.

`⚠` means blocked, never idle. Claude fires `Notification` for two different things: a permission
prompt, and an idle nudge after about a minute without input, which lands mid-tool just as readily
as after a finished turn. Treating both as `⚠` meant every tab you left alone drifted to `⚠`, and
the symbol stopped being worth acting on. So the hook tells them apart and the plugin ignores the
nudge entirely. An agent that genuinely needs you ends its turn to ask, so the signal comes through
as a permission prompt anyway.

`SubagentStop` isn't treated as "done" either, and that's deliberate: a subagent finishing says
nothing about the agent that owns the pane, which may well be mid-tool or blocked. Only the main
agent's `Stop` ends the turn. See
[`docs/adr/0007-producer-normalizes-core-decides.md`](docs/adr/0007-producer-normalizes-core-decides.md).

## Debugging

A wrong symbol on a tab has exactly two possible causes, and they need different tools: either the
harness never fired the event you expected, or it did and the plugin decided something else. Both
traces are opt-in and off by default.

**1. What the harness fired.** Point the hook at a log file, in the shell you launch Claude from:

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

**2. What the plugin decided.** Load it with `debug true` in `~/.config/zellij/config.kdl`:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/zellij-tab-namer.wasm";
    "file:~/.config/zellij/plugins/zellij-agent-activity.wasm" {
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

> **Upgrading?** The two halves upgrade separately. For the consumer, `curl` the new wasm and
> restart Zellij. For the producer, run
> `claude plugin update zellij-agent-activity@zellij-agent-activity` and start a new Claude session.
> They only need to agree on the pipe protocol major (`agent_activity.v1`), so a version drift
> within a major is harmless. See [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md).

## How is this different?

| Tool | What it is | How `zellij-agent-activity` differs |
|---|---|---|
| [zellij-attention](https://github.com/KiryuuLight/zellij-attention) | Binary `⏳`/`✅` by renaming the tab itself | Richer per-tool states, and it drives the namer instead of fighting it for `TabInfo.name` |
| [zj-radar](https://github.com/marktoda/zj-radar) | A left sidebar rail (that also renames tabs) | No new UI and no name ownership, just a prefix on your existing tab, through a dedicated namer |
| [zjstatus](https://github.com/dj95/zjstatus) | The status bar | Leaves your bar alone and decorates the tab name only |

In short: one plugin owns the tab name, and everything else decorates it.

## Development

```sh
cargo test    # pure core, host-native, no zellij needed
cargo wasm    # release build -> target/wasm32-wasip1/release/zellij-agent-activity.wasm
```

The plugin is a pure state machine, events in and effects out, with a thin wasm-gated adapter that
runs those effects against the Zellij host. The pane to tab routing and the priority aggregation are
therefore exercised as ordinary unit tests, never in a live session. See
[`docs/adr/0004-effects-seam-and-sink-abstraction.md`](docs/adr/0004-effects-seam-and-sink-abstraction.md).

## Credits

- Event mapping and hook patterns adapted from
  [`ishefi/zellaude`](https://github.com/ishefi/zellaude).
- Prior art and the operational lessons behind the `zellij pipe` back-pressure guard, from
  [`marktoda/zj-radar`](https://github.com/marktoda/zj-radar).
- Built to pair with [`vmaerten/zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer).

## License

MIT.
