# Prefix slot, per-pane activity aggregated by priority

The activity symbol is shown in the namer's **prefix** slot (`⚡ myrepo`), not
the suffix. The namer has one prefix and one suffix string per tab; taking the
prefix leaves the suffix free for the user's own decorations, so the activity
indicator and a manual `" [building]"` suffix never collide. A dedicated third
decoration channel in the namer was rejected as v1 over-engineering (it would
also break the "namer unmodified" invariant of ADR-0001).

Activity is tracked **per pane** (`pane_id → Activity`), then aggregated to the
tab by taking the **highest-priority** activity among the tab's panes
(`Waiting > Tool > Thinking > Init > Done`). A tab can hold several panes, hence
several agents; there is only one prefix. Last-write-wins would let a background
`Thinking ●` overwrite a foreground `Waiting ⚠` in the same tab — masking the
one state the tool exists to surface. Priority aggregation guarantees a pending
permission request is never hidden.

## Consequences

- The core keeps `Activity` (ported from zellaude) and an `activity_priority`
  ordering; the winning activity per tab is recomputed on every event and on
  pane moves.
- `Done ✓` persists (it is the lowest priority but still shown when nothing else
  is active) — you can see at a glance which tabs have finished and await you.
- The prefix string is `"{symbol} "`; the namer composes
  `{prefix}{base}{suffix}{count}` unchanged.
