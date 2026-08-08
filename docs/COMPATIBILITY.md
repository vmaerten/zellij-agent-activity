# Compatibility

Which plugin release works with which Zellij, and what to do when either moves.

## Matrix

| Plugin | `zellij-tile` | Zellij tested | Pipe protocol |
|---|---|---|---|
| 0.1.x | 0.44.3 | 0.44.3 | `agent_activity.v1` |

Every GitHub Release links back here, and states the `zellij-tile` version it was built against —
that line is derived from `Cargo.lock` at release time, so it can never drift from the artifact.

## Why the Zellij version matters

A Zellij plugin is wasm compiled against [`zellij-tile`](https://crates.io/crates/zellij-tile),
and **that dependency defines the plugin ABI the Zellij host expects**. There is no runtime
negotiation: if the host is too old, or the plugin API changed, the plugin fails to load or
misbehaves. So the `zellij-tile` version *is* the compatibility contract with Zellij.

## Policy — pin the minor, float the patch

`Cargo.toml` declares `zellij-tile = "0.44"`:

- **Patch releases float** (0.44.0 → 0.44.3): picked up automatically, no plugin release needed.
- **The minor is the anchor** (0.44 → 0.45): a bump can raise the floor or break the ABI, so it is
  treated as **at least a minor bump of the plugin**, with a new row in the matrix above and an
  updated floor in the README.

The stated floor ("requires Zellij >= 0.44.3") has an implicit ceiling at the next Zellij minor:
a newer Zellij minor may require a rebuild against the matching `zellij-tile`.

Only **one** Zellij minor is targeted at a time — the current stable. If supporting several ever
becomes necessary, the release would ship one `.wasm` per minor (suffixed asset), not a runtime
switch.

## The other version axes

This matrix covers the plugin ↔ Zellij axis. Three others exist and move independently — see
[ADR-0006](adr/0006-versioned-pipe-contract-compat-by-tolerance.md):

| Axis | Authority | Coordinated with the plugin? |
|---|---|---|
| Consumer (this plugin) | `Cargo.toml` | — |
| Zellij host API | `zellij-tile` minor | **yes** — this document |
| Producers (one per harness) | each producer's own manifest, or its `VERSION` | numbered in lockstep, but never checked |
| Pipe protocol | pipe name `agent_activity.vN` | only on a breaking wire change |

Every producer in this repo carries the same version string as the crate, because one number is
easier to quote in a bug report than one per harness — and because a harness *pins* a plugin on that
string, so it has to move every release for users to receive the producer at all. The opencode
producer has no manifest to pin, so it carries the number in the plugin file itself. `task bump`
moves them all together, and CI fails on drift.

That is a release convention, not a contract: **nothing checks that the halves match**, and they
routinely won't, since they install through different channels and at different times.
Compatibility comes from tolerance instead (unknown events and unknown args are ignored, never
fatal), so either side can update first. Only a breaking wire change bumps the protocol, and it
does so by renaming the pipe.

## Migration notes

### 0.1.x

Initial release. Requires [`zellij-tab-namer`](https://github.com/vmaerten/zellij-tab-namer)
loaded alongside — the plugin decorates through it and does nothing on its own.
