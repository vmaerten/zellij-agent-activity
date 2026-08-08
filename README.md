# zellij-agent-activity

A small [Zellij](https://zellij.dev) plugin that shows what your AI coding agent is doing as a
symbol in front of the tab it runs in: `⚡` running a command, `✎` editing, `⚠` waiting for you,
`✓` done. In a session with a dozen tabs, you can see which agent needs you without switching to it.

The plugin is harness-neutral. Claude Code and Codex are wired up today, and Gemini CLI or opencode
are each a hook script away.

<p align="center">
  <a href="https://github.com/vmaerten/zellij-agent-activity/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/vmaerten/zellij-agent-activity/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Zellij plugin" src="https://img.shields.io/badge/zellij-plugin-8A2BE2">
  <img alt="Claude Code" src="https://img.shields.io/badge/Claude%20Code-supported-orange">
  <img alt="Codex" src="https://img.shields.io/badge/Codex-supported-black">
  <img alt="Status" src="https://img.shields.io/badge/status-alpha-yellow">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-blue">
</p>

<p align="center">
  <img alt="Three agent sessions, three tabs: one working, one blocked on a permission, one done" src="docs/media/demo.gif" width="100%">
</p>

```
◆ starting   ● thinking   ⚡ bash   ✎ edit   ◉ read   ⊜ subagent   ◈ web   ⚠ needs you   ✓ done
```

## What is it?

AI coding agents work for long stretches, then quietly block on a permission prompt, or finish,
while you're looking at another tab. Past a handful of tabs it gets hard to keep track of which one
is waiting on you.

This plugin puts a single symbol in the tab name that follows the live activity of the agent session
running in that tab's pane.

Zellij lets only one plugin own a tab's name, and two fighting over it produce flickering renames
that clear your focus. So there is exactly one owner, and you pick which:

```
⚡ myrepo
```

With [`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer), this plugin computes the
symbol and hands it over its decoration pipe — the namer owns the name and wraps the symbol around
it. On its own, this plugin owns the name instead, and decorates whatever the tab is already called.

No new status bar, no extra column, no rename war. Your existing tab name, with a live prefix.

## Requirements

- Zellij 0.44.3 or later (`zellij --version`).
- [`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer), **optional**. It names your
  tabs after the git repo or directory, and this plugin decorates that name instead of owning it.
  Without it, the plugin decorates whatever the tab is already called.
- `jq` and bash, used by the forwarder that reports the agent's events.
- An agent to watch, the source of those events, with its producer installed through that agent's
  own plugin command: **Claude Code** 2.1.120 or later (`claude plugin install`), or **Codex**
  0.146.0 or later (`codex plugin add`). Both can run side by side.

| Plugin | `zellij-tile` | Zellij tested | Pipe protocol |
|---|---|---|---|
| 0.1.x | 0.44.3 | 0.44.3 | `agent_activity.v1` |

A plugin is wasm compiled against `zellij-tile`, and that pins the ABI the Zellij host expects, so a
newer Zellij *minor* may need a rebuild. The full matrix, the policy and the migration notes are in
[`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md).

## Pick a mode

The `mode` config key says who owns the tab name. It is **mandatory** — without it the plugin does
nothing and says so in the Zellij log.

| | `mode "pipe"` | `mode "rename"` |
|---|---|---|
| needs | `zellij-tab-namer` | nothing |
| renders | `⚡ myrepo` | `⚡ Tab #1`, or `⚡ myrepo` if you named the tab |
| owns the name | the namer | this plugin |

> The two are mutually exclusive. `mode "rename"` with `zellij-tab-namer` loaded is two plugins
> rewriting the same tab name on every update, forever — pick one.

## Install

```sh
# Download the wasm from the latest release into your plugins dir
curl -L -o ~/.config/zellij/plugins/zellij-agent-activity.wasm \
  https://github.com/vmaerten/zellij-agent-activity/releases/latest/download/zellij-agent-activity.wasm
```

> Rather build it yourself? `cargo wasm` produces the same file, see
> [Development](#development). Drop it into `~/.config/zellij/plugins/`.

Load it in `~/.config/zellij/config.kdl`, standalone:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/zellij-agent-activity.wasm" {
        mode "rename"
    }
}
```

…or alongside the namer:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/zellij-tab-namer.wasm";
    "file:~/.config/zellij/plugins/zellij-agent-activity.wasm" {
        mode "pipe"
    }
}
```

Restart Zellij and grant the plugin's permissions when prompted: `ReadApplicationState` and
`ReadCliPipes` in both modes, plus `ChangeApplicationState` under `rename` or
`MessageAndLaunchOtherPlugins` under `pipe`. Each mode asks only for what it uses, so `pipe` never
holds the ability to rename a tab.

Then install the producer, the piece that reports what your agent is doing. Each harness gets its
own, shipped from this same repo as a plugin of that harness. For **Claude Code**:

```sh
claude plugin marketplace add vmaerten/zellij-agent-activity
claude plugin install zellij-agent-activity@zellij-agent-activity
```

For **Codex**:

```sh
codex plugin marketplace add vmaerten/zellij-agent-activity
codex plugin add zellij-agent-activity@zellij-agent-activity
```

Start a new session, since both read their hooks at launch, and the tab prefix starts moving. Codex
additionally asks you to **approve the hooks** the first time: plugin hooks arrive untrusted and stay
inert until you accept them, in the interactive TUI.

Running both agents? Install both producers. They are separate plugins in separate tools, and each
tab shows whichever agent runs in it.

> `owner/repo` clones over SSH **for Claude Code**, so its first line fails if you don't have a
> GitHub key loaded. Use the URL form instead
> (`claude plugin marketplace add https://github.com/vmaerten/zellij-agent-activity.git`), or set
> `CLAUDE_CODE_PLUGIN_PREFER_HTTPS=1`. Codex clones over HTTPS and needs neither.

> Two steps, on purpose: the producer and the consumer really are two processes in two different
> tools. Each harness owns the merge into its own config, so nothing here ever writes to a file it
> doesn't own, and uninstalling is `claude plugin uninstall` / `codex plugin remove`. See
> [`docs/adr/0005-producer-per-harness-native-distribution.md`](docs/adr/0005-producer-per-harness-native-distribution.md).

> Replacing [`zellij-attention`](https://github.com/KiryuuLight/zellij-attention)? Remove it from
> `load_plugins` *and* delete its hook entries from `~/.claude/settings.json`. Hooks left piping to
> an unloaded plugin can block. This plugin's producer replaces it.

## How it works

```
agent hook                  producers/<harness>/forwarder.sh     zellij-agent-activity (wasm)
(a plugin of that agent)  ─►  $ZELLIJ_PANE_ID + event + ts  ─►   pane → tab · event → symbol
   PreToolUse/Bash          zellij pipe --name agent_activity.v1  highest-priority per tab
                                                                          │
                                                          pipe_message_to_plugin("set_prefix")
                                                                  routed by name, by tab_id
                                                                          ▼
                                              zellij-tab-namer  ─►  tab renders  "⚡ myrepo"
```

A plugin is the only thing that can see both the tab list and the pane manifest, so mapping the
reporting `pane_id` onto a stable `tab_id` is the one job a shell hook can't do. That mapping is
what this plugin adds. What happens to the name after that is the mode's business.

### The two modes

Everything above the last arrow is shared: the same events, the same per-tab winner. Only the last
step differs.

**`mode "pipe"`** sends `set_prefix` to `zellij-tab-namer`, which keeps its own base name and
composes `prefix + base + suffix`. The namer stays the sole owner of the name, and this plugin never
calls `rename_tab`.

**`mode "rename"`** writes the name itself, because Zellij has no prefix API — `rename_tab_with_id`
replaces the whole name. So the plugin strips a leading symbol off whatever the tab is currently
called, then puts the current one back:

```
"myrepo"     → strip → "myrepo" → writes "⚡ myrepo"
"⚡ myrepo"   → strip → "myrepo" → writes "● myrepo"      (no stacking)
"⚡ myrepo"   → strip → "myrepo" → writes "myrepo"        (cleared)
```

Because that is idempotent, it repairs itself: reload the plugin while a symbol is showing and the
next update cleans the leftover instead of decorating it twice. Rename a decorated tab yourself and
the symbol comes straight back on top of your new name.

Two consequences worth knowing before you pick this mode.

**Stripping is by symbol, and it applies to every tab.** Not only the ones running an agent: on load
and on every tab update, a leading `◆ ● ⚡ ✎ ◉ ⊜ ◈ ⚙ ⚠ ✓` followed by a space is removed from every
tab in the session. A tab you named `⚡ deploy` by hand becomes `deploy` even if no agent ever runs
in it. That is what makes the repair-on-reload above work — after a restart the plugin cannot know
which decorations were its own, so it treats all of them as its own.

**The symbol is the tab's real name**, so Zellij's session serialization captures it. If a session is
resurrected while the plugin is still loaded and configured, the first update cleans it up. If you
uninstalled the plugin or switched to `mode "pipe"` in between, the symbol stays: clear it with
`zellij action rename-tab`.

Running `rename` next to the namer is the rename war the design exists to avoid — see
[ADR-0008](docs/adr/0008-rename-sink-decorates-the-name-it-finds.md) for what `rename` decorates and
[ADR-0009](docs/adr/0009-the-sink-is-chosen-explicitly.md) for why the key has no default.

### The wire format

Everything between a harness and the plugin goes through a single `zellij pipe`. This is the whole
contract, and it's enough to write a producer for any other agent.

| arg | meaning |
|---|---|
| `pane_id` | `$ZELLIJ_PANE_ID` of the pane the agent runs in. Required |
| `hook_event` | normalized event name (`SessionStart`, `PreToolUse`, `Stop`, …). Required |
| `tool_name` | tool being invoked, for `PreToolUse`. A **canonical** name — see below |
| `ts_ms` | send time in ms, so events racing through parallel hooks stay ordered |
| `notification` | for `Notification` only: `permission` (needs the user) or `idle` (just a nudge) |

```sh
zellij pipe --name agent_activity.v1 --args "pane_id=3,hook_event=PreToolUse,tool_name=Bash,ts_ms=…"
```

The producer normalizes and the plugin decides. Harnesses word things differently: Claude says
"Claude needs your permission", Codex raises a `PermissionRequest`, opencode a `permission.asked`.
The same holds for `tool_name`, which is a fixed vocabulary — the one in the
[activity reference](#activity-reference) — and not the harness's raw name: Codex calls a file edit
`apply_patch`, and its producer sends `Edit`. Translating both is the producer's job, which keeps
harness vocabulary out of the plugin entirely. That's why supporting a new harness means writing a
script rather than touching the wasm — see
[ADR-0010](docs/adr/0010-the-wire-tool-vocabulary-is-canonical.md). Unknown values degrade safely: an
unrecognized `hook_event` leaves the pane alone, an unknown `tool_name` renders `⚙`, and anything
unexpected in `notification` counts as needing you rather than silently dropping a `⚠`.

Cleanup is purely event-driven. A pane's state clears on `SessionEnd` and is garbage-collected when
the pane closes. No timers, no wakeups. A crash leaves a stale prefix that heals itself on the next
prompt.

Design decisions are recorded as ADRs in [`docs/adr/`](docs/adr).

## Activity reference

This is the wire vocabulary, so it doubles as the list a producer translates into.

| wire event | Meaning | Symbol |
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

Claude Code's events map onto this one-to-one, being the harness it was written from. Codex's do
not, and its producer translates: `PermissionRequest` becomes `Notification` · `permission`,
`apply_patch` becomes `Edit`, `spawn_agent` becomes `Agent`. The full table is in
[`producers/codex/README.md`](producers/codex/README.md).

When a tab holds several agent panes, the highest-priority state wins
(`⚠ waiting > tool > ● thinking > ◆ init > ✓ done`), so a pending permission request is never hidden
behind a background pane's activity.

`⚠` means blocked, never idle. Claude fires `Notification` for two different things: a permission
prompt, and an idle nudge after about a minute without input, which lands mid-tool just as readily
as after a finished turn. Treating both as `⚠` meant every tab you left alone drifted to `⚠`, and
the symbol stopped being worth acting on. So the hook tells them apart and the plugin ignores the
nudge entirely. An agent that genuinely needs you ends its turn to ask, so the signal comes through
as a permission prompt anyway. Codex has no idle nudge at all: its `PermissionRequest` only fires on
the approval path, so it always means someone is being asked.

`SubagentStop` isn't treated as "done" either, and that's deliberate: a subagent finishing says
nothing about the agent that owns the pane, which may well be mid-tool or blocked. Only the main
agent's `Stop` ends the turn. See
[`docs/adr/0007-producer-normalizes-core-decides.md`](docs/adr/0007-producer-normalizes-core-decides.md).

## Debugging

**Nothing happens at all?** Check `mode` first — it is mandatory, and a missing or misspelled value
makes the plugin do nothing on purpose rather than guess. It always says so, no `debug` needed:

```sh
grep zellij-agent-activity "${TMPDIR:-/tmp}/zellij-$(id -u)/zellij-log/zellij.log"
```

A *wrong* symbol on a tab has two possible causes, and they need different tools: either the harness
never fired the event you expected, or it did and the plugin decided something else. Both traces are
opt-in and off by default.

**1. What the harness fired.** Point the hook at a log file, in the shell you launch the agent from:

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

> **Upgrading?** The halves upgrade separately. For the consumer, `curl` the new wasm and restart
> Zellij. For a producer, run `claude plugin update zellij-agent-activity@zellij-agent-activity` or
> `codex plugin marketplace upgrade zellij-agent-activity`, then start a new session. They only need
> to agree on the pipe protocol major (`agent_activity.v1`), so a version drift within a major is
> harmless. See [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md).

## How is this different?

| Tool | What it is | How `zellij-agent-activity` differs |
|---|---|---|
| [zellij-attention](https://github.com/KiryuuLight/zellij-attention) | Binary `⏳`/`✅` by renaming the tab itself | Richer per-tool states, and a `pipe` mode that drives the namer instead of fighting it for `TabInfo.name` |
| [zj-radar](https://github.com/marktoda/zj-radar) | A left sidebar rail (that also renames tabs) | No new UI, just a prefix on your existing tab — and it never renames behind your back, since the owner of the name is a config key |
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

The demo above is regenerated with [`vhs`](https://github.com/charmbracelet/vhs), so it can be
redone identically when the symbols change:

```sh
task wasm && vhs docs/media/demo.tape
```

It is a staged session — the panes report their own activity on the pipe rather than running a real
agent (`docs/media/demo/agent.sh`) — but the decoration is the real plugin, in `mode "rename"` so
the demo depends on nothing else.

## Credits

- Event mapping and hook patterns adapted from
  [`ishefi/zellaude`](https://github.com/ishefi/zellaude).
- Prior art and the operational lessons behind the `zellij pipe` back-pressure guard, from
  [`marktoda/zj-radar`](https://github.com/marktoda/zj-radar).
- Built to pair with [`vmaerten/zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer).

## License

MIT.
