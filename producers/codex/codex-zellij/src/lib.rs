//! Throwaway local-daemon experiment. No Codex payloads are logged or answered.
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{net::UnixStream, process::Command, sync::watch, time::timeout};
use tokio_tungstenite::{accept_async, client_async, tungstenite::Message};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    Clear,
    Init,
    Thinking,
    Tool(String),
    Waiting,
    Done,
}
impl Activity {
    pub fn args(&self, pane: u32, timestamp: u128) -> String {
        let (event, tool) = match self {
            Self::Clear => ("SessionEnd", ""),
            Self::Init => ("SessionStart", ""),
            Self::Thinking => ("PostToolUse", ""),
            Self::Tool(tool) => ("PreToolUse", tool.as_str()),
            Self::Waiting => ("Notification", ""),
            Self::Done => ("Stop", ""),
        };
        let mut args =
            format!("pane_id={pane},hook_event={event},tool_name={tool},ts_ms={timestamp}");
        if *self == Self::Waiting {
            args.push_str(",notification=permission");
        }
        args
    }
}

#[derive(Default, Debug)]
struct ThreadState {
    tools: BTreeMap<String, (u64, String)>,
    requests: HashSet<String>,
    waiting_flag: bool,
    active: bool,
    seen_turn: bool,
    sequence: u64,
}
impl ThreadState {
    fn activity(&self) -> Activity {
        if self.waiting_flag || !self.requests.is_empty() {
            return Activity::Waiting;
        }
        if let Some((_, tool)) = self.tools.values().max_by_key(|(order, _)| order) {
            return Activity::Tool(tool.clone());
        }
        if self.active {
            Activity::Thinking
        } else if self.seen_turn {
            Activity::Done
        } else {
            Activity::Init
        }
    }
    fn finish(&mut self) {
        self.tools.clear();
        self.requests.clear();
        self.waiting_flag = false;
        self.active = false;
        self.seen_turn = true;
    }
    fn status(&mut self, status: &Value) {
        match status["type"].as_str() {
            Some("active") => {
                self.active = true;
                self.waiting_flag = status["activeFlags"].as_array().is_some_and(|flags| {
                    flags.iter().any(|f| {
                        matches!(f.as_str(), Some("waitingOnApproval" | "waitingOnUserInput"))
                    })
                });
            }
            Some("idle" | "systemError" | "notLoaded") => {
                let seen_turn = self.seen_turn || self.active;
                self.finish();
                self.seen_turn = seen_turn;
            }
            _ => {}
        }
    }
    fn start_item(&mut self, item: &Value) {
        let Some(id) = item["id"].as_str() else {
            return;
        };
        let tool = match item["type"].as_str() {
            Some("commandExecution") => "Bash",
            Some("fileChange") => "Edit",
            Some("webSearch") => "WebSearch",
            Some("imageView") => "Read",
            Some("collabAgentToolCall") => "Agent",
            Some("mcpToolCall" | "dynamicToolCall") => "Other",
            _ => return,
        };
        self.sequence += 1;
        self.tools
            .insert(id.to_owned(), (self.sequence, tool.to_owned()));
    }
}

enum Pending {
    Select,
    Unsubscribe(String),
}

