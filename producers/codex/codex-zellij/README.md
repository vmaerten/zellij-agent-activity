# codex-zellij: autonomous local-daemon candidate

This branch now contains a **cooperative producer and launcher**, without modifying Codex or the
WASM plugin. The original hot-resume blocker is fixed for the updated producer: hooks finish
already-started emissions before a proxy takes ownership, then stay silent for that conversation
until its last proxy view closes. Other conversations keep their hooks. No Codex plugin settings,
approvals, model traffic, or user config are changed by the launcher.

This remains an unreleased candidate on `explore/codex-daemon-proxy`. Install **both** halves from
this branch before trying it against your normal daemon. The published 0.1.0 producer does not
implement coordination, and merely installing the Rust executable is not sufficient.

## Build and run

```sh
cargo build --release --locked --manifest-path producers/codex/codex-zellij/Cargo.toml
# Or install the executable on PATH:
cargo install --path producers/codex/codex-zellij --locked
```

Stage a small local marketplace (only the producer files, not Rust build artifacts):

```sh
task codex-proxy-package
```

For an existing installation, replace the old plugin and its marketplace source using Codex's
own commands. Do this initial migration with the old clients stopped and the daemon idle:

```sh
# Omit these two removal commands for a first installation.
codex plugin remove zellij-agent-activity@zellij-agent-activity
codex plugin marketplace remove zellij-agent-activity
codex plugin marketplace add "$PWD/target/codex-proxy-marketplace"
codex plugin add zellij-agent-activity@zellij-agent-activity
```

Keep that local marketplace directory for later local upgrades. This unreleased branch has not
bumped the published package version, so an existing 0.1.0 cache must be explicitly replaced;
merely refreshing the version cannot install these files. Reload/restart the idle daemon once
for this migration, then approve updated hooks if Codex requests it. Already-running old scripts
cannot acquire a lock retroactively. The launcher checks enabled hooks and refuses the known
uncoordinated producer. These steps were exercised in disposable homes; this work has **not**
installed or changed anything in your normal Codex home.

In a Zellij pane, with the shared daemon already running:

```sh
codex-zellij
codex-zellij -- resume
# Explicit endpoint for an isolated or nondefault server:
codex-zellij --daemon unix:///absolute/path/server.sock -- --no-alt-screen
```

Default daemon endpoint: `$CODEX_HOME/app-server-control/app-server-control.sock`, or
`$HOME/.codex/app-server-control/app-server-control.sock`. Each invocation captures its own
`ZELLIJ_SESSION_NAME` and `ZELLIJ_PANE_ID`, opens a private mode-0600 Unix socket in a mode-0700
directory, then launches the real `codex --remote` TUI. It joins an existing daemon and never
starts, stops, or updates it. Additional Codex arguments follow `--`. `--remote` and `--no-daemon`
cannot be forwarded because the launcher owns that connection.

Requirements: Unix/macOS, Rust for building, installed Codex and Zellij, and Python 3/bash/jq for
the updated hook producer. The preflight reads the current cwd's hook inventory through a separate
connection. It recognizes this repository's standard plugin and rejects known old/manual activity
hooks; it is not a universal audit of arbitrarily named custom scripts or other project configs.

## Why the handoff works

The shared directory is `$HOME/.local/state/zellij-agent-activity/codex-v1`. For isolated tests,
`ZELLIJ_AGENT_ACTIVITY_COORDINATION_DIR` overrides it; **daemon and launcher must use the same
absolute directory**. It must belong to the current user and have mode 0700. Lock filenames encode
thread IDs as UTF-8 hex. They contain no conversation contents or Zellij destinations.

Each conversation uses a gate and an ownership file. A hook locks the gate, probes exclusive
ownership, and emits only if no view owns the conversation. The gate stays locked through the
actual `zellij pipe` subprocess. Lock descriptors are inherited by the emitting shell so killing
the Python wrapper cannot release its gate before a surviving emitter finishes.

A proxy's sink acquires the gate, waits for any existing emitter to finish, takes a shared
ownership lock, then releases the gate. Multiple views can hold shared ownership together.
Subsequent hooks detect that ownership and return without sending. Closing one view leaves the
others intact; closing the last automatically restores legacy emission. OS locks avoid stale PID
files, clocks, heartbeats, or config restoration races. Empty lock files are deliberately retained:
unlinking them while another process still has the inode open would split the lock domain.

