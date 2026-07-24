# The versioned pipe contract — compatibility by tolerance, not lockstep

Once the producer ships as its own artifact (ADR-0005), the producer and the
consumer update independently and *will* drift: a Claude Code plugin can update
before the Zellij plugin, or after. The naive fix — "check the two versions
match, refuse if not" — rebuilds the coupling ADR-0005 just removed, and is
anyway impossible with several producers (Claude, Codex, opencode each on their
own version; no one maintains a producer×consumer compatibility matrix). So the
versions are **not** tied. There are three independent lines, and only the last
is a contract:

- **Producer semver** — the harness extension's own version (`plugin.json`).
- **Consumer semver** — the wasm plugin's own version (`Cargo.toml`).
- **Protocol major** — the pipe name (`agent_activity.v1`). *This* is the public
  API between them, and the only thing that must be coordinated.

The forwarder's semver and the wasm's semver need not correspond: forwarder
0.1→0.9 and wasm 0.1→0.9 all speak `agent_activity.v1`. The protocol bumps only
on a **breaking wire change**, decoupled from either semver.

Compatibility is achieved by **tolerance**, and the consumer already has the
shape for it (`main.rs`): an unknown `hook_event` returns `None` — "leave the
pane unchanged" — rather than erroring; an unknown `tool_name` falls back to
`⚙`; an absent `ts_ms` is never dropped; unknown args in the `BTreeMap` are
ignored. From that the evolution rule follows:

> **Within a major, changes are additive and optional only.**

This makes drift order-independent *by construction*. Take adding a field (say
`cwd`): a new consumer with an old producer sees `args.get("cwd") == None` and
uses the old behaviour; an old consumer with a new producer ignores the extra
arg. Whichever side updates first, nothing breaks — no handshake needed.

A change that cannot be additive (renaming a field, changing a value's meaning)
is a **new major**: bump the pipe name to `agent_activity.v2`. During the
transition the consumer subscribes to **both** v1 and v2; producers migrate at
their own pace; a v1-only producer keeps talking to the v1 listener; v1 is
dropped from the consumer later, once no v1 producers are expected. An old
consumer simply never receives v2 (it is not subscribed) — silence, never
misinterpretation.

## Consequences

- Rename the pipe `agent_activity` → **`agent_activity.v1`** (`PIPE_NAME` in
  `main.rs` and the `zellij pipe --name` in the forwarder), promoting it from an
  internal name to the public contract. This is the ROADMAP "Version the pipe"
  item, done now — before third-party producers exist — so v1 is the baseline
  everyone writes against.
- **No strict version check.** The only sanctioned "verif" is a best-effort
  *forward* detection: the producer may send an optional `proto=1` arg; if the
  consumer ever sees a `proto` higher than it knows, it *warns* (e.g. a distinct
  prefix / log — "producer too new, update the Zellij plugin") but still runs.
  A warning, never a refusal — a refusal would reintroduce lockstep. `proto` is
  optional; the pipe-name-as-major plus the additive rule already carry
  correctness, so `proto` is a diagnostic aid, not control flow.
- The wire format (`pane_id`, `hook_event`, `tool_name`, `ts_ms`) is now a
  documented public contract, not an implementation detail: it is what a
  third-party producer writes against, and what the additive rule protects.
- Any producer added under `producers/` (ADR-0005) targets `agent_activity.v1`
  and obeys the additive rule; it needs no knowledge of the consumer's version.
