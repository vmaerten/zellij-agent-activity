# Modes

Zellij lets only one plugin own a tab's name, and two fighting over it produce flickering renames
that clear your focus. So there is exactly one owner, and the `mode` config key says which.

It is **mandatory**: without it the plugin does nothing and says so in the Zellij log.

| | `mode "pipe"` | `mode "rename"` |
|---|---|---|
| needs | [`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer) | nothing |
| renders | `⚡ myrepo` | `⚡ Tab #1`, or `⚡ myrepo` if you named the tab |
| owns the name | the namer | this plugin |
| permissions | `ReadApplicationState`, `ReadCliPipes`, `MessageAndLaunchOtherPlugins` | `ReadApplicationState`, `ReadCliPipes`, `ChangeApplicationState` |

Each mode asks only for what it uses, so `pipe` never holds the ability to rename a tab.

> The two are mutually exclusive. `mode "rename"` with `zellij-tab-namer` loaded is two plugins
> rewriting the same tab name on every update, forever. Pick one.

Everything before the last step is shared: the same events, the same per-tab winner. Only what
happens to the name differs.

## `mode "pipe"`

Sends `set_prefix` to `zellij-tab-namer`, which keeps its own base name and composes
`prefix + base + suffix`. The namer stays the sole owner of the name, and this plugin never calls
`rename_tab`.

## `mode "rename"`

Writes the name itself, because Zellij has no prefix API: `rename_tab_with_id` replaces the whole
name. So the plugin strips a leading symbol off whatever the tab is currently called, then puts the
current one back:

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
in it. That is what makes the repair-on-reload above work: after a restart the plugin cannot know
which decorations were its own, so it treats all of them as its own.

**The symbol is the tab's real name**, so Zellij's session serialization captures it. If a session is
resurrected while the plugin is still loaded and configured, the first update cleans it up. If you
uninstalled the plugin or switched to `mode "pipe"` in between, the symbol stays: clear it with
`zellij action rename-tab`.

See [ADR-0008](adr/0008-rename-sink-decorates-the-name-it-finds.md) for what `rename` decorates and
[ADR-0009](adr/0009-the-sink-is-chosen-explicitly.md) for why the key has no default.
