# The wire's tool vocabulary is canonical — producers translate into it

ADR-0007 put harness vocabulary in the producer and kept policy in the core, so
that **adding a harness is writing a script**. It settled that rule for one field,
`notification`. Codex made the same question arrive for `tool_name`, and the
answer was not automatic.

The core renders a symbol by looking `tool_name` up in `TOOL_SYMBOLS`
(`src/main.rs`), a table whose entries — `Bash`, `Edit`, `Read`, `Agent`,
`WebSearch` — are Claude Code's tool names, there by accident of being written
first. Codex names the same acts differently: file edits are `apply_patch`,
subagents are `spawn_agent`. Two ways to make them render:

1. **Add the Codex names to `TOOL_SYMBOLS`.** The producer forwards raw names.
2. **Translate in the producer.** The table does not move.

The first is what a table invites, and it is wrong for the same reason ADR-0007
gave: it means editing Rust, rebuilding the wasm and shipping a release that every
user must install to recognize a harness they may not even run — and once for each
harness on the roadmap. The table would also become a union of every vendor's
naming, where `Edit` and `apply_patch` sit side by side meaning the same thing,
with nothing saying which a producer should send.

So:

> `tool_name` on the wire is a **canonical vocabulary**, not the harness's raw
> name. Translating into it is the producer's job.

The vocabulary is the one already documented in the README's activity reference.
It is Claude-shaped because Claude came first; that is history, not a claim that
Claude is the reference implementation. What matters is that it is *fixed* and
*documented*, so a third-party producer can be written against it — with `⚙` as
the fallback for anything outside it, which is what makes the set safe to keep
small.

The Codex producer therefore maps `apply_patch` → `Edit`, `spawn_agent` →
`Agent`, `web_search` → `WebSearch`, `view_image` → `Read`, and lets everything
else through unchanged. Codex already serializes `Bash` for its shell tools
(`codex-rs/core/src/tools/hook_names.rs`), so that one needs nothing.

## Consequences

- **`TOOL_SYMBOLS` is closed to harness growth.** It grows only when a *new kind
  of act* deserves its own symbol, never because another vendor spells an
  existing one differently. The `Task`/`Agent` pair stays as it is: those are two
  names from the same harness across versions, not two harnesses.
- Translation is a second rule living in shell, so it gets the same treatment as
  the first (ADR-0007): `producers/codex/scripts/test-forwarder.sh` asserts every
  arm, and it runs in `task ci` and as a CI job. **A producer that translates is
  a producer that must be tested.**
- The wire format section of the README now says `tool_name` is canonical, and
  the activity reference doubles as the list a producer writes against.
- One case is not a rename but a reclassification: Codex's `request_user_input`
  is the agent stopping to ask you something outside the approval path. It is
  emitted as `Notification` · `permission`, not as a tool, because `⚠` means
  blocked and that is exactly what it is. The rule generalises — a producer may
  map a harness's tool onto a *different* wire event when the tool is the signal.
- This does not reopen ADR-0006: nothing on the wire changed. A producer sending
  an unknown `tool_name` still renders `⚙`, so an untranslated harness degrades
  instead of breaking.
