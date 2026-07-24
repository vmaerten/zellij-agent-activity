// On native (cargo test) only the pure core is compiled; without the wasm
// entrypoint most items look unused to rustc.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::collections::{BTreeMap, HashMap, HashSet};
use zellij_tile::prelude::*;

mod installer;

/// Pipe name the hook forwards Claude activity on.
const PIPE_NAME: &str = "agent_activity";

/// Everything the plugin can do to the world. The core only emits these; the
/// wasm adapter's `drive` is the sole place they touch the zellij host — so the
/// core below compiles and is tested natively, free of host calls (ADR-0004).
#[derive(Debug, Clone, PartialEq)]
enum Effect {
    RequestPermissions(Vec<PermissionType>),
    Subscribe(Vec<EventType>),
    /// Show (`Some(prefix)`) or clear (`None`) a tab's activity prefix. The sink
    /// decides how — v1 pipes `set_prefix`/`clear_prefix` to zellij-tab-namer.
    ShowActivity {
        tab_id: usize,
        prefix: Option<String>,
    },
    /// Run a host command (fire-and-forget) — used once at load to auto-install
    /// the Claude Code hook into `~/.claude/settings.json`.
    RunCommand(Vec<String>, BTreeMap<String, String>),
    UnblockCliPipe(String),
    /// A diagnostic line. Only ever emitted when the plugin is loaded with
    /// `debug true`; the adapter writes it to stderr, which zellij captures in
    /// its log. Being an effect keeps the tracing testable and host-free.
    Log(String),
}

/// Queue a diagnostic line. Costs a bool check when debug is off — the `format!`
/// is never evaluated, so instrumentation can be liberal.
macro_rules! trace {
    ($state:expr, $($arg:tt)*) => {
        if $state.debug {
            $state.effects.push(Effect::Log(format!($($arg)*)));
        }
    };
}

/// What an agent is doing in a pane. Aggregated per tab by max priority so a
/// background `Thinking` never masks a foreground `Waiting` (ADR-0002).
#[derive(Debug, Clone, PartialEq)]
enum Activity {
    Init,
    Thinking,
    Tool(String),
    Waiting,
    Done,
}

impl Activity {
    fn priority(&self) -> u8 {
        match self {
            Activity::Waiting => 4,
            Activity::Tool(_) => 3,
            Activity::Thinking => 2,
            Activity::Init => 1,
            Activity::Done => 0,
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Activity::Init => "◆",
            Activity::Thinking => "●",
            Activity::Tool(name) => tool_symbol(name),
            Activity::Waiting => "⚠",
            Activity::Done => "✓",
        }
    }
}

fn tool_symbol(name: &str) -> &'static str {
    match name {
        "Bash" => "⚡",
        "Read" | "Glob" | "Grep" => "◉",
        "Edit" | "Write" | "MultiEdit" => "✎",
        "Task" => "⊜",
        "WebSearch" | "WebFetch" => "◈",
        _ => "⚙",
    }
}

/// Map a Claude Code hook event (+ tool name for `PreToolUse`) to an activity.
/// `None` means "leave the pane's activity unchanged" (unknown events).
/// `SessionEnd` is handled separately (clear). `Notification` is the event
/// Claude fires whenever it needs the user (permission prompt or idle nudge), so
/// it is the `Waiting` signal — not informational (ADR-0003, amended).
///
/// `SubagentStop` maps to nothing on purpose: a `Task` subagent finishing says
/// nothing about the main agent, which shares the same pane and may well be
/// mid-tool or blocked on a permission prompt. Treating it as `Done` flipped the
/// tab to `✓` while Claude was in fact waiting on the user (ADR-0003, amended).
fn activity_from_event(event: &str, tool: &str) -> Option<Activity> {
    Some(match event {
        "SessionStart" => Activity::Init,
        "PreToolUse" => Activity::Tool(tool.to_string()),
        "PostToolUse" | "UserPromptSubmit" => Activity::Thinking,
        "Notification" => Activity::Waiting,
        "Stop" => Activity::Done,
        _ => return None,
    })
}

