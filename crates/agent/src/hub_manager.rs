use anyhow::{Context, Result};
use std::collections::HashMap;
use tokio::process::Child;
use tokio::sync::RwLock;

struct HubProcess {
    port: u16,
    _child: Child,
}

pub struct HubManager {
    hubs: RwLock<HashMap<String, HubProcess>>,
    hub_bin: String,
    #[allow(dead_code)]
    base_port: u16,
}

impl HubManager {
    pub fn new(hub_bin: String, base_port: u16) -> Self {
        Self {
            hubs: RwLock::new(HashMap::new()),
            hub_bin,
            base_port,
        }
    }

    pub async fn spawn_hub(
        &self,
        hub_id: &str,
        db_path: &str,
        port: u16,
        voice_port: u16,
        owner_pubkey: Option<&str>,
        farm_url: Option<&str>,
    ) -> Result<()> {
        // See the same block in farm/src/hub_manager.rs: these names used to
        // be literals, and WAVVON_HUB_HTTP_PORT was one the hub never reads,
        // so the assigned port was silently ignored. `db_path` is likewise
        // still a SQLite-era file path with no PostgreSQL provisioning behind
        // it, so no database var is passed and the hub uses its own default.
        let bin = std::env::var(wavvon_hub_env::HUB_BIN).unwrap_or_else(|_| self.hub_bin.clone());
        let mut cmd = tokio::process::Command::new(&bin);
        cmd.env(wavvon_hub_env::HTTP_PORT, port.to_string())
            .env(wavvon_hub_env::VOICE_UDP_PORT, voice_port.to_string());
        tracing::warn!(
            hub_id,
            db_path,
            "no per-hub database provisioning: this hub will use the default \
             WAVVON_DATABASE_URL and share it with every other spawned hub"
        );
        if let Some(pk) = owner_pubkey {
            cmd.env(wavvon_hub_env::OWNER_PUBKEY, pk);
        }
        if let Some(url) = farm_url {
            cmd.env(wavvon_hub_env::FARM_URL, url);
        }
        let child = cmd.spawn().with_context(|| format!("spawn hub {hub_id}"))?;
        self.hubs.write().await.insert(
            hub_id.to_string(),
            HubProcess {
                port,
                _child: child,
            },
        );
        tracing::info!(hub_id, port, voice_port, "Hub spawned");
        Ok(())
    }

    pub async fn stop_hub(&self, hub_id: &str) -> Result<()> {
        let mut hubs = self.hubs.write().await;
        if let Some(mut proc) = hubs.remove(hub_id) {
            proc._child.kill().await.ok();
            tracing::info!(hub_id, "Hub stopped");
        }
        Ok(())
    }

    /// Restart a hub process: stop it if running, then re-spawn it.
    pub async fn restart_hub(
        &self,
        hub_id: &str,
        db_path: &str,
        port: u16,
        voice_port: u16,
        owner_pubkey: Option<&str>,
        farm_url: Option<&str>,
    ) -> Result<()> {
        self.stop_hub(hub_id).await?;
        self.spawn_hub(hub_id, db_path, port, voice_port, owner_pubkey, farm_url)
            .await
    }

    pub async fn list_hubs(&self) -> Vec<serde_json::Value> {
        self.hubs
            .read()
            .await
            .iter()
            .map(|(id, p)| serde_json::json!({"hub_id": id, "port": p.port, "status": "running"}))
            .collect()
    }
}
