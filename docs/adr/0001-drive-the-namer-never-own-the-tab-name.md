# Drive the namer, never own the tab name

Only one plugin may write `TabInfo.name`; two that call `rename_tab` fight over
it (renames bounce, decorations flicker on focus). `zellij-tab-namer` already
owns the name and exposes a generic decoration Pipe API (`set_prefix` /
`set_suffix` / `clear_*`, addressing tabs by stable `tab_id`). So this plugin
**never renames a tab**. It computes an activity symbol and asks the namer to
show it, over a pipe. The namer stays the sole owner of the name.

(ADR-0008 refines "never renames a tab": the invariant is **one owner**, and the
wording above assumed that owner is always the namer. The standalone `rename`
sink takes the name when nothing else holds it.)

The plugin's only novel job is what a hook cannot do: map the reporting
`pane_id` to a `tab_id` (a plugin sees `TabUpdate` + `PaneManifest`; a hook does
not). Everything else, the symbol and the wrapping, is delegated to the namer.

## Consequences

- **Invariant: the namer is not modified.** Delivery is solved on this side. The
  plugin sends `pipe_message_to_plugin(MessageToPlugin::new("set_prefix")
  .with_args(...))`; zellij is expected to route it to the already-loaded namer
  by pipe name (the namer accepts `PipeSource::Plugin` messages, see its tests)
  without launching anything. This routing is server-side and unverified
  statically, so it is validated live before the plugin is fleshed out.
- If name-based delivery does not reach the namer, the fallback is
  `.with_plugin_url(namer_url)` from a config key (default to the common namer
  URL), accepting that a wrong URL risks launching a duplicate namer. Adding a
  receiver verb to the namer is **excluded**: it would break this invariant.
- This is why the draft that did `rename_tab` directly (and `zellij-attention`)
  are abandoned rather than continued: they reintroduce the collision.
- Requires the `MessageAndLaunchOtherPlugins` permission (to pipe to the namer)
  and `ReadApplicationState` (to receive `TabUpdate` / `PaneUpdate`).