This is local **producer ownership coordination**, not the deferred registry that routes events
from hooks to a stored list of pane destinations. Routing still comes from each proxy's connection.
Events completed before handoff are legacy events; this does not repair an old incorrect indicator
already left in another pane. The no-overlap guarantee starts after ownership is acquired.

Ownership acquisition and Zellij sending run in the coalescing sink worker, independently of
WebSocket forwarding. An unresponsive consumer has a two-second subprocess deadline. Draining
has a four-second deadline; failure leaves the proxy's activity disabled for that update and reports
an error, while Codex transport continues. Selection changes during draining discard stale activity.
Cleanup sends `SessionEnd` before releasing ownership and removes the private socket.

The launcher never answers server requests or changes Codex frames. Approvals, answers, unknown
JSON, and binary messages are relayed. Successful start/resume/fork responses select the view;
errors preserve its previous selection. Active tools are tracked separately, waits outrank tools,
and only a server resolution clears a pending request. No prompt/tool content is logged.

## Verification

```sh
task codex-proxy-check
task codex-proxy-installed
task ci
```

`codex-proxy-check` runs Rust formatting, clippy, tests, and the Python handoff regression using
the actual Rust ownership implementation in a subprocess. Its fake Zellij deliberately holds an
old producer in flight, then verifies draining, per-conversation suppression, unrelated threads,
multiple views, and ownership recovery after killing lease holders. The driver in
`tests/support/hold_lease.rs` is only a test example, not an installed launcher command.

`codex-proxy-installed` runs the real local app-server protocol tests and two variants of the
installed-Codex probe:

```sh
python3 producers/codex/codex-zellij/tests/probe_installed.py
python3 producers/codex/codex-zellij/tests/probe_installed.py --coordinated
```

The first intentionally preserves the original failure as a control: changing `disabledPluginIds`
does not stop an already-running hook. The second uses the real updated shell producer and Rust
ownership code with installed Codex; it verifies hot resume, no competing emissions after handoff,
an unaffected second conversation, two owners, and automatic restoration after the last exits.
The task also stages and installs the real producer in another disposable home, then starts the
installed Codex TUI through the normal launcher in a pseudo-terminal. That smoke test verifies a
real conversation selection and clean SIGTERM shutdown, including the normal compatibility check.

Neither test copies credentials or contacts a model: a deliberately unavailable loopback provider
lets hooks and interruption execute. Each uses a disposable home/marketplace and trusts only its
own generated marker-hook hashes. The real user's daemon is never touched.

Rust tests separately validate exact bidirectional messages, permission forwarding, thread selection,
parallel tools, sink coalescing, ownership/security checks, switch during drain, process/socket
cleanup, and installed Zellij with an absent session. These tests require Unix socket permission.
`--lab` skips preflight **only for isolated tests**; it no longer bypasses coordination.

## Remaining acceptance work

The installed-Codex coexistence test drives the app-server and actual ownership/producer paths;
the separate TUI smoke test covers startup and shutdown without model inference. Visual acceptance
in two real Zellij panes and real command/edit/approval turns remain to be done. Local packaging
and fresh installation are exercised; an upgrade of the user’s running setup is still manual. A paginated resume can
omit active tool history, so a newly attached view may initially show thinking until the next item.
Pre-selection notifications and background TUI resume/prefetch behavior also limit initial fidelity.

Ordinary exits, SIGINT, SIGTERM, view switches and daemon disconnect clear/release the view. Daemon
restart requires a fresh client; non-idempotent RPCs are never retried silently. A hard SIGKILL of
the whole launcher cannot guarantee its final `SessionEnd` or socket cleanup, and an in-flight
Zellij child may outlive that kill; clean shutdown is required for the full delivery guarantee.
Crashing an idle lease holder does release its OS ownership automatically, as tested.

See [the updated comparison](../../../docs/CODEX-DAEMON-COMPARISON.md).
Protocol reference: [Codex App Server](https://learn.chatgpt.com/docs/app-server).
