use std::{os::unix::fs::PermissionsExt, process::Command};

#[test]
fn launcher_captures_view_and_removes_private_socket_after_child_exit() {
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let root = directory.path();
    let codex = root.join("codex");
    let zellij = root.join("zellij");
    let args = root.join("client-args");
    let destination = root.join("destination");
    std::fs::write(
        &codex,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            args.display()
        ),
    )
    .unwrap();
    std::fs::write(
        &zellij,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            destination.display()
        ),
    )
    .unwrap();
    for file in [&codex, &zellij] {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_codex-zellij"))
        .args([
            "--lab",
            "--daemon",
            "unix:///nonexistent/daemon.sock",
            "--",
            "--no-alt-screen",
        ])
        .env(
            "PATH",
            format!("{}:{}", root.display(), std::env::var("PATH").unwrap()),
        )
        .env("ZELLIJ_SESSION_NAME", "view-session")
        .env("ZELLIJ_PANE_ID", "46")
        .env(
            "ZELLIJ_AGENT_ACTIVITY_COORDINATION_DIR",
            root.join("coordination"),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let args = std::fs::read_to_string(args).unwrap();
    let mut lines = args.lines();
    assert_eq!(lines.next(), Some("--remote"));
    let socket = lines.next().unwrap().strip_prefix("unix://").unwrap();
    assert!(!std::path::Path::new(socket).exists());
    assert!(!std::path::Path::new(socket).parent().unwrap().exists());
    assert_eq!(lines.next(), Some("--no-alt-screen"));
    let destination = std::fs::read_to_string(destination).unwrap();
    assert!(destination.contains("--session\nview-session\npipe\n"));
    assert!(destination.contains("pane_id=46,hook_event=SessionEnd"));
}

#[test]
fn conflicting_remote_flags_are_rejected() {
    let result = Command::new(env!("CARGO_BIN_EXE_codex-zellij"))
        .args(["--", "--remote", "unix:///elsewhere"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("codex-zellij owns the remote connection")
    );
}
