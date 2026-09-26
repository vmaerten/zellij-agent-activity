use codex_zellij::{
    coordinated_sink, ownership::Ownership, preflight::validate_hooks, Activity, View,
};
use serde_json::json;
use std::{fs::OpenOptions, os::unix::fs::PermissionsExt, time::Duration};
use tokio::{sync::watch, time::timeout};

#[test]
fn preflight_accepts_coordinated_producer_and_rejects_legacy_and_custom_hooks() {
    let producer = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let mut hook = json!({"enabled":true,"pluginId":"zellij-agent-activity@zellij-agent-activity","command":"${PLUGIN_ROOT}/scripts/forwarder.sh","sourcePath":producer.join("hooks/hooks.json")});
    assert!(validate_hooks(&json!({"data":[{"hooks":[hook.clone()],"errors":[]}]})).is_ok());
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("hooks")).unwrap();
    std::fs::create_dir(temp.path().join("scripts")).unwrap();
    std::fs::write(
        temp.path().join("scripts/forwarder.sh"),
        "#!/bin/sh\nzellij pipe",
    )
    .unwrap();
    hook["sourcePath"] = json!(temp.path().join("hooks/hooks.json"));
    assert!(validate_hooks(&json!({"data":[{"hooks":[hook]}]})).is_err());
    assert!(validate_hooks(&json!({"data":[{"hooks":[{"enabled":true,"command":"zellij pipe --name agent_activity.v1"}]}]})).is_err());
    assert!(validate_hooks(&json!({"data":[{"hooks":[],"errors":[]}]})).is_ok());
}

#[tokio::test]
async fn ownership_rejects_public_directory_and_symlink_lock() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Ownership::acquire(temp.path(), "t").await.is_err());
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink("/dev/null", temp.path().join("74.gate")).unwrap();
    assert!(Ownership::acquire(temp.path(), "t").await.is_err());
}

#[tokio::test]
async fn sink_switch_while_draining_emits_latest_view_and_releases_ownership() {
    let temp = tempfile::tempdir_in("/tmp").unwrap();
    let root = temp.path().join("coordination");
    drop(Ownership::acquire(&root, "a").await.unwrap());
    let gate = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("61.gate"))
        .unwrap();
    gate.lock().unwrap();
    let script = temp.path().join("sink");
    let log = temp.path().join("sent");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (tx, rx) = watch::channel(View::default());
    let worker = tokio::spawn(coordinated_sink(
        rx,
        "view".into(),
        46,
        script.to_str().unwrap().into(),
        Duration::from_secs(2),
        root.clone(),
    ));
    tx.send_replace(View {
        thread: Some("a".into()),
        activity: Activity::Tool("Bash".into()),
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    tx.send_replace(View {
        thread: Some("b".into()),
        activity: Activity::Done,
    });
    gate.unlock().unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            if std::fs::read_to_string(&log)
                .unwrap_or_default()
                .contains("hook_event=Stop")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tx.send_replace(View::default());
    drop(tx);
    timeout(Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap();
    let log = std::fs::read_to_string(log).unwrap();
    assert!(!log.contains("tool_name=Bash"));
    assert!(log.contains("hook_event=SessionEnd"));
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("62.owners"))
        .unwrap();
    assert!(lease.try_lock().is_ok());
}
