# Event-driven cleanup, minimal forwarder hook

Cleanup is purely event-driven — no timers, no periodic wakeups (a property
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

The `⚠` "Claude needs you" state comes from the **`Notification`** hook, not a
`PermissionRequest` event (Claude Code has no such hook; verified against the
installed version and cross-checked with the zj-radar project). Claude fires
`Notification` precisely when it wants the user's attention — a permission prompt
or an idle nudge — so every `Notification` maps to `Waiting`. This supersedes the
earlier design note that treated `Notification` as informational.

## Consequences

- `ts_ms` is stamped by the hook at send time; the core drops any event whose
  `ts_ms` is older than the last one seen for that pane, since parallel hook
  subprocesses race. The core stays pure — the hook supplies the clock.
- `zellij pipe` is **not** fire-and-forget: it blocks the caller until every
  plugin instance consumes the message, so a hook whose plugin is absent or stuck
  can leak file descriptors until the zellij server hits `EMFILE` (the failure
  that killed `zellij-smart-tabs`). The hook therefore wraps the pipe in a
  self-limiting ~5s timeout that survives the hook runner being killed.
- Desktop notifications / bell are out of scope for v1; they are orthogonal to
  the plugin and can be added to the hook later, opt-in, without touching the
  Rust.
- Click-to-focus (zellaude's "click the tab to focus the waiting pane") is **not
  possible** here: it depends on rendering a status bar with click regions, and
  this plugin renders nothing. That is the accepted cost of not being a bar.
- Keeping the hook dumb makes it trivial to replicate per harness later
  (codex / opencode / gemini just forward the same fields).
