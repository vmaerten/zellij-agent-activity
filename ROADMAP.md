# Roadmap

`zellij-agent-activity` works end-to-end with Claude Code (validated live) and
lives on a **private** repo. The name is harness-neutral by design — here's the
path to a public, multi-harness release.

## Before going public

- [ ] **Flip to public** — `gh repo edit vmaerten/zellij-agent-activity --visibility public --accept-visibility-change-consequences`.
- [ ] **Demo media** — screenshot/GIF (`docs/media/demo.gif`).
- [ ] **Config surface** — make the symbols (and, if needed, the namer pipe
  target) configurable instead of hardcoded.

## Multi-harness — the reason for the neutral name

The core already normalizes events; each harness just needs a **forwarder hook**
that emits the same `agent_activity` pipe (`pane_id`, `hook_event`, `tool_name`,
`ts_ms`). One small script per harness:

- [ ] **Codex** — `~/.codex/hooks.json` (or the `notify` program).
- [ ] **Gemini CLI** — `.gemini/settings.json` hooks (`BeforeTool` / `AfterTool`,
  `AfterAgent`, `Notification`).
- [ ] **opencode** — a tiny TS plugin that shells out to the forwarder
  (`session.idle`, `tool.execute.*`, `permission.asked`).
- [ ] **Document the wire format** so anyone can write a producer.

## Robustness / API

- [ ] **Version the pipe** — `agent_activity` → `agent_activity.v1` (the name is
  the version authority), once third-party producers exist.
- [ ] **Standalone sink** (ADR-0004) — add a `rename` sink so the plugin works
  *without* `zellij-tab-namer` (owns the tab name itself, delta-driven). Keep the
  `pipe` sink as the integrated mode.
- [ ] **Tool mapping** — map more tools if the `⚙` fallback feels too coarse.

## Reach (optional)

- [ ] **Desktop notification / bell on `⚠`** — as an opt-in add-on to the hook
  (orthogonal to the plugin; no Rust changes).
- [ ] **CI** — `cargo test` + `cargo wasm`.
- [ ] **awesome-zellij** entry — next to `zj-radar`; emphasize it's a *tab prefix*
  (not a sidebar) that *drives a namer* (doesn't own the tab name).

See [`docs/adr/`](docs/adr) for the design decisions.
