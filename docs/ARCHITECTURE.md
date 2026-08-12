# Architecture

## How it works

```
agent event                 producers/<harness>/                 zellij-agent-activity (wasm)
(a plugin of that agent)  ─►  $ZELLIJ_PANE_ID + event + ts  ─►   pane → tab · event → symbol
   PreToolUse/Bash          zellij pipe --name agent_activity.v1  highest-priority per tab
                                                                          │
                                                          pipe_message_to_plugin("set_prefix")
                                                                  routed by name, by tab_id
                                                                          ▼
                                              zellij-tab-namer  ─►  tab renders  "⚡ myrepo"
```

A plugin is the only thing that can see both the tab list and the pane manifest, so mapping the
reporting `pane_id` onto a stable `tab_id` is the one job a producer can't do. That mapping is
what this plugin adds. What happens to the name after that is [the mode's business](MODES.md).

## The wire format

Everything between a harness and the plugin goes through a single `zellij pipe`. This is the whole
contract, and it's enough to write a producer for any other agent.

| arg | meaning |
|---|---|
| `pane_id` | `$ZELLIJ_PANE_ID` of the pane the agent runs in. Required |
| `hook_event` | normalized event name (`SessionStart`, `PreToolUse`, `Stop`, …). Required |
| `tool_name` | tool being invoked, for `PreToolUse`. A **canonical** name, see below |
| `ts_ms` | send time in ms, so events racing through parallel hooks stay ordered |
| `notification` | for `Notification` only: `permission` (needs the user) or `idle` (just a nudge) |

```sh
zellij pipe --name agent_activity.v1 --args "pane_id=3,hook_event=PreToolUse,tool_name=Bash,ts_ms=…"
```

The producer normalizes and the plugin decides. Harnesses word things differently: Claude says
"Claude needs your permission", Codex raises a `PermissionRequest`, opencode a `permission.asked`.
The same holds for `tool_name`, which is a fixed vocabulary (the one in the README's activity table)
and not the harness's raw name: Codex calls a file edit `apply_patch`, and its producer sends
`Edit`. Translating both is the producer's job, which keeps harness vocabulary out of the plugin
entirely. That's why supporting a new harness means writing a script rather than touching the wasm,
see [ADR-0010](adr/0010-the-wire-tool-vocabulary-is-canonical.md). That script is a shell forwarder
where the harness runs commands, and a plugin where it doesn't: opencode's producer is a `.js` file
loaded into its server, and calls the same `zellij pipe`. Unknown values degrade safely:
an unrecognized `hook_event` leaves the pane alone, an unknown `tool_name` renders `⚙`, and anything
unexpected in `notification` counts as needing you rather than silently dropping a `⚠`.

Cleanup is purely event-driven. A pane's state clears on `SessionEnd` and is garbage-collected when
the pane closes. No timers, no wakeups. A crash leaves a stale prefix that heals itself on the next
prompt.

## Activity, in detail

Claude Code's events map one-to-one onto the wire vocabulary, being the harness it was written from.
The others do not, and their producers translate. Codex sends `Notification` · `permission` for a
`PermissionRequest`, `Edit` for `apply_patch`, `Agent` for `spawn_agent`; opencode sends `Stop` for
`session.idle`, `Agent` for `task`, `Edit` for both `edit` and `apply_patch`. The full tables are in
[`producers/codex/README.md`](../producers/codex/README.md) and
[`producers/opencode/README.md`](../producers/opencode/README.md).

When a tab holds several agent panes, the highest-priority state wins
(`⚠ waiting > tool > ● thinking > ◆ init > ✓ done`), so a pending permission request is never hidden
behind a background pane's activity.

`⚠` means blocked, never idle. Claude fires `Notification` for two different things: a permission
prompt, and an idle nudge after about a minute without input, which lands mid-tool just as readily
as after a finished turn. Treating both as `⚠` meant every tab you left alone drifted to `⚠`, and
the symbol stopped being worth acting on. So the hook tells them apart and the plugin ignores the
nudge entirely. An agent that genuinely needs you ends its turn to ask, so the signal comes through
as a permission prompt anyway. Neither Codex nor opencode has an idle nudge at all: their
`PermissionRequest` and `permission.asked` only fire on the approval path, so they always mean
someone is being asked.

`SubagentStop` isn't treated as "done" either, and that's deliberate: a subagent finishing says
nothing about the agent that owns the pane, which may well be mid-tool or blocked. Only the main
agent's `Stop` ends the turn. Harnesses that give a subagent no separate event (opencode runs one in
a child session that reports exactly like the main one) put that filter in the producer instead.
See [ADR-0007](adr/0007-producer-normalizes-core-decides.md).

## Prior art and alternatives

| Tool | What it is | How `zellij-agent-activity` differs |
|---|---|---|
| [zellij-attention](https://github.com/KiryuuLight/zellij-attention) | Binary `⏳`/`✅` by renaming the tab itself | Richer per-tool states, and a `pipe` mode that drives the namer instead of fighting it for `TabInfo.name` |
| [zj-radar](https://github.com/marktoda/zj-radar) | A left sidebar rail (that also renames tabs) | No new UI, just a prefix on your existing tab, and it never renames behind your back, since the owner of the name is a config key |
| [zjstatus](https://github.com/dj95/zjstatus) | The status bar | Leaves your bar alone and decorates the tab name only |

In short: one plugin owns the tab name, and everything else decorates it.

## Decision records

| ADR | |
|---|---|
| [0001](adr/0001-drive-the-namer-never-own-the-tab-name.md) | Drive the namer, never own the tab name |
| [0002](adr/0002-prefix-slot-and-per-pane-priority.md) | Prefix slot, per-pane activity aggregated by priority |
| [0003](adr/0003-event-driven-cleanup-and-minimal-hook.md) | Event-driven cleanup, minimal forwarder hook |
| [0004](adr/0004-effects-seam-and-sink-abstraction.md) | Effects seam: testable core and a sink abstraction for standalone |
| [0005](adr/0005-producer-per-harness-native-distribution.md) | Producer per harness, distributed natively: the plugin never self-installs |
| [0006](adr/0006-versioned-pipe-contract-compat-by-tolerance.md) | The versioned pipe contract: compatibility by tolerance, not lockstep |
| [0007](adr/0007-producer-normalizes-core-decides.md) | The producer normalizes, the core decides |
| [0008](adr/0008-rename-sink-decorates-the-name-it-finds.md) | The rename sink decorates the name it finds |
| [0009](adr/0009-the-sink-is-chosen-explicitly.md) | The sink is chosen explicitly, and a bad config says so |
| [0010](adr/0010-the-wire-tool-vocabulary-is-canonical.md) | The wire's tool vocabulary is canonical: producers translate into it |
| [0011](adr/0011-the-opencode-producer-is-a-stateful-plugin.md) | The opencode producer is a stateful plugin, not a stateless hook |
