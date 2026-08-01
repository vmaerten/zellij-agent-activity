# The sink is chosen explicitly, and a bad config says so

There are now two sinks (ADR-0008), and they are mutually exclusive: `pipe`
drives `zellij-tab-namer`, `rename` owns the tab name itself. Something has to
pick, and the cost of picking wrong is not symmetric:

| picking… | what happens |
|---|---|
| `pipe` with no namer loaded | the `set_prefix` goes nowhere. The plugin does **nothing**, silently. Annoying, fixable, harmless. |
| `rename` with the namer loaded | both plugins rewrite `TabInfo.name` on every `TabUpdate`, forever. Not a flicker — an unterminated loop between two plugins, and the namer loses its names. |

That is the collision ADR-0001 exists to prevent, so the second row is the one
the design has to make hard to reach.

## Nothing is auto-detected

The obvious move is to detect the namer and default accordingly. It was
rejected on three counts. `PaneInfo` does carry `plugin_url`, but the namer is
loaded through `load_plugins`, headless, and whether a paneless plugin appears
in the `PaneManifest` at all could not be established without a live session.
Asking the namer directly is out: ADR-0001 holds the namer unmodified, and a
health verb is still a modification. And detection tests a **proxy** — "the
namer is loaded" — rather than the problem: `zj-radar` renames tabs too, and
would sail straight past it.

Detecting the *symptom* instead (we wrote X, something else keeps overwriting
it) does test the real thing and works against any competitor. It is the right
mechanism, and it is deferred to #28 rather than bundled here.

## The key is mandatory

> `sink "pipe"` or `sink "rename"`. No default.

A default of `pipe` keeps every existing config working, but leaves the newcomer
with the inert plugin this whole feature exists to fix — the default would not
solve anything, only the README would. A default of `rename` serves the newcomer
and hands every existing user a rename war on upgrade, unasked. Making the key
mandatory breaks existing configs exactly once, and at the time of writing there
is one existing user.

That also closes the accident: nobody reaches `rename` without typing it.
Deliberately running `rename` alongside the namer remains possible, and against
that the guard is the README, in plain words, plus #28 later.

**An unknown value behaves as an absent one.** `sink "renmae"` must not quietly
fall back to `pipe`, which would turn a typo into a plugin that does nothing for
no visible reason — the most tedious bug of the set.

## A misconfiguration is always logged

"Mandatory" cannot mean "refuses to start with an error": this plugin is
headless, loaded by `load_plugins`, its `render` is empty, and there is no
surface to show anything on. Left alone, mandatory degrades into *silently does
nothing* — the same failure we just refused to ship as a default, moved from
"no namer" to "no key".

So the plugin emits no effects and writes one line that is **not** gated behind
`debug`, landing in the zellij log where someone looking for why nothing moves
will find it. This is a new rule, and it is worth stating because the config
surface will grow:

> A configuration error is always logged. A decision trace is logged under
> `debug`.

`show_self(true)` — a background plugin materialising as a floating pane to show
its error — was considered and parked. It is unmissable and intrusive in equal
measure, and its behaviour on a paneless plugin needs live validation. If the
plugin ever gains a real surface, or if #26 brings notifications, that is where
this comes back.

## Consequences

- `load` parses `sink` before anything else; absent or unrecognised means no
  permissions requested, no subscriptions, no effects — and one log line.
- Two native tests pin it: `sink` absent, and `sink` misspelled, both yielding
  zero effects plus a log. They are the first tests to assert on an ungated
  `Effect::Log`.
- The README stops presenting `zellij-tab-namer` as a requirement. The choice
  between the two sinks is posed **before** the install steps, because with a
  mandatory key you cannot install without having made it, and it carries the
  exclusivity warning in plain words.
- Both install snippets in the README grow a config block. The one that omits it
  is no longer valid.
