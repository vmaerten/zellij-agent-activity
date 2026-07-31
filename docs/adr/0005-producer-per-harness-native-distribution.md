# Producer per harness, distributed natively — the plugin never self-installs

ADR-0003 keeps the hook a **dumb forwarder** so it is "trivial to replicate per
harness later". This ADR settles *how* that replication is distributed, and
reverses the original self-install.

The Zellij plugin (the **consumer**) owns exactly one thing: `TabInfo.name`
decoration and the pipe contract it listens on. It must **not** reach into other
tools' config files. The first draft did the opposite: on the `RunCommands`
grant the plugin emitted an `Effect::RunCommand` that ran `installer.rs`'s
`INSTALL_TEMPLATE` — jq surgery, backup-first, symlink-resolve, version-tag — to
write the forwarder into `~/.claude/settings.json`. That bought a one-step
install at the cost of the plugin editing a file it does not own, an arbitrary
`sh` execution behind a permission, a `jq` dependency, and a messy uninstall.
It also does not generalise: there is no single mechanism that can register a
producer across Claude Code, Codex, Gemini and opencode — opencode has no shell
hook at all (it is a TS plugin), Codex uses a single `notify` program in
`config.toml`, each config shape is bespoke. A consumer that "knows the config
format of every harness" is a maintenance sink that edits files it does not own.

So the model is inverted: **each harness's producer is distributed as a native
extension of that harness**, in that harness's own format, installed by that
harness's own tooling. For Claude Code this is a **Claude Code plugin** —
`.claude-plugin/plugin.json` + `hooks/hooks.json` registering the events on
`"${CLAUDE_PLUGIN_ROOT}"/scripts/forwarder.sh`. The forwarder is the *same*
script (watchdog and all, per ADR-0003), only now bundled and referenced by
`${CLAUDE_PLUGIN_ROOT}` instead of copied in by jq. Claude Code owns the merge
with the user's `settings.json`, so there is no clobber risk and uninstall is
`claude plugin uninstall`.

One repo hosts both roles: it *is* a marketplace (`.claude-plugin/
marketplace.json`) and carries the producers in a `producers/` subtree, so the
Claude user runs two non-interactive lines — `claude plugin marketplace add
vmaerten/zellij-agent-activity` then `claude plugin install …` — and future
producers (`producers/codex/`, `producers/opencode/`) live beside the first.

## Consequences

- **Deleted, not kept in parallel:** `src/installer.rs`, the `INSTALL_TEMPLATE`,
  `Effect::RunCommand`, and its emission on the permission grant
  (`main.rs` `PermissionRequestResult::Granted`) all go. Keeping self-install
  *and* the Claude Code plugin would mean maintaining two Claude producers for
  the same job — rejected.
- The plugin's permission set shrinks from four to three: `ReadApplicationState`,
  `MessageAndLaunchOtherPlugins` and `ReadCliPipes` stay; **`RunCommands` is
  dropped** (it only
  ever served the install). Smaller trust surface. Update `init` and the
  `init_requests_permissions_and_subscribes` / `granted_permission_installs_the_hook`
  tests accordingly.
- The accepted cost is **two install steps** for the Claude user (Zellij plugin
  *and* the Claude Code plugin) instead of one. This is honest: the producer and
  the consumer are genuinely two processes in two tools. The marketplace
  one-liner keeps it low-friction; there is no zero-step path that does not put
  the consumer back to editing another tool's config.
- Repo layout: `producers/claude/{.claude-plugin/plugin.json, hooks/hooks.json,
  scripts/forwarder.sh}` and a top-level `marketplace.json`. `scripts/
  zellij-agent-activity-hook.sh` moves under `producers/claude/scripts/` (its
  header comment about "the plugin's installer does this for you" is removed).
- This supersedes the self-install and amends the ROADMAP's "Multi-harness"
  section: each new harness is a new entry under `producers/`, distributed its
  own way, never a new branch inside the consumer. The pipe stays the only thing
  they share — see ADR-0006.