#[derive(Default)]
struct State {
    /// Effects queued by the current call, drained into its return value.
    effects: Vec<Effect>,
    /// tab position (PaneManifest key) → stable tab_id, from the last TabUpdate.
    tab_id_by_pos: HashMap<usize, usize>,
    /// tab position → non-plugin pane ids, from the last PaneManifest.
    panes: HashMap<usize, Vec<u32>>,
    /// pane_id → stable tab_id (the namer addresses decorations by tab_id).
    pane_to_tab: HashMap<u32, usize>,
    /// pane_id → its current activity.
    pane_activity: HashMap<u32, Activity>,
    /// tab_id → last prefix pushed, to skip redundant pipe emissions.
    shown: HashMap<usize, String>,
    /// pane_id → last activity send-time (ms); drops events racing in out of
    /// order through parallel hook subprocesses (ADR-0003).
    last_ts: HashMap<u32, u64>,
    /// Emit `Effect::Log` diagnostics — set from the `debug` plugin config key.
    debug: bool,
}

#[cfg(target_arch = "wasm32")]
register_plugin!(State);

// ─── Adapter: the only place plugin behaviour touches the zellij host ───────
// wasm-only: the host functions are extern symbols that don't exist on native,
// so the linker itself guarantees the core below stays free of host calls.

#[cfg(target_arch = "wasm32")]
impl ZellijPlugin for State {
    fn load(&mut self, config: BTreeMap<String, String>) {
        let effects = self.init(&config);
        self.drive(effects);
    }

    fn update(&mut self, event: Event) -> bool {
        let effects = self.handle(event);
        self.drive(effects);
        false
    }

    fn pipe(&mut self, message: PipeMessage) -> bool {
        let effects = self.handle_pipe(message);
        self.drive(effects);
        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {}
}

#[cfg(target_arch = "wasm32")]
impl State {
    fn drive(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::RequestPermissions(perms) => request_permission(&perms),
                Effect::Subscribe(events) => subscribe(&events),
                Effect::RunCommand(command, context) => {
                    let argv: Vec<&str> = command.iter().map(String::as_str).collect();
                    run_command(&argv, context);
                }
                Effect::UnblockCliPipe(id) => unblock_cli_pipe_input(&id),
                // Plugin stdout is the render surface, so diagnostics go to
                // stderr — zellij funnels it into its own log file.
                Effect::Log(line) => eprintln!("[zellij-agent-activity] {line}"),
                Effect::ShowActivity { tab_id, prefix } => {
                    let mut args = BTreeMap::new();
                    args.insert("tab_id".to_string(), tab_id.to_string());
                    let name = match &prefix {
                        Some(value) => {
                            args.insert("value".to_string(), value.clone());
                            "set_prefix"
                        }
                        None => "clear_prefix",
                    };
                    // ADR-0001: route to the already-loaded namer by pipe name;
                    // no plugin_url/destination so nothing new is launched.
                    pipe_message_to_plugin(MessageToPlugin::new(name).with_args(args));
                }
            }
        }
    }
}

// ─── Core: pure state machine, events in → effects out, host-free ───────────

impl State {
    fn init(&mut self, config: &BTreeMap<String, String>) -> Vec<Effect> {
        self.debug = matches!(config.get("debug").map(String::as_str), Some("true" | "1"));
        trace!(self, "debug tracing on (config: {config:?})");
        self.effects.push(Effect::RequestPermissions(vec![
            PermissionType::ReadApplicationState,
            PermissionType::MessageAndLaunchOtherPlugins,
            // Covers `unblock_cli_pipe_input`, which the host denies without it
            // — silently, and (measured) harmlessly, since zellij releases the
            // pipe on its own anyway. The call is a defensive belt over that;
            // the grant just keeps it from being a denied no-op that floods the
            // zellij log (ADR-0003).
            PermissionType::ReadCliPipes,
            PermissionType::RunCommands,
        ]));
        self.effects.push(Effect::Subscribe(vec![
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::PermissionRequestResult,
        ]));
        std::mem::take(&mut self.effects)
    }

