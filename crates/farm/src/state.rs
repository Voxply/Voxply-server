use std::collections::HashMap;
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::hub_manager::HubManager;

/// Shared state for the farm process.
pub struct FarmState {
    /// PostgreSQL connection pool for the farm database.
    pub db: PgPool,
    /// The farm's Ed25519 signing keypair — private half stays here only.
    pub keypair: Arc<SigningKey>,
    /// Canonical URL for this farm (e.g. `"https://farm.example.com"`).
    /// Embedded in every token as `iss`.
    pub farm_url: String,
    /// Last time (unix secs) we tried to re-fetch the farm pubkey after a
    /// verification failure. Used for rate-limiting the retry logic.
    pub last_pubkey_refresh: RwLock<i64>,
    /// Hub process lifecycle manager. Owns the map of running child processes.
    pub hub_manager: Arc<HubManager>,
    /// Shared HTTP client for outbound requests (proxying, health checks).
    pub http_client: reqwest::Client,
    /// Directory where hub data directories are stored.
    pub hubs_dir: String,
    /// Map server_id → bounded sender for the agent's WebSocket write half.
    /// Only present while the agent is connected.
    pub agent_senders: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<String>>>>,
}

impl FarmState {
    pub fn new(
        db: PgPool,
        keypair: SigningKey,
        farm_url: String,
        hub_manager: Arc<HubManager>,
        hubs_dir: String,
    ) -> Self {
        Self {
            db,
            keypair: Arc::new(keypair),
            farm_url,
            last_pubkey_refresh: RwLock::new(0),
            hub_manager,
            http_client: reqwest::Client::new(),
            hubs_dir,
            agent_senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Farm public key as a lowercase hex string.
    pub fn public_key_hex(&self) -> String {
        use ed25519_dalek::VerifyingKey;
        hex::encode(VerifyingKey::from(self.keypair.as_ref()).as_bytes())
    }

    /// Send a `restart_hub` command to the agent hosting `server_id`.
    ///
    /// Returns `Err(())` if that agent isn't currently connected (its sender
    /// isn't in `agent_senders`) or its send channel is full — callers should
    /// treat this as "agent offline" (503).
    pub async fn send_restart_to_agent(
        &self,
        server_id: &str,
        hub_id: &str,
        db_url: &str,
        port: u16,
        voice_port: u16,
        owner_pubkey: Option<&str>,
    ) -> Result<(), ()> {
        let sender = {
            let map = self.agent_senders.read().await;
            map.get(server_id).cloned()
        };
        let sender = sender.ok_or(())?;
        let cmd = serde_json::json!({
            "type": "restart_hub",
            "hub_id": hub_id,
            "db_url": db_url,
            "port": port,
            "voice_port": voice_port,
            "owner_pubkey": owner_pubkey,
            "farm_url": self.farm_url,
        });
        sender.try_send(cmd.to_string()).map_err(|_| ())
    }

    /// Return `existing` if set, else allocate a fresh voice port and persist
    /// it. Backfills hubs created before `voice_port` existed (or agent-hosted
    /// hubs whose `hub_spawned` confirmation predates this field).
    pub async fn resolve_voice_port(&self, hub_id: &str, existing: Option<i32>) -> u16 {
        if let Some(vp) = existing {
            return vp as u16;
        }
        let vp = self.hub_manager.allocate_voice_port().await;
        let _ = sqlx::query("UPDATE hubs SET voice_port = $1 WHERE id = $2")
            .bind(vp as i32)
            .bind(hub_id)
            .execute(&self.db)
            .await;
        vp
    }
}
