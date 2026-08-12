# Event-driven cleanup, minimal forwarder hook

Cleanup is purely event-driven: no timers, no periodic wakeups (a property
inherited from the namer). A pane's activity is dropped on `SessionEnd` (Claude
fires it reliably on exit) and GC'd when the pane disappears from a
`PaneManifest`. A crash or hard `Ctrl-C` can leave a stale prefix, but it
self-heals: the next `UserPromptSubmit` / `SessionStart` in that pane overwrites
it. A decay timer was rejected: the failure it covers is rare and visible, and
timers would forfeit the zero-wakeup property. It can be added later if it
proves annoying in practice.

The hook script is a **minimal forwarder**: it reads the Claude hook JSON on
stdin and pipes `pane_id`, `hook_event`, `tool_name`, and a send-time `ts_ms`
to the plugin. Nothing else. The `⚠` prefix on the tab *is* the notification.
(ADR-0007 refines "nothing else": the forwarder also *normalizes* its harness's
vocabulary, but it still decides nothing.)

Its one addition is an **opt-in trace**, off unless `ZELLIJ_AGENT_ACTIVITY_LOG`
names a file: one JSON line per event, appended before forwarding. It pairs with
the plugin's own `debug true` tracing (`Effect::Log` → stderr → the zellij log):
producer side answers *what the harness fired, and in what order*, consumer side
answers *what the plugin decided*, and a wrong symbol is always one or the other.
The `SubagentStop` bug above had to be reconstructed after the fact from Claude's
transcript files, which is what motivated both, and the trace then showed that
reconstruction to be wrong, which is the best argument for having it. Fields are
selected rather than dumped: `tool_input` is large and can hold secrets, and short
lines keep the `O_APPEND` write atomic across parallel hook subprocesses.

**`SubagentStop` is not a "done" signal** and is neither mapped nor registered: a
subagent finishing says nothing about the agent that owns the pane. It was dropped
after a tab showed `✓` while the agent sat on a permission prompt, a diagnosis
later shown to be wrong, since tracing proved `SubagentStop` is never emitted at
all and subagents report no hooks whatsoever. The mapping was still wrong, so the
change stands, but that incident is **not** what it fixed and remains unexplained.
Only the main agent's `Stop` ends the turn. See ADR-0007 for the measurements,
including the corollary that several agents never share a pane's event stream, so
the "key by `(pane_id, agent_id)`" follow-up once planned here is moot.

The `⚠` "Claude needs you" state comes from the **`Notification`** hook, not a
`PermissionRequest` event (Claude Code has no such hook; verified against the
installed version and cross-checked with the zj-radar project). Claude fires it
whenever it wants the user's attention, which turned out to cover two different
situations, a permission prompt *and* an idle nudge after a minute without input.
Only the
first is worth a `⚠`; **ADR-0007 supersedes the original "every `Notification` maps
to `Waiting`"** and splits them at the producer.

## Consequences

- `ts_ms` is stamped by the hook at send time; the core drops any event whose
  `ts_ms` is older than the last one seen for that pane, since parallel hook
  subprocesses race. The core stays pure: the hook supplies the clock.
- `zellij pipe` is **not** fire-and-forget: it blocks the caller until every
  plugin instance consumes the message, so a hook whose plugin is absent or stuck
  can leak file descriptors until the zellij server hits `EMFILE` (the failure
  that killed `zellij-smart-tabs`). The hook therefore wraps the pipe in a
  self-limiting ~5s timeout that survives the hook runner being killed.
- `unblock_cli_pipe_input` needs the **`ReadCliPipes`** permission, which went
  unrequested from the start: 3157 denials in one zellij log before anyone
  looked. Measured A/B on a live session, though, the denial costs nothing
  observable: a full hook round-trip is ~30ms with or without the grant, because
  zellij releases the pipe by itself once a plugin has handled the message. The
  unblock call is a defensive belt (inherited from zj-radar) over a host that
  already unblocks; requesting the permission makes it a real belt instead of a
  denied no-op flooding the log. It is *not* what stands between us and the
  `EMFILE` failure: the hook's own watchdog is.
  `unblocking_a_cli_pipe_is_covered_by_a_requested_permission` ties the effect to
  the grant so the two cannot drift apart again.
- Still unexplained: `Action CliPipe did not complete within 1s timeout` keeps
  appearing server-side even with the grant in place and the client returning in
  ~30ms. It has no measured effect on hook latency, so it is recorded here rather
  than chased.
- Desktop notifications / bell are out of scope for v1; they are orthogonal to
  the plugin and can be added to the hook later, opt-in, without touching the
  Rust.
- Click-to-focus (zellaude's "click the tab to focus the waiting pane") is **not
  possible** here: it depends on rendering a status bar with click regions, and
  this plugin renders nothing. That is the accepted cost of not being a bar.
- Keeping the hook dumb makes it trivial to replicate per harness later
  (codex / opencode / gemini just forward the same fields).
