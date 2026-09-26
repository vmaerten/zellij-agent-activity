# Codex shared-daemon routing: experiment results

Both branches start from `main` at `af85530`. They are independent experiments, not a stack.
The original checkout's local deletion of `.cargo/config.toml` and edits to troubleshooting and
Codex producer documentation were preserved; neither worktree includes those uncommitted changes.

| Track | Branch | Entry point | Result |
|---|---|---|---|
| Native context | `explore/codex-daemon-native` | `producers/codex/experiments/native/README.md` | Simulated upstream lifecycle and tested producer fanout; requires a Codex change |
| Local proxy | `explore/codex-daemon-proxy` | `producers/codex/codex-zellij/README.md` | Host Rust launcher and proxy work in the lab; unrestricted hook coexistence is blocked |

The worktrees are `/private/tmp/zellij-codex-daemon-native` and
`/private/tmp/zellij-codex-daemon-proxy`. Each branch contains this comparison; switch worktrees to
run that track's task commands. Both keep the existing WASM code, wire protocol, tab priority,
and symbols. Neither adds textual progress messages to the product.

## What the evidence establishes

Native reproduction: two actual subprocess clients send synthetic view selections to a simulated
server process that inherited the first client's environment. The existing hook targets pane 26
for client B even though B captured pane 46 in another session. Supplying the proposed server-owned
view context routes to B, fans out when A also views B's conversation, and removes B on disconnect.
The experiment tests five lifecycle/normalizer/sink scenarios plus the existing producer suite.
It was **not** validated with a modified Codex binary or real native client-context payloads.

Proxy: the standalone `codex-zellij` Rust crate builds independently from WASM. Eleven default Rust
tests cover observer state, reproducible JSONL input, launcher cleanup, and a simulated WebSocket
server/sink. Two opt-in tests exercise installed Codex and Zellij: two proxies attach to one
isolated app-server, create separate conversations in the same cwd, then view one shared
conversation; installed Zellij is also exercised against a unique absent session. No paid/model
inference is involved. There is no claim of visual acceptance in real panes or a complete real
TUI command/edit/approval session.

The additional Python probe proves a specific coexistence blocker on `codex-cli 0.157.0`:
new-thread config overrides suppress the selected plugin; loaded-thread resume config overrides
do not. `thread/settings/update.disabledPluginIds` suppresses later hooks without affecting a
second conversation, but an already-running hook still emits after that update returns. The
probe deliberately holds and releases its own marker hook to make the race deterministic.
It trusts only its own generated hook hashes in a disposable home.

Consequently the proxy does not enable production use or silently disable the producer globally.
It requires `--lab` and a test daemon without activity hooks. A new-thread-only product could use
the verified override, but that would narrow the requested resume/hot-attach scope. A full solution
needs coordinated ownership and draining of existing producer invocations, including multiple
views and restoration when the last view leaves. A producer registry could provide such a protocol,
but belongs to the deferred alternative below.

## Same acceptance scenarios, different evidence levels

| Scenario | Native branch | Proxy branch |
|---|---|---|
| Two conversations, same repository, different sessions | Subprocess reproduction and explicit destination assertions | Observer test; installed server creates distinct threads in one cwd; session routing tested through sink |
| One conversation in two panes | Simulated view fanout | Two observers plus two real proxy connections to one installed server |
| Command, edit, thinking, user wait, completed turn | Simulated state and existing canonical normalization tests | JSONL replay and observer tests; real model/tool turn not run |
| Parallel tools and multiple waiting requests | Simulated active item/request sets | Observer tests preserve surviving tool and waiting priority |
| Interruption and resume | Simulated state transition | Observer snapshot test and interrupted loopback turn on installed server |
| Fork and conversation switch | Simulated selection to a distinct identity | Correlated start/resume/fork responses, failed switch, old-thread filtering; actual TUI fork not run |
| Close view / disconnect | Simulated removal and `SessionEnd` | Launcher socket cleanup, unsubscribe and abrupt transport loss tests |
| Daemon restart | Simulated invalidation; crash handling still upstream work | Disconnect clears state; a new proxy connects to a restarted mock server; no transparent replay |
| Missing or blocked Zellij | Concurrent fanout bounded by existing two-second hook watchdog | Coalesced independent sink, bounded subprocess; installed absent-session test |
| Exclusive producer for managed conversations | Upstream must own delivery and cancel stale snapshots | Deterministic hot-attach blocker reproduced; normal use deliberately refused |

## Tradeoffs

| Dimension | Native context | Local proxy |
|---|---|---|
| Indicator fidelity | Can know authoritative view ownership, hook IDs and state once upstream implements it | Observes transport; incomplete paginated resume history and pre-selection events limit initial fidelity |
| Installation | Existing producer plus a future compatible Codex version | Extra native binary, launcher usage in each pane, one private socket per view |
| Robustness | Upstream can coordinate lifecycle and queued deliveries; prototype hooks still synchronously wait up to 2s | Slow/missing Zellij is independent of transport; daemon disconnect requires a new client |
| Maintenance | Small downstream adapter; upstream contract/version/testing responsibility | JSON-RPC method/item drift, TUI view-selection behavior, process and socket lifecycle |
| Upstream dependency | Required for the proposed client context and dispatch guarantees | Existing local WebSocket APIs suffice for transport; hook ownership/draining remains unresolved |
| Current adoption | No native compatibility claim from simulation | Laboratory only; not safe to combine with the current hook producer for arbitrary hot resume |

Native client context is the more direct long-term fit for authoritative multi-view routing.
The proxy establishes that local transport observation is viable and isolates the additional
problem that must be solved before it can coexist with hooks. Neither track is a production release.

## Validation record (2026-09-26)

Environment: macOS arm64, `codex-cli 0.157.0`, `zellij 0.45.1`, Rust `1.98.0`, Node `24.21.0`.
The existing user's running daemon was never queried or changed. Installed-server tests spawn
fresh processes from the CLI binary; they are not validation of a separately installed 0.157.1 daemon.

- `task ci` passed on both branches: formatting, clippy, producer tests, plugin/manifests checks,
  44 core Rust tests, and release WASM build. The initially missing `wasm32-wasip1` Rust target
  was installed, then the full checks passed.
- Native: `task codex-native-repro` and `task codex-native-check` passed (five additional Python tests).
- Proxy: formatting and clippy with warnings denied passed; eleven default Rust tests and two installed
  binary tests passed. `task codex-proxy-installed` also includes the deterministic Python blocker.
- Sandbox restrictions prevented Unix listeners on the first attempt. Socket tests were rerun with
  local socket access; failures were not skipped or counted as passing.

See each track's README for exact commands, runnable fixtures, constraints, and unvalidated behavior.
The native `UPSTREAM.md` links related Codex issues
[#42457](https://github.com/openai/codex/issues/42457) (daemon cwd inherited by hooks) and
[#44063](https://github.com/openai/codex/issues/44063) (disconnect lifecycle). These reports motivate
the proposal; they do not prove this implementation. Nothing was posted upstream.

## Deferred alternatives

A local registry could map thread IDs to live `(session, pane)` views and carry ownership/leases
that both hooks and launchers consult. It needs atomic registration, reference counting, stale
lease cleanup, permissions, and a drain/activation protocol. It is documented here only; there is
no registry prototype in either branch.

Terminal-title observation could give a coarse busy/done signal without hook routing changes.
It depends on TUI title behavior and title ownership, provides less tool/permission fidelity,
and cannot by itself establish multi-view conversation identity. It is also not prototyped.
