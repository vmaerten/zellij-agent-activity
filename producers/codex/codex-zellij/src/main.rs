use anyhow::{bail, Context, Result};
use codex_zellij::{relay, sink, Activity};
use std::{os::unix::fs::PermissionsExt, path::Path, time::Duration};
use tokio::{net::UnixListener, process::Command, sync::watch};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--help") {
        println!("PROTOTYPE: codex-zellij --lab --daemon unix:///absolute/socket -- [Codex arguments]\nOnly for isolated tests without activity hooks. Loaded-thread hook suppression is blocked.\nBuild with cargo build --manifest-path producers/codex/codex-zellij/Cargo.toml");
        return Ok(());
    }
    if args.len() < 4 || args[0] != "--lab" || args[1] != "--daemon" || args[3] != "--" {
        bail!("prototype blocked for normal use: hook suppression is not reliable; see README. Isolated tests require --lab --daemon unix:///path --");
    }
    let path = args[2]
        .strip_prefix("unix://")
        .filter(|s| Path::new(s).is_absolute())
        .context("daemon must be an explicit absolute local Unix socket")?;
    let session = std::env::var("ZELLIJ_SESSION_NAME").context("run inside a Zellij session")?;
    if session.is_empty() {
        bail!("ZELLIJ_SESSION_NAME is empty");
    }
    let pane: u32 = std::env::var("ZELLIJ_PANE_ID")
        .context("run inside a Zellij pane")?
        .parse()?;
    // /tmp keeps the path under macOS's small sockaddr_un limit. tempfile creates mode 0700.
    let directory = tempfile::Builder::new()
        .prefix("codex-zellij-")
        .tempdir_in("/tmp")?;
    let socket = directory.path().join("view.sock");
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let (tx, rx) = watch::channel(Activity::Clear);
    let mut sink = tokio::spawn(sink(
        rx,
        session,
        pane,
        "zellij".into(),
        Duration::from_secs(2),
    ));
    let mut child = Command::new("codex")
        .args(["--remote", &format!("unix://{}", socket.display())])
        .args(&args[4..])
        .kill_on_drop(true)
        .spawn()?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let transport = async {
        let (stream, _) = listener.accept().await?;
        relay(stream, path, tx.clone()).await
    };
    let result = tokio::select! {
        status = child.wait() => status.context("Codex child failed").and_then(|s| if s.success() {Ok(())} else {Err(anyhow::anyhow!("Codex exited with {s}"))}),
        result = transport => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = terminate.recv() => Ok(()),
    };
    let _ = child.kill().await;
    let _ = child.wait().await;
    tx.send_replace(Activity::Clear);
    drop(tx);
    if tokio::time::timeout(Duration::from_secs(5), &mut sink)
        .await
        .is_err()
    {
        sink.abort();
        let _ = sink.await;
    }
    drop(listener);
    drop(directory);
    result
}
