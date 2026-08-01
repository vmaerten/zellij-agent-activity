# Changelog

All notable changes to this project are documented in this file. Entries are
generated from the conventional commits by [git-cliff](https://git-cliff.org),
except where writing one by hand said more.

## [0.1.0] - 2026-08-01

First release.

Shows what your AI coding agent is doing as a symbol in front of the tab it runs
in, by driving [`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer)
through its decoration pipe. It never renames a tab itself.

- Per-tool symbols: `◆` starting, `●` thinking, `⚡` bash, `✎` edit, `◉` read,
  `⊜` subagent, `◈` web, `⚙` any other tool, `⚠` needs you, `✓` done.
- Activity is tracked per pane and aggregated per tab by priority, so a pending
  permission request is never hidden behind a background pane's activity.
- Claude Code producer, distributed as a Claude Code plugin from this repo.
- `agent_activity.v1` pipe protocol, documented so any harness can feed it.
- Event-driven cleanup: no timers, no polling.
- Opt-in tracing on both sides: `ZELLIJ_AGENT_ACTIVITY_LOG` for the hook,
  `debug true` for the plugin.
