//! Opt-in integration with the installed app-server. Never connects to the user's daemon.
use codex_zellij::{relay, Activity};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::{net::UnixStream, process::Command, sync::watch, time::timeout};
use tokio_tungstenite::{client_async, tungstenite::Message, WebSocketStream};

async fn rpc(client: &mut WebSocketStream<UnixStream>, value: Value) -> Value {
    let id = value["id"].clone();
    client
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
    timeout(Duration::from_secs(15), async {
        loop {
            let msg = client.next().await.expect("EOF").unwrap();
            if let Message::Text(text) = msg {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value.get("method").is_none() && value["id"] == id {
                    return value;
                }
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires installed Codex; uses a disposable CODEX_HOME and local server"]
async fn installed_codex_shared_server_via_two_proxies() {
    let dir = tempfile::tempdir_in("/tmp").unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("config.toml"), "model_provider=\"probe\"\nmodel=\"probe\"\n[model_providers.probe]\nname=\"probe\"\nbase_url=\"http://127.0.0.1:1/v1\"\nwire_api=\"responses\"\n").unwrap();
    let path = dir.path().join("daemon.sock");
    let mut server = Command::new("codex")
        .args([
            "app-server",
            "--listen",
            &format!("unix://{}", path.display()),
        ])
        .env("CODEX_HOME", &home)
        .env_remove("OPENAI_API_KEY")
        .env_remove("CODEX_API_KEY")
        .env_remove("CODEX_API_TOKEN")
        .env("ZELLIJ_SESSION_NAME", "inherited-first")
        .env("ZELLIJ_PANE_ID", "26")
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    timeout(Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let mut clients = Vec::new();
    let mut relays = Vec::new();
    for _ in 0..2 {
        let (a, b) = UnixStream::pair().unwrap();
        let (tx, _rx) = watch::channel(Activity::Clear);
        let path = path.clone();
        relays.push(tokio::spawn(async move {
            relay(b, path.to_str().unwrap(), tx).await
        }));
        let (mut client, _) = client_async("ws://localhost/", a).await.unwrap();
        let response=rpc(&mut client,json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"activity_lab","version":"0.0.0"},"capabilities":{"experimentalApi":true}}})).await;
        assert!(response.get("result").is_some(), "{response}");
        client
            .send(Message::Text(
                json!({"method":"initialized"}).to_string().into(),
            ))
            .await
            .unwrap();
        clients.push(client);
    }
    let mut ids = Vec::new();
    for client in &mut clients {
        let result = rpc(
            client,
            json!({"id":2,"method":"thread/start","params":{"cwd":dir.path()}}),
        )
        .await;
        assert!(result.get("result").is_some(), "{result}");
        ids.push(
            result["result"]["thread"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    assert_ne!(ids[0], ids[1]);
    // Materialize a durable rollout with an interrupted turn against an unavailable loopback provider.
    let turn = rpc(&mut clients[0], json!({"id":20,"method":"turn/start","params":{"threadId":ids[0],"input":[{"type":"text","text":"probe","text_elements":[]}]}})).await;
    assert!(turn.get("result").is_some(), "{turn}");
    timeout(Duration::from_secs(10), async {
        while let Some(msg) = clients[0].next().await {
            if let Message::Text(text) = msg.unwrap() {
                let v: Value = serde_json::from_str(&text).unwrap();
                if v["method"] == "error" {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    let interrupted = rpc(&mut clients[0],json!({"id":21,"method":"turn/interrupt","params":{"threadId":ids[0],"turnId":turn["result"]["turn"]["id"]}})).await;
    assert!(interrupted.get("result").is_some(), "{interrupted}");
    let result = rpc(
        &mut clients[1],
        json!({"id":3,"method":"thread/resume","params":{"threadId":ids[0]}}),
    )
    .await;
    assert!(result.get("result").is_some(), "{result}");
    assert_eq!(result["result"]["thread"]["id"], ids[0]);
    // Unknown RPC errors must also pass through unchanged and preserve the connection.
    let error = rpc(
        &mut clients[0],
        json!({"id":4,"method":"activity/unsupported","params":{}}),
    )
    .await;
    assert!(error.get("error").is_some());
    server.kill().await.unwrap();
    server.wait().await.unwrap();
    for relay in relays {
        let _ = timeout(Duration::from_secs(5), relay)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "requires installed Zellij; targets a unique nonexistent session"]
async fn installed_zellij_absent_session_is_bounded() {
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let session = format!(
        "absent-{}",
        directory.path().file_name().unwrap().to_string_lossy()
    );
    let (tx, rx) = watch::channel(Activity::Clear);
    let worker = tokio::spawn(codex_zellij::sink(
        rx,
        session,
        46,
        "zellij".into(),
        Duration::from_millis(500),
    ));
    tx.send_replace(Activity::Waiting);
    drop(tx);
    timeout(Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap();
}
