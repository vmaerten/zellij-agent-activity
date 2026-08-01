# The rename sink decorates the name it finds

Without `zellij-tab-namer` this plugin does nothing. It computes a symbol and
pipes it to the namer, so anyone who does not already run the namer installs an
inert plugin. ADR-0004 anticipated the fix — the core emits an abstract
"this tab should show this symbol", and a second sink realises it with
`rename_tab_with_id` — and left the hard part open: **what does a sink that
writes `TabInfo.name` decorate?**

ADR-0001 is not weakened by this; its wording is. The invariant is that *only
one plugin may own a tab name*, and the consequence written there — "this plugin
never renames a tab" — assumed the namer was always the owner. The rename sink
is what you run when nothing else owns the name. Ownership stays exclusive, it
just is not always the namer's. ADR-0009 makes the choice explicit so the two
can never be on at once by accident.

## Zellij has no prefix API

`set_prefix` is the namer's invention, not a host primitive. The namer keeps a
`base_names` map of its own and recomposes `prefix + base + suffix` every time;
it never reads back what it wrote. The only host lever is
`rename_tab_with_id(tab_id, new_name)`, which writes **the whole name**.

So "only change the prefix" is not a cheaper implementation, it is the *result*
we want, and it has to be manufactured. Naively it cannot be:

```
state    tab = "myrepo"
we write rename_tab_with_id(1, "⚡ myrepo")
zellij   → TabUpdate { name: "⚡ myrepo" }     ← our own write, coming back
we write rename_tab_with_id(1, "● ⚡ myrepo")
```

Something has to tell *our decoration* apart from *the user's name*.

## The symbol alphabet is that discriminator, not our own bookkeeping

Two candidates. **Bookkeeping**: remember what we wrote, and treat any incoming
name that differs as somebody else's. **The alphabet**: strip a known symbol off
the front of whatever name we find, and put the wanted one back.

The alphabet wins, and the reason is that it is idempotent — the composition is
a pure function of the tab's current name, so there is no stored state that can
drift from reality:

```
strip(name)       leading <known symbol> + space → drop both
compose(sym, name) sym + " " + strip(name)   |   strip(name) when there is no symbol
```

Everything else falls out of that, with no special case for any of it:

| what happens | symbol | composed | our view | emitted |
|---|---|---|---|---|
| `TabUpdate` at load | — | `myrepo` | `myrepo` | **nothing** — identical |
| `PreToolUse` / Bash | `⚡` | `⚡ myrepo` | `myrepo` | rename; view becomes `⚡ myrepo` |
| `TabUpdate` (our echo) | `⚡` | `⚡ myrepo` | `⚡ myrepo` | **nothing** — the loop closes |
| user runs `rename-tab deploy` | `⚡` | `⚡ deploy` | `deploy` | rename — the symbol survives |
| `SessionEnd` | — | `deploy` | `⚡ deploy` | rename — cleaned |

Row 3 is the anti-`● ⚡ myrepo` guard, and note that it needs no knowledge that
the write was ours. Row 4 is a user rename crossing a decoration without a rule
of its own. And a plugin reload is repaired for free: a fresh state facing a tab
called `⚡ myrepo` composes `strip("⚡ myrepo")` = `myrepo`, sees a difference,
and emits one cleaning rename.

Bookkeeping gets none of that. It is also no better on the case that motivates
it — a config change reloads the plugin, which wipes the memory precisely when a
tab still carries the old symbol.

The accepted false positive: someone who deliberately names a tab `⚡ deploy`
loses their `⚡` on the first decoration. The alphabet is exotic enough that this
is a documented line, not a design problem.

## The strip set is historical, not current

Symbols are becoming configurable (#20), so "the alphabet" cannot mean "the
alphabet in force right now". A strip that only knows the current set never
cleans anything from *before* the change, which is the one case it exists for.

> The strip set is the built-in defaults **∪** the configured symbols.

The ten built-in glyphs (`◆ ● ⚡ ✎ ◉ ⊜ ◈ ⚙ ⚠ ✓`) therefore become a **legacy
format**, not a preference: they stay in the code even if the defaults change,
because tabs out there still carry them. Do not "clean up" that list.

## We never compute a name

The sink decorates the name zellij reports and nothing else. On a tab nobody has
named, `TabInfo.name` is `Tab #3`, so the sink renders `⚠ Tab #3` — and that is
the nominal case for the audience this exists for, someone with no namer.

Rebuilding the namer's job here (cwd, git root) was rejected outright. Treating
`Tab #N` as an empty name and rendering a bare `⚡` was rejected too: the base
would be empty, so a clear could not restore `Tab #3`, and the idempotence above
would break on the most common case for a cosmetic gain. `⚠ Tab #3` still says
the only thing the plugin exists to say — *this* tab wants you.

## Deduplication compares to the tab, not to a memory

The existing `shown` map compares prefixes. For the rename sink that is wrong in
both directions: a user renaming a decorated tab leaves the prefix unchanged, so
the tab keeps the new bare name until the agent next moves (false negative); and
an undecorated tab composes to its own current name, which differs from an empty
`shown`, so every tab gets renamed to itself at load (false positive).

> Emit a rename if and only if the composed name differs from the name we
> believe the tab carries.

That belief lives in `tab_name`, which the core needs anyway — `TabUpdate`
already carries `t.name` and the code was throwing it away. It is updated
optimistically on write, and **overwritten unconditionally by every
`TabUpdate`**: under this scheme there is nothing in it worth preserving, so the
real name always wins.

So the rename sink needs no `shown` at all — it compares against reality, which
is the same argument that chose the alphabet. `shown` stays, unchanged, for the
pipe sink, which has no way to observe the namer's state and must compare
against a memory. The asymmetry is deliberate: each sink deduplicates against
what it can actually see.

## Consequences

- **The sink is a strategy of the core, not a concern of the adapter.** The core
  reads the config and emits the final effect — `SetPrefix { tab_id, prefix }`
  or `RenameTab { tab_id, name }` — and `drive` goes back to being a 1:1 mapping
  onto host functions. This refines ADR-0004: the invariant there is *the
  adapter decides nothing*, not *the sink lives in the adapter*. Putting the
  strip in `drive` would have parked the only real logic of this feature on the
  one side of the linker `cargo test` cannot reach.
- The core gains `tab_name: HashMap<usize, String>`, filled from `TabUpdate`.
  Everything else — strip, composition, the emit rule — stays in the pure core
  and is tested natively.
- The 26 existing tests keep running against the pipe sink; they exercise
  decisions (priority, staleness, GC), which the split does not touch. The
  rename sink gets its own cases for what is genuinely new — composition, echo
  idempotence, no emission at load, user rename, reload repair, legacy strip
  with custom symbols, `Tab #N` — plus a parity test proving both sinks receive
  the same decisions on one identical scenario.
- Two plugins owning the name at once is now possible to configure, and it is an
  unterminated rename loop, not a flicker. ADR-0009 covers what the config does
  about it; automatic back-off is tracked in #28.
- ADR-0001's "this plugin never renames a tab" reads, from here on, as *never
  renames a tab that something else owns*.
