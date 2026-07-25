# The producer normalizes, the core decides

`Notification` turned out to be **two signals wearing one name**. Claude Code
fires it both for a permission prompt — the user must act — and for an *idle
nudge* about a minute after the turn ended, when there is nothing to do. The core
mapped both to `Waiting`, so every tab drifted to `⚠` once left alone, and the one
thing the plugin exists to tell you ("*this* session needs you") stopped meaning
anything. Measured on a live pane:

| time | event | `message` | symbol |
|---|---|---|---|
| 07:47:38 | `Notification` | Claude needs your permission | ⚠ |
| 07:48:27 | `Stop` | — | ✓ |
| 07:49:27 | `Notification` | **Claude is waiting for your input** | ⚠ |

Only the payload's `message` separates them, and the forwarder was dropping it —
it sent `hook_event` and `tool_name` and nothing else. So the question was where
to put the knowledge that these two strings mean different things.

**In the producer.** Not because the wire couldn't carry the raw message — args
are comma-separated so a message can't ride along as an arg, but `zellij pipe`
has a payload that could. The reason is evolution: ADR-0005 gives each harness its
own producer, and Codex, opencode and crush will each phrase this differently. Had
the core classified, adding a harness would mean editing Rust, rebuilding the wasm
and shipping a release every user must install to recognize a harness they may not
even run. With normalization in the producer, **adding a harness is writing a
script — the core does not change.** So:

> The producer normalizes its harness's vocabulary. The core decides policy.

"Does this wording mean permission or idle" is vocabulary. "Should an idle nudge
change the tab" is policy. The normalized vocabulary already fits the harnesses on
the roadmap: opencode exposes `permission.asked` and `session.idle`, which map
straight onto the two values.

The wire gains an optional `notification=permission|idle`, sent only for
`Notification` events — additive and optional, per ADR-0006, so producer and
consumer still drift in any order without breaking.

**Unknown wording falls back to `permission`, deliberately.** If Claude rewords
its messages, a stale rule yields one `⚠` too many; the opposite default would
silently swallow a real "come unblock me". The costs are not symmetric, so the
fallback is not either. The core applies the same asymmetry to the wire: a missing
or unrecognized kind counts as needing the user.

## The policy: an idle nudge is ignored only on a finished turn

A nudge arriving *mid-turn* is not noise — it means the agent is blocked on the
user, typically on a question it asked. That `⚠` must survive. Only a nudge after
the turn ended is redundant, because `Stop` has already put `✓` there, which is the
right state.

The pane's own activity carries this: `Done` is produced by `Stop` alone and any
later event overwrites it, so `pane_activity[pane] == Done` **is** the
"turn has ended" flag. A first draft added a separate `turn_done` set; it was
removed as a second source of truth for the same fact, with its own mutations and
GC to keep in sync.

## What was measured, and what it invalidates

Traced from a real session (`ZELLIJ_AGENT_ACTIVITY_LOG`), headless and interactive:

- **Subagents report nothing.** A subagent that made two tool calls produced zero
  hook events — no `PreToolUse`/`PostToolUse` of its own, no `SubagentStop`, no
  `transcript_path` under `/subagents/`. Several agents never write into one pane,
  which retires the "key by `(pane_id, agent_id)`" design once considered here, and
  corrects two claims in ADR-0003.
- **`SubagentStop` is never emitted at all.** Dropping it (ADR-0003, amended)
  remains right as specification, but it did not fix the observed incident it was
  blamed for — that diagnosis was an inference from transcript mtimes, not an
  observation, and the incident (`✓` shown during a permission prompt) is still
  unexplained. The trace is the tool for catching it if it recurs.
- **The subagent tool is named `Agent`, not `Task`** — so `tool_symbol("Task")` was
  dead code and launching a subagent rendered `⚙`. Both names now map to `⊜`.
- **`PostToolUse` for `Agent` lands ~2s in while the subagent runs for 7s**: the
  launch is asynchronous, and the main agent can finish its turn while subagents
  are still working.

## Consequences

- ADR-0003's "minimal forwarder" becomes **a forwarder that normalizes but never
  decides**. Normalization is the producer's whole reason to exist per ADR-0005;
  policy stays in the core.
- That puts one rule in shell, which `cargo test` cannot reach — so the forwarder
  gains a `ZELLIJ_AGENT_ACTIVITY_DRY_RUN` short-circuit printing the args instead
  of piping them, and `scripts/test-hook.sh` asserts the mapping (both real
  wordings, unknown wording, absent message, non-`Notification` events). It runs in
  `task ci` and as its own CI job. Every future producer gets the same treatment:
  a producer that classifies is a producer that must be tested.
- `notification` joins the documented wire format a third-party producer writes
  against, alongside `pane_id`, `hook_event`, `tool_name` and `ts_ms`.
- The `⚠` is now worth acting on, which is the whole point: `Stop` says finished,
  `⚠` says blocked, and the two no longer collapse into one symbol.
