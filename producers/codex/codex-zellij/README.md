# codex-zellij: local proxy experiment

Throwaway host Rust executable on `explore/codex-daemon-proxy`, based on `main` at `af85530`.
**Blocked for normal installation: attaching an already-active conversation cannot fence an
existing producer hook that is already running.** The executable requires explicit `--lab` use
with an isolated server without the activity hook. It does not attempt to disable hooks or alter
any user's configuration. The existing plugin, producer, and `agent_activity.v1` remain unchanged.

```sh
# From the repository root; builds separately from the WASM plugin.
cargo build --manifest-path producers/codex/codex-zellij/Cargo.toml
task codex-proxy-check
task codex-proxy-installed
task ci
```

The installed-server test and Python probe allocate disposable `CODEX_HOME` directories and
terminate only their own child processes. They do not attach to, restart, or update the user's
daemon. A deliberately unavailable loopback model provider lets hooks and interruption run without
credentials or paid inference. Rust tests need Unix socket permission; a sandbox that forbids
local listeners must run these tests outside that restriction. Python, bash, jq, Codex, and the
repository's tools are needed for all checks. The Rust library tests alone do not need Codex.

## Lab launch

Use a disposable `CODEX_HOME`, no activity hooks, and one local server shared by the test clients:

```sh
# Terminal 1. Retain this path for the other test panes.
lab_dir=$(mktemp -d /tmp/codex-activity-lab.XXXXXX)
mkdir "$lab_dir/home"
CODEX_HOME="$lab_dir/home" codex app-server --listen "unix://$lab_dir/server.sock"
```

In each test Zellij pane, set `lab_dir` to that printed/retained directory path, then:

```sh
CODEX_HOME="$lab_dir/home" producers/codex/codex-zellij/target/debug/codex-zellij \
  --lab --daemon "unix://$lab_dir/server.sock" -- --no-alt-screen
```

The normal Codex client UI may require authentication for inference; the automated tests do not.
Stop the test clients and test server before deleting the disposable directory. No global daemon
management command is needed. The `--lab` flag acknowledges this experimental scope; it does not
inspect the supplied daemon to prove isolation. Never point this prototype at a daemon with the
activity producer enabled.

The launcher captures session and pane, creates a mode-0700 temporary directory containing a
mode-0600 Unix socket, and runs `codex --remote unix://<private-socket>` with inherited terminal I/O.
Only explicit absolute Unix daemon endpoints are accepted. It forwards application messages in
both directions, including unknown messages, binary messages, approvals and answers. It never
answers a server request or injects an approval. It parses JSON only for local observation and
does not log prompt, response, command, or tool payloads.

Each successful start/resume/fork selects a conversation for that launcher instance. Request IDs
are tracked by direction; failures keep the old selection. Other threads and subagents cannot
move the view through notifications. Successful unsubscribe clears it. Resume snapshots hydrate
active items when present. A per-thread active-item set prevents a parallel completion from erasing
another tool; pending requests and thread-level waiting flags have priority. Permission replies do
not clear a wait until the server resolves it. Interruption/completion clears transient activity.

The Zellij worker consumes coalesced snapshots, with at most one subprocess and a two-second
timeout, explicit `--session`, closed stdin, and monotonic timestamps. Missing/blocked Zellij cannot
hold up Codex forwarding. Cleanup emits `SessionEnd`, kills/reaps the sink child, and removes the
private socket. Ordinary exit, SIGINT, and SIGTERM are handled; SIGKILL cannot guarantee cleanup.
Daemon disconnect terminates this view and clears its indicator. Restart requires launching a
fresh client; the proxy never silently retries non-idempotent RPCs.

## Reproduced suppression blocker

Run `python3 producers/codex/codex-zellij/tests/probe_installed.py`.
The test creates its own local marketplace with one vetted marker hook, installs it only into
a temporary home, and trusts exactly its two generated hook hashes. It records only thread IDs
and event names. On installed `codex-cli 0.157.0`, the assertions demonstrate:

1. `thread/start.config["plugins.activity-probe@activity-probe.enabled"] = false` prevents hooks
   for that new thread. The JSON override key uses an unquoted dotted path.
2. The same override in `thread/resume` of a loaded thread does **not** prevent its next hook.
3. `thread/settings/update.disabledPluginIds` does prevent subsequent hooks for that thread,
   while an unrelated thread still emits. This is stronger behavior than the generated
   `ThreadStartResponse` comment ("Does not yet filter plugin capabilities") suggests.
4. A hook deliberately held in flight before resume still emits **after** the settings RPC has
   returned and the test releases it. Thus a proxy-only opt-out does not revoke existing producers.

This is a blocker for the requested unrestricted hot-attach/coexistence scope, not proof that every
possible autonomous solution is impossible. A narrower new-thread-only launcher could use the
verified config override. A general solution needs a producer ownership/drain protocol, or a safe
way to reject/coordinate hot attachment and restore ownership after the final view closes.
Neither global disabling nor changing the daemon environment preserves unrelated conversations.
A filesystem registry would be another design, intentionally not prototyped here.

## Evidence and remaining limits

`tests/trace.jsonl` and `tests/scenarios.rs` cover thread identity, tools, waits, switching,
fork selection, resume snapshots, interruption and two views. `tests/transport.rs` uses a simulated
WebSocket server and tests exact application-message forwarding, no fabricated permission replies,
disconnect, reconnection with a new proxy, missing consumers and bounded/coalesced sink delivery.
`tests/installed.rs` connects two proxies to one **installed** Codex app-server in a disposable home,
creates distinct threads in the same cwd, persists an interrupted turn, then attaches the second
view to the first thread. A second installed test targets a unique absent Zellij session. It does not claim a complete TUI/model/tool integration test.

No live model turn with real command/edit/approval execution or visual assertion in Zellij was
performed. The launcher uses the real TUI's documented `--remote` entry point, but the installed
integration test drives the protocol directly. Full live active-tool history may be absent from a
paginated resume response; such a view can show thinking until the next observed item. Notifications
before selection and TUI background resume/prefetch behavior remain further fidelity limits.
The lab restriction stays in place even though the simulated scenarios pass.

See [the comparison](../../../docs/CODEX-DAEMON-COMPARISON.md).
Protocol reference: [Codex App Server](https://learn.chatgpt.com/docs/app-server); the experiment
also uses schemas generated locally by `codex app-server generate-json-schema --experimental`.
