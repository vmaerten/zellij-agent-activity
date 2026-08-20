# Effects seam: testable core and a sink abstraction for standalone

The plugin follows the namer's pattern (its ADR-0001): a pure core
(`init` / `handle` / `handle_pipe` take zellij events, return `Vec<Effect>`) and
a thin `ZellijPlugin` adapter gated to `#[cfg(target_arch = "wasm32")]` that
executes the effects. The pane→tab races are exercised as native unit tests
instead of in a live session.

Until `zellij-tile` 0.44 the seam enforced itself: host functions were extern
symbols that only existed on wasm, so a host call in the core failed to link
natively. 0.45 stubs `host_run_plugin_command` out to a no-op on native, and
that guarantee is gone: the call now compiles, writes JSON to a stdout nobody
reads, and the suite stays green while the plugin misbehaves on wasm.
`clippy.toml` replaces it, listing the host functions as `disallowed-methods`;
`task ci` already runs `clippy -D warnings`, and `drive()` carries the only
allow. The list is nominative, so extend it when the adapter learns a new host
call.

The key effect is deliberately abstract:
`Effect::ShowActivity { tab_id, symbol: Option<String> }`: "this tab should
show this symbol (or none)". *How* is the sink's job. v1 ships one sink that
realises it by piping `set_prefix` / `clear_prefix` to the namer. This is the
seam that lets the plugin be published without forcing the namer on everyone: a
future **standalone** build adds a `rename` sink that realises the same effect
with `rename_tab_with_id` (delta-driven, same event-driven model). ADR-0008
builds it, and corrects "*How* is the sink's job": the sink turned out to be a
strategy of the **core**, not of the adapter: the invariant here is that the
adapter decides nothing, not that the sink lives in it.

## Consequences

- A `statusbar` sink (render its own powerline bar, zellaude-style) is
  explicitly **not** the standalone direction: it would reintroduce a render
  loop and rebuild the very thing we chose not to be. `rename` keeps the
  delta-driven, zero-wakeup shape.
- Native `[[bin]]` builds need the empty
  `#[cfg(not(target_arch = "wasm32"))] fn main() {}`; the real entrypoint is
  `register_plugin!` on wasm. `.cargo/config.toml` uses a `wasm` alias rather
  than forcing `wasm32-wasip1`, so `cargo test` runs the core natively. Don't
  "fix" this back.
- The core emits `ShowActivity` only when a tab's winning symbol *changes*, so
  redundant events don't spam the namer with pipes.