    fn handle(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::TabUpdate(tabs) => self.on_tab_update(tabs),
            Event::PaneUpdate(manifest) => self.on_pane_update(manifest),
            // `run_command` needs the RunCommands grant, which only lands here —
            // never in `init` (host calls before the grant are denied). So the
            // hook auto-install is emitted on grant, not at load.
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                let (command, context) = installer::install_command();
                self.effects.push(Effect::RunCommand(command, context));
            }
            _ => {}
        }
        std::mem::take(&mut self.effects)
    }

    fn handle_pipe(&mut self, message: PipeMessage) -> Vec<Effect> {
        trace!(self, "pipe '{}' args={:?}", message.name, message.args);
        if let PipeSource::Cli(pipe_id) = &message.source {
            // The hook's `zellij pipe` blocks until we unblock it.
            self.effects.push(Effect::UnblockCliPipe(pipe_id.clone()));
        }
        if message.name == PIPE_NAME {
            self.on_activity(&message.args);
        }
        std::mem::take(&mut self.effects)
    }

    fn on_activity(&mut self, args: &BTreeMap<String, String>) {
        let Some(pane_id) = args.get("pane_id").and_then(|s| s.parse::<u32>().ok()) else {
            trace!(self, "drop: no usable pane_id in {args:?}");
            return;
        };
        // Ordering: a stale event never overwrites a newer one for this pane.
        if let Some(ts) = args.get("ts_ms").and_then(|s| s.parse::<u64>().ok()) {
            if let Some(&last) = self.last_ts.get(&pane_id) {
                if ts < last {
                    trace!(self, "pane {pane_id}: drop, stale ts {ts} < {last}");
                    return;
                }
            }
            self.last_ts.insert(pane_id, ts);
        }
        let Some(&tab_id) = self.pane_to_tab.get(&pane_id) else {
            // pane not mapped yet — Claude events arrive long after load
            trace!(self, "pane {pane_id}: drop, not mapped to a tab yet");
            return;
        };
        let event = args.get("hook_event").map(|s| s.as_str()).unwrap_or("");
        if event == "SessionEnd" {
            trace!(self, "pane {pane_id} (tab {tab_id}): SessionEnd -> cleared");
            self.pane_activity.remove(&pane_id);
            self.recompute_tab(tab_id);
            return;
        }
        let tool = args.get("tool_name").map(|s| s.as_str()).unwrap_or("");
        match activity_from_event(event, tool) {
            Some(activity) => {
                trace!(
                    self,
                    "pane {pane_id} (tab {tab_id}): {event}/{tool} -> {activity:?}"
                );
                self.pane_activity.insert(pane_id, activity);
                self.recompute_tab(tab_id);
            }
            // Unmapped events (`SubagentStop`, anything Claude adds later) leave
            // the pane as it was — this is the branch that must NOT flip to ✓.
            None => trace!(
                self,
                "pane {pane_id} (tab {tab_id}): {event}/{tool} unmapped, state kept"
            ),
        }
    }

    fn on_tab_update(&mut self, tabs: Vec<TabInfo>) {
        self.tab_id_by_pos = tabs.iter().map(|t| (t.position, t.tab_id)).collect();
        let alive: HashSet<usize> = tabs.iter().map(|t| t.tab_id).collect();
        self.shown.retain(|id, _| alive.contains(id));
        self.rebuild_pane_to_tab();
        trace!(self, "tabs: position->tab_id {:?}", self.tab_id_by_pos);
        self.recompute_all();
    }

    fn on_pane_update(&mut self, manifest: PaneManifest) {
        self.panes = manifest
            .panes
            .into_iter()
            .map(|(pos, panes)| {
                let ids = panes
                    .into_iter()
                    .filter(|p| !p.is_plugin)
                    .map(|p| p.id)
                    .collect();
                (pos, ids)
            })
            .collect();
        let alive: HashSet<u32> = self.panes.values().flatten().copied().collect();
        self.pane_activity.retain(|id, _| alive.contains(id));
        self.last_ts.retain(|id, _| alive.contains(id));
        self.rebuild_pane_to_tab();
        trace!(self, "panes: pane_id->tab_id {:?}", self.pane_to_tab);
        self.recompute_all();
    }

    fn rebuild_pane_to_tab(&mut self) {
        let mut map = HashMap::new();
        for (pos, pane_ids) in &self.panes {
            if let Some(&tab_id) = self.tab_id_by_pos.get(pos) {
                for &pane_id in pane_ids {
                    map.insert(pane_id, tab_id);
                }
            }
        }
        self.pane_to_tab = map;
    }

    fn recompute_all(&mut self) {
        let tab_ids: Vec<usize> = self.tab_id_by_pos.values().copied().collect();
        for tab_id in tab_ids {
            self.recompute_tab(tab_id);
        }
    }

    /// Recompute a tab's winning activity (max priority among its panes) and
    /// emit a ShowActivity effect only if the resulting prefix changed.
    fn recompute_tab(&mut self, tab_id: usize) {
        let desired: Option<String> = self
            .pane_activity
            .iter()
            .filter(|(pane_id, _)| self.pane_to_tab.get(pane_id) == Some(&tab_id))
            .max_by_key(|(_, activity)| activity.priority())
            .map(|(_, activity)| format!("{} ", activity.symbol()));

        if self.shown.get(&tab_id) == desired.as_ref() {
            return;
        }
        match &desired {
            Some(prefix) => {
                self.shown.insert(tab_id, prefix.clone());
            }
            None => {
                self.shown.remove(&tab_id);
            }
        }
        trace!(self, "tab {tab_id}: prefix -> {desired:?}");
        self.effects.push(Effect::ShowActivity {
            tab_id,
            prefix: desired,
        });
    }
}

