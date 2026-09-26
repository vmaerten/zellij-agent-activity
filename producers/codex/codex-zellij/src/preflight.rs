//! Refuse the known competing legacy producer before launching a normal view.
use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use tokio::{net::UnixStream, time::timeout};
use tokio_tungstenite::{client_async, tungstenite::Message};

pub fn validate_hooks(response: &Value) -> Result<()> {
    let entries = response["data"]
        .as_array()
        .context("unsupported hooks/list response")?;
    for entry in entries {
        if entry["errors"]
            .as_array()
            .is_some_and(|errors| !errors.is_empty())
        {
            bail!("Codex could not load its hooks; inspect /hooks first");
        }
        for hook in entry["hooks"]
            .as_array()
            .context("missing hook inventory")?
        {
            if hook["enabled"] == false {
                continue;
            }
            let plugin = hook["pluginId"].as_str().unwrap_or_default();
            let command = hook["command"].as_str().unwrap_or_default();
            let ours = plugin.split('@').next() == Some("zellij-agent-activity");
            if !ours {
                if command.contains("agent_activity.v1")
                    || command.contains("zellij-agent-activity")
                {
                    bail!("custom activity hook detected; replace it with the coordinated Codex producer");
                }
                continue;
            }
            let source = Path::new(
                hook["sourcePath"]
                    .as_str()
                    .context("missing producer source path")?,
            );
            let root = source
                .parent()
                .and_then(Path::parent)
                .context("invalid producer location")?;
            let expected = root
                .join("scripts/forwarder.sh")
                .to_string_lossy()
                .into_owned();
            let canonical_command = command.trim_matches('"');
            if canonical_command != expected
                && canonical_command != "${PLUGIN_ROOT}/scripts/forwarder.sh"
            {
                bail!("activity hook uses a custom command; install the standard coordinated producer");
            }
            let forwarder = std::fs::read_to_string(root.join("scripts/forwarder.sh"))?;
            let guard =
                std::fs::read_to_string(root.join("scripts/coordination.py")).unwrap_or_default();
            if !forwarder.contains("# COORDINATION_PROTOCOL=1:") || !guard.contains("Protocol 1:") {
                bail!("installed activity producer is too old; install the coordinated producer before using codex-zellij");
            }
        }
    }
    Ok(())
}

pub async fn check(path: &str, cwd: &Path) -> Result<()> {
    timeout(Duration::from_secs(10),async {
        let stream=UnixStream::connect(path).await.context("cannot connect to the local Codex daemon")?;
        let (mut socket,_)=client_async("ws://localhost/",stream).await?;
        socket.send(Message::Text(json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"codex_zellij_check","version":"0.1.0"},"capabilities":{"experimentalApi":true}}}).to_string().into())).await?;
        for id in [1,2] {
            let response=loop {
                let frame=socket.next().await.context("daemon disconnected during hook check")??;
                if let Message::Text(text)=frame {
                    let value:Value=serde_json::from_str(&text)?;
                    if value.get("method").is_none() && value["id"]==id {break value;}
                }
            };
            if response.get("error").is_some() {bail!("daemon rejected the hook inventory check");}
            if id==1 {
                socket.send(Message::Text(json!({"method":"initialized"}).to_string().into())).await?;
                socket.send(Message::Text(json!({"id":2,"method":"hooks/list","params":{"cwds":[cwd]}}).to_string().into())).await?;
            } else {validate_hooks(&response["result"])?;}
        }
        let _=socket.close(None).await;
        Ok(())
    }).await.context("daemon hook check timed out")?
}
