# The opencode producer is a stateful plugin, not a stateless hook

ADR-0005 settled that each harness's producer is distributed as a native
extension of that harness, and it already flagged the exception: "opencode has no
shell hook at all (it is a TS plugin)". Both other producers are the same shape:
a manifest of `command` hooks pointing at a `forwarder.sh` that reads a JSON
payload on stdin and exits. opencode offers nothing of the kind. Its extension
point is a **program**: a `.js`/`.ts` file loaded into the opencode server, whose
exports are called back as the session runs.

That difference forces three decisions the shell producers never had to make.

## There is no forwarder script

The obvious way to keep the family resemblance is a thin plugin that shells out
to a `forwarder.sh`, so the translation table stays in one language across the
three producers. Rejected: the plugin would have to resolve a sibling file's path
at runtime, which turns a one-file install into "clone the repo", and it spends a
process per event to reach a runtime we are already inside.

So the producer is **one self-contained file**, and the two guards move into it.
The watchdog around `zellij pipe`, the one that stops a stuck plugin leaking a
file descriptor per event until the zellij server hits `EMFILE` (ADR-0003),
becomes `spawn(…, { timeout })`. The call is deliberately **not awaited**:
unlike a hook process, a plugin call sits inside the agent's own event loop, and
`ts_ms` already keeps the wasm's ordering safe. Only `dispose` awaits, because
the process is about to exit.

Spawning from a runtime rather than a shell brought back a lesson the shell
producers get for free. `zellij pipe` also accepts a payload on **stdin** and
blocks until EOF, and a hook script inherits a stdin that is already at EOF. A
plugin does not: the default pipe stays open, so every message hung until the
watchdog killed it, and none were ever delivered. `stdio: "ignore"` is therefore
load-bearing, and the spawn result is reported into the debug log instead of
being discarded: a producer whose only failure mode is silence is a producer
nobody can debug.

The file must also export **exactly one thing**. opencode's loader treats every
export as a plugin function and throws on anything else, so there is no exported
helper to unit-test directly. The producer therefore keeps the
`ZELLIJ_AGENT_ACTIVITY_DRY_RUN` seam the shell producers have, printing the args
instead of piping them, and the tests drive the real hooks through it.

## Subagents force the producer to keep state

ADR-0007 established that a subagent says nothing about the agent that owns the
pane, and enforced it by *not registering* `SubagentStop`. opencode gives no such
choice: a subagent is a child **session**, and it emits the same
`tool.execute.*`, `chat.message` and `session.idle` as the main one, through the
same hooks. A child's `session.idle` would post a `✓` while the main agent is
still working, precisely the bug ADR-0007 exists to prevent, arriving through a
door that cannot be closed at registration time.

The only signal available is `session.created`, which carries `parentID`. So the
producer keeps the set of session ids it has seen born with a parent and drops
everything they emit. Sessions it never saw being born are reported: that is the
resumed root session, created before the plugin loaded. A child session is always
born during the run, so it is always seen: the asymmetry is safe in the
direction that matters.

This makes it **the first producer with state**, and the reason it is worth an
ADR rather than a comment: "the producer normalizes, the core decides" was cheap
while normalizing was a `case` statement. Here it costs a `Set` that lives as
long as the server. The alternative, teaching the core about parent sessions,
puts harness structure back inside the wasm, which is the thing ADR-0005,
ADR-0007 and ADR-0010 each refused in turn.

For the same reason the producer sends **nothing on load**. opencode instantiates
the plugin more than once, and an instance created during shutdown emitted a
`SessionStart` *after* the `dispose` that had just cleared the pane, leaving a
stray `◆` on the tab. Observed, not theorised. `session.created` is the honest
start-of-session signal; the cost is that `opencode --continue` shows no `◆`
until the first prompt.

## A file in the plugins directory is the native mechanism

ADR-0005's rule is "installed by that harness's own tooling", which for the other
two means a marketplace and an install command. opencode loads any file dropped
in `~/.config/opencode/plugins/` or `.opencode/plugins/` at startup: no config
entry, no approval step. That *is* its native mechanism, so the rule holds even
though nothing here is a marketplace.

The versioned alternative is publishing to npm and listing the package in
`opencode.json`. It buys a pinned version and an upgrade command, and costs an
npm account, a publish step in the release workflow and a package name to hold.
Not worth it at this size; a `curl` that overwrites the file is the upgrade, and
npm stays available if pinning ever becomes a real need.

## Consequences

- `producers/opencode/` holds no `hooks.json`, no `plugin.json` and no
  `forwarder.sh`, just the plugin, its tests and a README. The version lockstep
  that `task ci` enforces on the other two manifests is enforced here on the
  file's `VERSION` constant instead.
- The tests are the first JavaScript in the repo. They run on `node --test` with
  no dependencies, no `package.json` and no lockfile, so the toolchain cost is a
  node binary that CI already has.
- No marketplace file changes. In particular
  `.agents/plugins/marketplace.json` stays Codex-only, and the CI guard asserting
  that is untouched.
- The debug log gains a `dropped` field, because "a subagent's event was
  discarded" is now a decision the producer makes and therefore one you must be
  able to read back.
