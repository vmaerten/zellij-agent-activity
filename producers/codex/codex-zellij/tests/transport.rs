use codex_zellij::{relay, sink, Activity, View};
use futures_util::{SinkExt, StreamExt};
use std::{os::unix::fs::PermissionsExt, time::Duration};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::watch,
    time::timeout,
};
use tokio_tungstenite::{accept_async, client_async, tungstenite::Message};

#[tokio::test]
async fn bidirectional_permissions_unknown_messages_binary_and_daemon_restart() {
    let dir = tempfile::tempdir_in("/tmp").unwrap();
    let path = dir.path().join("daemon.sock");
    let daemon = UnixListener::bind(&path).unwrap();
    let (left, right) = UnixStream::pair().unwrap();
    let (tx, rx) = watch::channel(View::default());
    let upstream_path = path.clone();
    let relay_task =
        tokio::spawn(async move { relay(right, upstream_path.to_str().unwrap(), tx).await });
    let (mut client, _) = client_async("ws://localhost/", left).await.unwrap();
    let (stream, _) = daemon.accept().await.unwrap();
    let mut server = accept_async(stream).await.unwrap();
    let request=Message::Text(r#"{"id":5,"method":"item/commandExecution/requestApproval","params":{"threadId":"t","command":"private text"}}"#.into());
    server.send(request.clone()).await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(1), client.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        request
    );
    // No fabricated approval was sent upstream.
    assert!(timeout(Duration::from_millis(30), server.next())
        .await
        .is_err());
    let answer = Message::Text(r#"{"id":5,"result":{"decision":"decline"}}"#.into());
    client.send(answer.clone()).await.unwrap();
    assert_eq!(server.next().await.unwrap().unwrap(), answer);
    let binary = Message::Binary(vec![0, 255, 12].into());
    client.send(binary.clone()).await.unwrap();
    assert_eq!(server.next().await.unwrap().unwrap(), binary);
    let unknown = Message::Text(r#"{"method":"future/method","params":{"unknown":true}}"#.into());
    server.send(unknown.clone()).await.unwrap();
    assert_eq!(client.next().await.unwrap().unwrap(), unknown);
    // Abrupt daemon loss terminates this connection, clears the indicator, never replays RPCs.
    drop(server);
    let _ = timeout(Duration::from_secs(1), relay_task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*rx.borrow(), View::default());
    // A fresh instance can connect after the daemon is restarted (no implicit RPC retry).
    drop(daemon);
    std::fs::remove_file(&path).unwrap();
    let restarted = UnixListener::bind(&path).unwrap();
    let (left, right) = UnixStream::pair().unwrap();
    let (tx, _) = watch::channel(View::default());
    let second = tokio::spawn(async move { relay(right, path.to_str().unwrap(), tx).await });
    let (mut client, _) = client_async("ws://localhost/", left).await.unwrap();
    let (stream, _) = restarted.accept().await.unwrap();
    let mut server = accept_async(stream).await.unwrap();
    let message = Message::Text(r#"{"id":1,"method":"initialize","params":{}}"#.into());
    client.send(message.clone()).await.unwrap();
    assert_eq!(server.next().await.unwrap().unwrap(), message);
    drop(server);
    let _ = timeout(Duration::from_secs(1), second)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn blocked_sink_is_bounded_and_coalesces_latest_cleanup() {
    let dir = tempfile::tempdir_in("/tmp").unwrap();
    let script = dir.path().join("zellij");
    let log = dir.path().join("args");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexec sleep 30\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (tx, rx) = watch::channel(Activity::Clear);
    let worker = tokio::spawn(sink(
        rx,
        "another-session".into(),
        46,
        script.to_str().unwrap().into(),
        Duration::from_millis(500),
    ));
    tx.send_replace(Activity::Waiting);
    for _ in 0..500 {
        if log.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(log.exists());
    for _ in 0..10000 {
        tx.send_replace(Activity::Thinking);
    }
    tx.send_replace(Activity::Clear);
    drop(tx);
    timeout(Duration::from_secs(3), worker)
        .await
        .unwrap()
        .unwrap();
    let output = std::fs::read_to_string(log).unwrap();
    assert!(output.contains("--session another-session pipe --name agent_activity.v1"));
    assert!(output.contains("pane_id=46"));
    assert!(output.contains("hook_event=SessionEnd"));
    assert!(output.lines().count() <= 3);
}

#[tokio::test]
async fn missing_consumer_does_not_panic_or_delay_shutdown() {
    let (tx, rx) = watch::channel(Activity::Clear);
    let worker = tokio::spawn(sink(
        rx,
        "missing".into(),
        1,
        "/nonexistent/zellij".into(),
        Duration::from_millis(20),
    ));
    tx.send_replace(Activity::Tool("Bash".into()));
    drop(tx);
    timeout(Duration::from_secs(3), worker)
        .await
        .unwrap()
        .unwrap();
}
