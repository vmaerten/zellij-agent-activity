# Roadmap

`zellij-agent-activity` works end-to-end with Claude Code (validated live) and
is ready to tag, with CI and a tag-driven release pipeline. It still lives on a
**private** repo. The name is harness-neutral by design — here's the path to a
public, multi-harness release.

## Before going public

- [ ] **Flip to public** — `gh repo edit vmaerten/zellij-agent-activity --visibility public --accept-visibility-change-consequences`.
      Every install path in the README depends on it: the release download 404s
      and `claude plugin marketplace add` can't clone while the repo is private.
- [ ] **Demo media** — screenshot/GIF (`docs/media/demo.gif`). It's a visual
      plugin whose README shows nothing.
- [ ] **GitHub topics** — none set; only possible once public.

## Design work

- [ ] **Standalone sink** (ADR-0004) — add a `rename` sink so the plugin works
  *without* `zellij-tab-namer` (owns the tab name itself, delta-driven). Keep the
  `pipe` sink as the integrated mode. This is the one that matters most: today
  the plugin does nothing on its own, which halves who can use it.
- [ ] **Config surface** — make the symbols (and, if needed, the namer pipe
  target) configurable instead of hardcoded.
- [ ] **Tool mapping** — map more tools if the `⚙` fallback feels too coarse.

## Multi-harness — the reason for the neutral name

The core already normalizes events; each harness just needs a **forwarder hook**
that emits the same `agent_activity.v1` pipe (`pane_id`, `hook_event`,
`tool_name`, `ts_ms`). One small script per harness, each distributed as a native
extension of that harness under `producers/` (ADR-0005):

- [x] **Claude Code** — a Claude Code plugin (`producers/claude/`), installed via
  `claude plugin install`; the repo is its own marketplace.
- [ ] **Codex** — `~/.codex/hooks.json` (or the `notify` program).
- [ ] **Gemini CLI** — `.gemini/settings.json` hooks (`BeforeTool` / `AfterTool`,
  `AfterAgent`, `Notification`).
- [ ] **opencode** — a tiny TS plugin that shells out to the forwarder
  (`session.idle`, `tool.execute.*`, `permission.asked`).
- [x] **Document the wire format** so anyone can write a producer — see
  [The wire format](README.md#the-wire-format) in the README, and ADR-0007 for the
  producer-normalizes / core-decides split it rests on.

## Repo hygiene

- [x] **Version the pipe** — `agent_activity` → `agent_activity.v1` (the name is
  the version authority), done before the first distributed producer froze it.
- [x] **CI** — fmt, clippy, the producer tests, `cargo test` and `cargo wasm`.
- [x] **Release pipeline** — pushing a `v*` tag verifies the tag against
  `Cargo.toml`, builds the wasm and publishes it as a Release asset, with notes
  assembled from the git-cliff changelog. `task release` drives the whole thing.
- [x] **Renovate** — non-major updates land in one weekly PR; `zellij-tile` is
  carved out and reviewed on its own, since it pins the plugin ABI.

## Reach (optional)

- [ ] **Desktop notification / bell on `⚠`** — as an opt-in add-on to the hook
  (orthogonal to the plugin; no Rust changes).
- [ ] **awesome-zellij** entry — next to `zj-radar`; emphasize it's a *tab prefix*
  (not a sidebar) that *drives a namer* (doesn't own the tab name).

See [`docs/adr/`](docs/adr) for the design decisions.
