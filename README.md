# zellij-agent-activity

A tiny [Zellij](https://zellij.dev) plugin that shows **what your AI coding agent is doing right
now** as a symbol in front of the tab it runs in — `⚡` running a command, `✎` editing, `⚠` waiting
for you, `✓` done — so you can glance across a many-tab session and see, without switching, which
agent needs you. Harness-neutral by design; **Claude Code** is wired up today, others (Codex,
Gemini CLI, opencode) are a hook script away.

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
  <a href="#how-is-this-different">How is this different?</a>
</p>

<!-- Drop a screen recording here: docs/media/demo.gif -->

```
◆ starting   ● thinking   ⚡ bash   ✎ edit   ◉ read   ⊜ subagent   ◈ web   ⚠ needs you   ✓ done
```

## What is it?

AI coding agents spend long stretches working, then quietly block on a permission prompt — or
finish — while you're looking at another tab. In a busy Zellij session it's easy to lose track of
which one is waiting on you.

`zellij-agent-activity` surfaces that at a glance: a single symbol in the tab name that tracks the
live activity of the agent session in that tab's pane (Claude Code today).

The twist — and the whole point — is **it never renames your tabs itself.** Zellij lets only one
plugin own a tab's name, and two that fight over it produce flickering, focus-clearing renames.
So this plugin computes the activity symbol and hands it to
[`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer) through its decoration pipe.
The namer stays the single owner of the name and simply wraps the symbol around it:

```
⚡ myrepo
```

No new status bar, no new column, no rename war. Your existing tab name, with a live prefix.

## Highlights

- **See which tab needs you** — `⚠` the moment Claude asks for permission, `✓` when it's done.
- **Per-tool granularity** — `⚡` bash, `✎` edit, `◉` read, `⊜` subagent, `◈` web — not just a
  binary busy/idle.
- **Never fights for the tab name** — decorates through `zellij-tab-namer`; exactly one plugin
  owns `TabInfo.name`.
- **Not a status bar** — leaves zjstatus / your bar completely alone.
- **Push-driven, zero polling, zero timers** — updates arrive from Claude Code hooks; the plugin
  is idle between events.
- **Multiple agents per tab handled right** — a `⚠` from one pane is never masked by a `●` from
  another in the same tab (highest-priority state wins).

## Requirements

- **Zellij ≥ 0.44.3** (`zellij --version`) — built against `zellij-tile 0.44.x`; a newer Zellij
  minor may require a rebuild.
- **[`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer)** loaded — this plugin
  drives it and does nothing on its own (a standalone mode is on the roadmap).
- **`jq`** and **bash** — used by the hook that forwards Claude's events.
- **Claude Code** — the source of the activity events.

## Install

```sh
# Download the wasm from the latest release into your plugins dir
curl -L -o ~/.config/zellij/plugins/zellij-agent-activity.wasm \
  https://github.com/vmaerten/zellij-agent-activity/releases/latest/download/zellij-agent-activity.wasm
```

> Prefer building from source? See [Development](#development) — `cargo wasm` produces the same
> `zellij-agent-activity.wasm`; drop it into `~/.config/zellij/plugins/`.

Load it alongside the namer in `~/.config/zellij/config.kdl`:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/zellij-tab-namer.wasm";
    "file:~/.config/zellij/plugins/zellij-agent-activity.wasm";
}
```

Restart Zellij and **grant the plugin's permissions** when prompted
(`ReadApplicationState`, `MessageAndLaunchOtherPlugins`, `RunCommands`). On that grant the plugin
**auto-installs its Claude Code hook** into `~/.claude/settings.json` (idempotent, backed up to
`.bak`, and it never touches your other hooks). Start a **new** Claude session — Claude reads its
hooks at launch — and the tab prefix comes alive.

> Replacing [`zellij-attention`](https://github.com/KiryuuLight/zellij-attention)? Remove it from
> `load_plugins` *and* delete its hook entries from `~/.claude/settings.json` — leaving hooks that
> pipe to an unloaded plugin can block. This plugin's hook is the replacement.

## How it works

```
Claude Code hook            zellij-agent-activity-hook.sh        zellij-agent-activity (wasm)
(~/.claude/settings.json) ─►  $ZELLIJ_PANE_ID + event + ts  ─►   pane → tab · event → symbol
   PreToolUse/Bash            zellij pipe --name agent_activity   highest-priority per tab
                                                                          │
                                                          pipe_message_to_plugin("set_prefix")
                                                                  routed by name, by tab_id
                                                                          ▼
                                              zellij-tab-namer  ─►  tab renders  "⚡ myrepo"
```

A plugin is the only thing that can see both the tab list and the pane manifest, so mapping the
reporting `pane_id` to a stable `tab_id` is the one job a shell hook can't do — that's what this
plugin adds. Everything else (the symbol, the wrapping, the name) is delegated to the namer.

Cleanup is purely event-driven: a pane's state clears on `SessionEnd` and is garbage-collected
when the pane closes. No timers, no wakeups. A crash leaves a stale prefix that self-heals on the
next prompt.

Design decisions are recorded as ADRs in [`docs/adr/`](docs/adr).

## Activity reference

| Claude hook event | Meaning | Symbol |
|---|---|---|
| `SessionStart` | starting up | `◆` |
| `UserPromptSubmit`, `PostToolUse` | thinking | `●` |
| `PreToolUse` · `Bash` | running a command | `⚡` |
| `PreToolUse` · `Edit` / `Write` / `MultiEdit` | editing | `✎` |
| `PreToolUse` · `Read` / `Glob` / `Grep` | reading | `◉` |
| `PreToolUse` · `Task` | subagent | `⊜` |
| `PreToolUse` · `WebSearch` / `WebFetch` | web | `◈` |
| `PreToolUse` · other / MCP | tool | `⚙` |
| `Notification` | **needs you** (permission / idle) | `⚠` |
| `Stop`, `SubagentStop` | done | `✓` |
| `SessionEnd` | — (clears the prefix) | |

When a tab holds several Claude panes, the highest-priority state wins
(`⚠ waiting > tool > ● thinking > ◆ init > ✓ done`) — a pending permission request is never
hidden behind a background pane's activity.

## How is this different?

| Tool | What it is | How `zellij-agent-activity` differs |
|---|---|---|
| [zellij-attention](https://github.com/KiryuuLight/zellij-attention) | Binary `⏳`/`✅` by renaming the tab itself | Richer per-tool states, and it drives the namer instead of fighting it for `TabInfo.name` |
| [zj-radar](https://github.com/marktoda/zj-radar) | A left sidebar rail (that also renames tabs) | No new UI and no name ownership — just a prefix on your existing tab, via a dedicated namer |
| [zjstatus](https://github.com/dj95/zjstatus) | The status bar | Leaves your bar alone; decorates the tab name only |

The short version: **one plugin owns the tab name; everything else decorates.**

## Development

```sh
cargo test    # pure core, host-native, no zellij needed
cargo wasm    # release build -> target/wasm32-wasip1/release/zellij-agent-activity.wasm
```

The plugin is a pure state machine (events in → effects out) with a thin, wasm-gated adapter that
executes those effects against the Zellij host — so the pane→tab routing and priority aggregation
are exercised as ordinary unit tests, never in a live session. See
[`docs/adr/0004-effects-seam-and-sink-abstraction.md`](docs/adr/0004-effects-seam-and-sink-abstraction.md).

## Credits

- Event mapping, hook, and auto-installer patterns adapted from
  [`ishefi/zellaude`](https://github.com/ishefi/zellaude).
- Prior art and hard-won operational lessons (the `zellij pipe` back-pressure guard) from
  [`marktoda/zj-radar`](https://github.com/marktoda/zj-radar).
- Built to pair with [`vmaerten/zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer).

## License

MIT.