/// One observer is one view; a successful start/resume/fork response selects its thread.
/// Request IDs are scoped by direction, so a server request cannot consume a client RPC.
#[derive(Default)]
pub struct Observer {
    selected: Option<String>,
    pending: HashMap<String, Pending>,
    threads: HashMap<String, ThreadState>,
}
impl Observer {
    pub fn activity(&self) -> Activity {
        self.selected
            .as_ref()
            .and_then(|id| self.threads.get(id))
            .map_or(Activity::Clear, ThreadState::activity)
    }
    pub fn client(&mut self, value: &Value) {
        if let (
            Some(
                method @ ("thread/start" | "thread/resume" | "thread/fork" | "thread/unsubscribe"),
            ),
            Some(id),
        ) = (value["method"].as_str(), value.get("id"))
        {
            self.pending.insert(
                id.to_string(),
                if method == "thread/unsubscribe" {
                    Pending::Unsubscribe(
                        value["params"]["threadId"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    )
                } else {
                    Pending::Select
                },
            );
        }
        // Responses to approvals are merely relayed. The server's resolved event is authoritative.
    }
    pub fn server(&mut self, value: &Value) {
        if value.get("method").is_none() {
            if let Some(pending) = value
                .get("id")
                .and_then(|id| self.pending.remove(&id.to_string()))
            {
                if value.get("error").is_some() {
                    return;
                }
                if let Pending::Unsubscribe(thread) = pending {
                    if self.selected.as_deref() == Some(&thread) {
                        self.selected = None;
                    }
                    self.threads.remove(&thread);
                } else if let Some(id) = value["result"]["thread"]["id"].as_str() {
                    let thread = &value["result"]["thread"];
                    let state = self.threads.entry(id.to_owned()).or_default();
                    // An idle newly created thread is initialized, not a completed turn.
                    state.seen_turn |= thread["turns"]
                        .as_array()
                        .is_some_and(|turns| !turns.is_empty());
                    state.status(&thread["status"]);
                    if let Some(turns) = thread["turns"].as_array() {
                        for turn in turns.iter().filter(|t| t["status"] == "inProgress") {
                            state.active = true;
                            if let Some(items) = turn["items"].as_array() {
                                for item in items.iter().filter(|i| i["status"] == "inProgress") {
                                    state.start_item(item);
                                }
                            }
                        }
                    }
                    self.selected = Some(id.to_owned());
                    self.threads.retain(|key, _| key == id);
                }
            }
            return;
        }
        let p = &value["params"];
        let Some(thread) = p["threadId"].as_str() else {
            return;
        };
        // Notifications for another thread, including subagents, never move the view.
        if self.selected.as_deref() != Some(thread) {
            return;
        }
        let state = self.threads.entry(thread.to_owned()).or_default();
        match value["method"].as_str().unwrap_or_default() {
            "turn/started" => {
                state.finish();
                state.active = true;
            }
            "turn/completed" => state.finish(),
            "thread/status/changed" => state.status(&p["status"]),
            "item/started" => {
                state.active = true;
                state.start_item(&p["item"]);
            }
            "item/completed" => {
                if let Some(id) = p["item"]["id"].as_str() {
                    state.tools.remove(id);
                }
            }
            "serverRequest/resolved" => {
                state.requests.remove(&p["requestId"].to_string());
            }
            "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
            | "mcpServer/elicitation/request" => {
                if let Some(id) = value.get("id") {
                    state.requests.insert(id.to_string());
                }
            }
            _ => {}
        }
    }
}

/// Coalesced snapshots bound memory to one pending state; a stuck consumer cannot stall Codex.
pub async fn sink(
    mut rx: watch::Receiver<Activity>,
    session: String,
    pane: u32,
    executable: String,
    deadline: Duration,
) {
    let mut last_timestamp = 0;
    while rx.changed().await.is_ok() {
        let activity = rx.borrow_and_update().clone();
        last_timestamp = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis())
        .max(last_timestamp + 1);
        let mut cmd = Command::new(&executable);
        cmd.args([
            "--session",
            &session,
            "pipe",
            "--name",
            "agent_activity.v1",
            "--args",
            &activity.args(pane, last_timestamp),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
        if let Ok(mut child) = cmd.spawn() {
            if timeout(deadline, child.wait()).await.is_err() {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }
    }
}

pub async fn relay(
    client: UnixStream,
    upstream_path: &str,
    states: watch::Sender<Activity>,
) -> Result<()> {
    let result = relay_inner(client, upstream_path, &states).await;
    states.send_replace(Activity::Clear);
    result
}
async fn relay_inner(
    client: UnixStream,
    upstream_path: &str,
    states: &watch::Sender<Activity>,
) -> Result<()> {
    let mut downstream = timeout(Duration::from_secs(5), accept_async(client)).await??;
    let upstream = timeout(Duration::from_secs(5), UnixStream::connect(upstream_path)).await??;
    let (mut upstream, _) = timeout(
        Duration::from_secs(5),
        client_async("ws://localhost/", upstream),
    )
    .await??;
    let mut observer = Observer::default();
    loop {
        let (from_client, msg) = tokio::select! {
            message = downstream.next() => (true, message),
            message = upstream.next() => (false, message),
        };
        let Some(msg) = msg else { return Ok(()) };
        let msg = msg.context("WebSocket disconnected")?;
        let close = msg.is_close();
        if let Message::Text(text) = &msg {
            if let Ok(value) = serde_json::from_str(text) {
                if from_client {
                    observer.client(&value);
                } else {
                    observer.server(&value);
                }
                let next = observer.activity();
                states.send_if_modified(|old| {
                    if *old == next {
                        false
                    } else {
                        *old = next;
                        true
                    }
                });
            }
        }
        if from_client {
            upstream.send(msg).await?;
        } else {
            downstream.send(msg).await?;
        }
        if close {
            return Ok(());
        }
    }
}
