use codex_zellij::{Activity, Observer};
use serde_json::{json, Value};

fn select(o: &mut Observer, id: &str, method: &str) {
    o.client(&json!({"id":1,"method":method,"params":{"threadId":id}}));
    o.server(&json!({"id":1,"result":{"thread":{"id":id}}}));
}
fn event(o: &mut Observer, method: &str, params: Value) {
    o.server(&json!({"method":method,"params":params}));
}
fn item(o: &mut Observer, method: &str, thread: &str, id: &str, kind: &str) {
    event(
        o,
        method,
        json!({"threadId":thread,"item":{"id":id,"type":kind}}),
    );
}
#[test]
fn same_repo_separate_threads_and_two_views_of_one_thread() {
    let mut a = Observer::default();
    let mut b = Observer::default();
    let mut c = Observer::default();
    select(&mut a, "one", "thread/start");
    select(&mut b, "two", "thread/start");
    select(&mut c, "one", "thread/resume");
    for o in [&mut a, &mut b, &mut c] {
        item(o, "item/started", "one", "cmd", "commandExecution");
    }
    assert_eq!(a.activity(), Activity::Tool("Bash".into()));
    assert_eq!(c.activity(), a.activity());
    assert_eq!(b.activity(), Activity::Init);
    assert!(a.activity().args(26, 1).starts_with("pane_id=26,"));
    assert!(b.activity().args(46, 1).starts_with("pane_id=46,"));
}
#[test]
fn parallel_tools_keep_waiting_priority_and_surviving_tools() {
    let mut o = Observer::default();
    select(&mut o, "t", "thread/start");
    event(&mut o, "turn/started", json!({"threadId":"t"}));
    assert_eq!(o.activity(), Activity::Thinking);
    item(&mut o, "item/started", "t", "cmd", "commandExecution");
    item(&mut o, "item/started", "t", "edit", "fileChange");
    o.server(&json!({"id":"p1","method":"item/commandExecution/requestApproval","params":{"threadId":"t"}}));
    o.server(&json!({"id":"p2","method":"item/tool/requestUserInput","params":{"threadId":"t"}}));
    item(&mut o, "item/completed", "t", "edit", "fileChange");
    assert_eq!(o.activity(), Activity::Waiting);
    o.client(&json!({"id":"p1","result":{"decision":"accept"}}));
    assert_eq!(o.activity(), Activity::Waiting);
    event(
        &mut o,
        "serverRequest/resolved",
        json!({"threadId":"t","requestId":"p1"}),
    );
    assert_eq!(o.activity(), Activity::Waiting);
    event(
        &mut o,
        "serverRequest/resolved",
        json!({"threadId":"t","requestId":"p2"}),
    );
    assert_eq!(o.activity(), Activity::Tool("Bash".into()));
    item(&mut o, "item/completed", "t", "cmd", "commandExecution");
    assert_eq!(o.activity(), Activity::Thinking);
    event(
        &mut o,
        "turn/completed",
        json!({"threadId":"t","turn":{"status":"interrupted"}}),
    );
    assert_eq!(o.activity(), Activity::Done);
}
#[test]
fn switching_fork_errors_unsubscribe_and_old_events() {
    let mut o = Observer::default();
    select(&mut o, "one", "thread/start");
    item(&mut o, "item/started", "one", "cmd", "commandExecution");
    o.client(&json!({"id":8,"method":"thread/resume","params":{"threadId":"bad"}}));
    o.server(&json!({"id":8,"error":{"code":-1}}));
    assert_eq!(o.activity(), Activity::Tool("Bash".into()));
    select(&mut o, "fork", "thread/fork");
    item(&mut o, "item/started", "one", "edit", "fileChange");
    assert_eq!(o.activity(), Activity::Init);
    o.client(&json!({"id":9,"method":"thread/unsubscribe","params":{"threadId":"fork"}}));
    o.server(&json!({"id":9,"result":{}}));
    assert_eq!(o.activity(), Activity::Clear);
}
#[test]
fn resume_snapshot_restores_active_items_and_other_clients_waits() {
    let mut o = Observer::default();
    o.client(&json!({"id":1,"method":"thread/resume","params":{"threadId":"t"}}));
    o.server(&json!({"id":1,"result":{"thread":{"id":"t","status":{"type":"active","activeFlags":["waitingOnApproval"]},"turns":[{"status":"inProgress","items":[{"id":"a","type":"commandExecution","status":"inProgress"},{"id":"b","type":"fileChange","status":"inProgress"}]}]}}}));
    assert_eq!(o.activity(), Activity::Waiting);
    event(
        &mut o,
        "thread/status/changed",
        json!({"threadId":"t","status":{"type":"active","activeFlags":[]}}),
    );
    assert_eq!(o.activity(), Activity::Tool("Edit".into()));
    item(&mut o, "item/completed", "t", "b", "fileChange");
    assert_eq!(o.activity(), Activity::Tool("Bash".into()));
}
#[test]
fn replay_trace() {
    let mut o = Observer::default();
    for line in include_str!("trace.jsonl").lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        if record["direction"] == "client" {
            o.client(&record["message"]);
        } else {
            o.server(&record["message"]);
        }
        if let Some(expected) = record["expected"].as_str() {
            assert_eq!(format!("{:?}", o.activity()), expected);
        }
    }
}

#[test]
fn idle_new_thread_keeps_initial_indicator_until_a_turn_finishes() {
    let mut o = Observer::default();
    o.client(&json!({"id":1,"method":"thread/start","params":{}}));
    o.server(&json!({"id":1,"result":{"thread":{"id":"t","status":{"type":"idle"},"turns":[]}}}));
    assert_eq!(o.activity(), Activity::Init);
    event(
        &mut o,
        "thread/status/changed",
        json!({"threadId":"t","status":{"type":"idle"}}),
    );
    assert_eq!(o.activity(), Activity::Init);
    event(&mut o, "turn/started", json!({"threadId":"t"}));
    event(&mut o, "turn/completed", json!({"threadId":"t"}));
    assert_eq!(o.activity(), Activity::Done);
}
