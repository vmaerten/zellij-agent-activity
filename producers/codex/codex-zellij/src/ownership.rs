//! Cooperative ownership only: no stored destinations, PIDs, expiry clocks, or config changes.
use anyhow::{bail, Context, Result};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::time::{sleep, Instant};

pub fn directory() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("ZELLIJ_AGENT_ACTIVITY_COORDINATION_DIR") {
        return Ok(root.into());
    }
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is required")?)
            .join(".local/state/zellij-agent-activity/codex-v1"),
    )
}

pub struct Ownership {
    _lease: File,
}
impl Ownership {
    pub async fn acquire(root: &Path, thread: &str) -> Result<Self> {
        if thread.is_empty() || thread.len() > 100 {
            bail!("invalid conversation identity");
        }
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
        let metadata = fs::symlink_metadata(root)?;
        // SAFETY: getuid has no preconditions or pointer arguments.
        let uid = unsafe { libc::getuid() };
        if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
            bail!("coordination directory must be a private directory owned by this user");
        }
        let key = thread
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let open = |suffix| -> Result<File> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(root.join(format!("{key}.{suffix}")))?;
            let info = file.metadata()?;
            if !info.is_file() || info.uid() != uid {
                bail!("invalid coordination lock file");
            }
            Ok(file)
        };
        let gate = open("gate")?;
        let lease = open("owners")?;
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            match gate.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    sleep(Duration::from_millis(10)).await
                }
                Err(error) => {
                    return Err(anyhow::anyhow!("cannot drain previous producer: {error}"))
                }
            }
        }
        lease.try_lock_shared()?;
        // Dropping the gate permits hooks to run, but their exclusive lease probe now fails.
        // The OS drops this shared lease after the final view exits, including SIGKILL.
        drop(gate);
        Ok(Self { _lease: lease })
    }
}