// Native `[[bin]]` builds need an entrypoint; the real one is `register_plugin!`
// on wasm (ADR-0004).
#[cfg(not(target_arch = "wasm32"))]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: usize, position: usize, active: bool) -> TabInfo {
        TabInfo {
            tab_id: id,
            position,
            active,
            ..Default::default()
        }
    }

    fn manifest(entries: &[(usize, &[u32])]) -> PaneManifest {
        let panes = entries
            .iter()
            .map(|(position, pane_ids)| {
                let panes = pane_ids
                    .iter()
                    .map(|&id| PaneInfo {
                        id,
                        ..Default::default()
                    })
                    .collect();
                (*position, panes)
            })
            .collect();
        PaneManifest { panes }
    }

    fn activity_pipe(args: &[(&str, &str)]) -> PipeMessage {
        PipeMessage {
            source: PipeSource::Plugin(0),
            name: PIPE_NAME.to_string(),
            payload: None,
            args: args
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            is_private: false,
        }
    }

    /// One active tab (id 1, position 0) holding pane 10.
    fn ready_state() -> State {
        let mut state = State::default();
        state.handle(Event::TabUpdate(vec![tab(1, 0, true)]));
        state.handle(Event::PaneUpdate(manifest(&[(0, &[10])])));
        state
    }

    fn show_effects(effects: &[Effect]) -> Vec<(usize, Option<String>)> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::ShowActivity { tab_id, prefix } => Some((*tab_id, prefix.clone())),
                _ => None,
            })
            .collect()
    }

    fn log_lines(effects: &[Effect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Log(line) => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn init_requests_permissions_and_subscribes() {
        let mut state = State::default();
        let effects = state.init(&BTreeMap::new());
        assert_eq!(
            effects,
            vec![
                Effect::RequestPermissions(vec![
                    PermissionType::ReadApplicationState,
                    PermissionType::MessageAndLaunchOtherPlugins,
                    // Missing this one is invisible at runtime — see `init`.
                    PermissionType::ReadCliPipes,
                    PermissionType::RunCommands,
                ]),
                Effect::Subscribe(vec![
                    EventType::TabUpdate,
                    EventType::PaneUpdate,
                    EventType::PermissionRequestResult,
                ]),
            ]
        );
    }

    #[test]
    fn unblocking_a_cli_pipe_is_covered_by_a_requested_permission() {
        // The plugin unblocks every CLI pipe it receives, and the host denies
        // that without `ReadCliPipes` — silently, since the event itself still
        // lands and the symbols keep working. Bind effect and grant together so
        // one can't drift from the other, which is how it went unnoticed.
        let mut state = State::default();
        let requested = match state.init(&BTreeMap::new()).first() {
            Some(Effect::RequestPermissions(perms)) => perms.clone(),
            other => panic!("init must request permissions first, got {other:?}"),
        };
        let effects = state.handle_pipe(PipeMessage {
            source: PipeSource::Cli("pipe-1".to_string()),
            name: PIPE_NAME.to_string(),
            payload: None,
            args: BTreeMap::new(),
            is_private: false,
        });
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::UnblockCliPipe(_))),
            "a CLI pipe must be unblocked, got {effects:?}"
        );
        assert!(
            requested.contains(&PermissionType::ReadCliPipes),
            "…so ReadCliPipes must be requested, got {requested:?}"
        );
    }

    #[test]
    fn granted_permission_installs_the_hook() {
        // The hook install must wait for the grant — `run_command` before it is
        // denied by the host, which is why it is not emitted from `init`.
        let mut state = State::default();
        let effects = state.handle(Event::PermissionRequestResult(PermissionStatus::Granted));
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::RunCommand(cmd, _) if cmd.first().map(String::as_str) == Some("sh")
        )));
    }

    #[test]
    fn activity_before_mapping_is_dropped() {
        let mut state = State::default();
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        assert_eq!(show_effects(&effects), vec![]);
    }

    #[test]
    fn pretooluse_bash_shows_bolt_prefix_on_the_pane_s_tab() {
        let mut state = ready_state();
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        assert_eq!(show_effects(&effects), vec![(1, Some("⚡ ".to_string()))]);
    }

    #[test]
    fn waiting_wins_over_thinking_in_the_same_tab() {
        // Two Claude panes (10, 11) in tab 1.
        let mut state = State::default();
        state.handle(Event::TabUpdate(vec![tab(1, 0, true)]));
        state.handle(Event::PaneUpdate(manifest(&[(0, &[10, 11])])));

        // pane 11 is thinking, pane 10 is waiting on the user (Notification).
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "11"),
            ("hook_event", "PostToolUse"),
        ]));
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "Notification"),
        ]));
        // The tab must show the Waiting symbol, never the Thinking one.
        assert_eq!(show_effects(&effects), vec![(1, Some("⚠ ".to_string()))]);
    }

    #[test]
    fn clearing_the_winner_falls_back_to_the_next_pane() {
        let mut state = State::default();
        state.handle(Event::TabUpdate(vec![tab(1, 0, true)]));
        state.handle(Event::PaneUpdate(manifest(&[(0, &[10, 11])])));
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "11"),
            ("hook_event", "PostToolUse"),
        ]));
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "Notification"),
        ]));

        // pane 10 finishes its session → Waiting gone, Thinking (pane 11) wins.
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "SessionEnd"),
        ]));
        assert_eq!(show_effects(&effects), vec![(1, Some("● ".to_string()))]);
    }

    #[test]
    fn session_end_on_last_pane_clears_the_prefix() {
        let mut state = ready_state();
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "SessionEnd"),
        ]));
        assert_eq!(show_effects(&effects), vec![(1, None)]);
    }

    #[test]
    fn stale_ts_is_dropped() {
        let mut state = ready_state();
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "Notification"),
            ("ts_ms", "1000"),
        ]));
        // An older event (ts 500) must not overwrite the Waiting state.
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
            ("ts_ms", "500"),
        ]));
        assert_eq!(show_effects(&effects), vec![]);
    }

    #[test]
    fn redundant_activity_emits_no_effect() {
        let mut state = ready_state();
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        assert_eq!(show_effects(&effects), vec![]);
    }

    #[test]
    fn pane_moved_between_tabs_clears_old_and_sets_new() {
        let mut state = State::default();
        state.handle(Event::TabUpdate(vec![tab(1, 0, true), tab(2, 1, false)]));
        state.handle(Event::PaneUpdate(manifest(&[(0, &[10]), (1, &[20])])));
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "Notification"),
        ]));

        // pane 10 moves from tab 1 (pos 0) to tab 2 (pos 1).
        let effects = state.handle(Event::PaneUpdate(manifest(&[(0, &[]), (1, &[20, 10])])));
        let shows = show_effects(&effects);
        assert!(
            shows.contains(&(1, None)),
            "old tab must clear, got {shows:?}"
        );
        assert!(
            shows.contains(&(2, Some("⚠ ".to_string()))),
            "new tab must show the symbol, got {shows:?}"
        );
    }

    #[test]
    fn tab_closed_while_active_is_gc_d_without_emitting_to_it() {
        let mut state = ready_state();
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        // tab 1 closes: another tab takes its slot, its pane disappears.
        let e1 = state.handle(Event::TabUpdate(vec![tab(2, 0, true)]));
        let e2 = state.handle(Event::PaneUpdate(manifest(&[(0, &[20])])));
        for (tab_id, _) in show_effects(&e1).into_iter().chain(show_effects(&e2)) {
            assert_ne!(tab_id, 1, "must never emit to the dead tab");
        }
        assert!(!state.shown.contains_key(&1));
        assert!(!state.pane_activity.contains_key(&10));
    }

    #[test]
    fn pane_update_before_tab_update_resolves_without_deferral() {
        let mut state = State::default();
        // PaneUpdate first: positions not yet mapped to tab_ids.
        state.handle(Event::PaneUpdate(manifest(&[(0, &[10])])));
        // Activity arriving in the gap is dropped (unmapped).
        let gap = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        assert_eq!(show_effects(&gap), vec![]);
        // TabUpdate lands → mapping resolves, next activity maps correctly.
        state.handle(Event::TabUpdate(vec![tab(1, 0, true)]));
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        assert_eq!(show_effects(&effects), vec![(1, Some("⚡ ".to_string()))]);
    }

    #[test]
    fn plugin_panes_are_ignored() {
        let mut state = State::default();
        state.handle(Event::TabUpdate(vec![tab(1, 0, true)]));
        let panes = PaneManifest {
            panes: [(
                0,
                vec![
                    PaneInfo {
                        id: 10,
                        is_plugin: false,
                        ..Default::default()
                    },
                    PaneInfo {
                        id: 99,
                        is_plugin: true,
                        ..Default::default()
                    },
                ],
            )]
            .into_iter()
            .collect(),
        };
        state.handle(Event::PaneUpdate(panes));
        // The plugin pane 99 is never mapped → its activity is dropped.
        let dropped = state.handle_pipe(activity_pipe(&[
            ("pane_id", "99"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        assert_eq!(show_effects(&dropped), vec![]);
        // The terminal pane 10 works.
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ]));
        assert_eq!(show_effects(&effects), vec![(1, Some("⚡ ".to_string()))]);
    }

    #[test]
    fn done_persists_as_check_prefix() {
        let mut state = ready_state();
        let effects =
            state.handle_pipe(activity_pipe(&[("pane_id", "10"), ("hook_event", "Stop")]));
        assert_eq!(show_effects(&effects), vec![(1, Some("✓ ".to_string()))]);
        assert_eq!(state.shown.get(&1), Some(&"✓ ".to_string()));
    }

    #[test]
    fn equal_ts_is_processed_and_absent_ts_never_drops() {
        let mut state = ready_state();
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "Notification"),
            ("ts_ms", "1000"),
        ]));
        // Equal ts (1000, not strictly older) is processed, not dropped.
        let e = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
            ("ts_ms", "1000"),
        ]));
        assert_eq!(show_effects(&e), vec![(1, Some("⚡ ".to_string()))]);
        // Absent ts is never dropped (and leaves the ordering guard untouched).
        let e = state.handle_pipe(activity_pipe(&[("pane_id", "10"), ("hook_event", "Stop")]));
        assert_eq!(show_effects(&e), vec![(1, Some("✓ ".to_string()))]);
    }

    #[test]
    fn session_end_on_unmapped_pane_is_a_noop() {
        let mut state = ready_state();
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "777"),
            ("hook_event", "SessionEnd"),
        ]));
        assert_eq!(show_effects(&effects), vec![]);
    }

    #[test]
    fn subagent_stop_never_flips_a_waiting_pane_to_done() {
        // The live incident this guards against: the main agent was blocked on a
        // permission prompt (`Notification` → ⚠) while two `Task` subagents were
        // still running *in the same pane*. Each one finishing fired
        // `SubagentStop`, which used to mean `Done` — so the tab showed ✓ while
        // Claude was in fact waiting on the user.
        let mut state = ready_state();
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
            ("ts_ms", "1000"),
        ]));
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "Notification"),
            ("ts_ms", "2000"),
        ]));
        let effects = state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "SubagentStop"),
            ("ts_ms", "3000"),
        ]));
        assert_eq!(show_effects(&effects), vec![]);
        assert_eq!(state.shown.get(&1), Some(&"⚠ ".to_string()));
    }

    #[test]
    fn stop_alone_marks_the_pane_done() {
        // The counterpart: only the *main* agent's `Stop` ends the turn.
        let mut state = ready_state();
        state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "Notification"),
        ]));
        let effects =
            state.handle_pipe(activity_pipe(&[("pane_id", "10"), ("hook_event", "Stop")]));
        assert_eq!(show_effects(&effects), vec![(1, Some("✓ ".to_string()))]);
    }

    #[test]
    fn debug_off_emits_no_log_effects() {
        let mut state = State::default();
        let mut effects = state.init(&BTreeMap::new());
        effects.extend(state.handle(Event::TabUpdate(vec![tab(1, 0, true)])));
        effects.extend(state.handle(Event::PaneUpdate(manifest(&[(0, &[10])]))));
        effects.extend(state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ])));
        assert_eq!(log_lines(&effects), Vec::<String>::new());
    }

    #[test]
    fn debug_on_traces_the_whole_decision_path() {
        let mut state = State::default();
        state.init(&BTreeMap::from([("debug".to_string(), "true".to_string())]));
        state.handle(Event::TabUpdate(vec![tab(1, 0, true)]));
        state.handle(Event::PaneUpdate(manifest(&[(0, &[10])])));

        // An event the plugin acts on: received, mapped, and emitted.
        let acted = log_lines(&state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "PreToolUse"),
            ("tool_name", "Bash"),
        ])));
        assert!(
            acted.iter().any(|l| l.contains("PreToolUse/Bash -> Tool"))
                && acted.iter().any(|l| l.contains("tab 1: prefix -> Some")),
            "must trace the mapping and the emission, got {acted:?}"
        );

        // An event it deliberately ignores must say so rather than go silent —
        // that silence is what made the ✓ bug invisible.
        let ignored = log_lines(&state.handle_pipe(activity_pipe(&[
            ("pane_id", "10"),
            ("hook_event", "SubagentStop"),
        ])));
        assert!(
            ignored.iter().any(|l| l.contains("unmapped, state kept")),
            "must trace the ignored event, got {ignored:?}"
        );

        // And a dropped one must name the reason.
        let dropped = log_lines(
            &state.handle_pipe(activity_pipe(&[("pane_id", "777"), ("hook_event", "Stop")])),
        );
        assert!(
            dropped
                .iter()
                .any(|l| l.contains("not mapped to a tab yet")),
            "must trace the drop reason, got {dropped:?}"
        );
    }

    #[test]
    fn tool_symbol_table() {
        assert_eq!(tool_symbol("Bash"), "⚡");
        assert_eq!(tool_symbol("Read"), "◉");
        assert_eq!(tool_symbol("Glob"), "◉");
        assert_eq!(tool_symbol("Grep"), "◉");
        assert_eq!(tool_symbol("Edit"), "✎");
        assert_eq!(tool_symbol("Write"), "✎");
        assert_eq!(tool_symbol("MultiEdit"), "✎");
        assert_eq!(tool_symbol("Task"), "⊜");
        assert_eq!(tool_symbol("WebSearch"), "◈");
        assert_eq!(tool_symbol("WebFetch"), "◈");
        assert_eq!(tool_symbol("SomethingUnknown"), "⚙");
    }
}
