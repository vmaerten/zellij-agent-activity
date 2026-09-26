use anyhow::{bail, Context, Result};
use codex_zellij::{coordinated_sink, ownership, preflight, relay, View};
use std::{os::unix::fs::PermissionsExt, path::Path, time::Duration};
use tokio::{net::UnixListener, process::Command, sync::watch};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--help") {
        println!("codex-zellij [--daemon unix:///absolute/socket] -- [Codex arguments]\nRequires the coordinated activity producer, or no activity hooks.\n--lab skips the producer compatibility check for isolated tests only.");
        return Ok(());
    }
    let lab = args.first().is_some_and(|arg| arg == "--lab");
    let args = if lab { &args[1..] } else { &args[..] };
    let (daemon, forwarded) = if args.first().is_some_and(|arg| arg == "--daemon") {
        if args.len() < 3 || args[2] != "--" {
            bail!("usage: codex-zellij [--daemon unix:///path] -- [Codex arguments]");
        }
        (args[1].clone(), &args[3..])
    } else {
        if args.first().is_some_and(|arg| arg != "--") {
            bail!("separate Codex arguments with --");
        }
        let home = std::env::var_os("CODEX_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or(
                std::path::PathBuf::from(std::env::var_os("HOME").context("HOME is required")?)
                    .join(".codex"),
            );
        (
            format!(
                "unix://{}",
                home.join("app-server-control/app-server-control.sock")
                    .display()
            ),
            args.get(1..).unwrap_or_default(),
        )
    };
    let path = daemon
        .strip_prefix("unix://")
        .filter(|p| Path::new(p).is_absolute())
        .context("daemon must be an explicit absolute local Unix socket")?;
    if forwarded
        .iter()
        .any(|arg| arg == "--remote" || arg.starts_with("--remote=") || arg == "--no-daemon")
    {
        bail!("codex-zellij owns the remote connection; do not pass --remote or --no-daemon");
    }
    if !lab {
        preflight::check(path, &std::env::current_dir()?).await?;
    }
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
    let (tx, rx) = watch::channel(View::default());
    let mut sink = tokio::spawn(coordinated_sink(
        rx,
        session,
        pane,
        "zellij".into(),
        Duration::from_secs(2),
        ownership::directory()?,
    ));
    let mut child = Command::new("codex")
        .args(["--remote", &format!("unix://{}", socket.display())])
        .args(forwarded)
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
    tx.send_replace(View::default());
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
