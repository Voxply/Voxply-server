/// Hub process lifecycle manager.
///
/// Owns the map of running hub child processes and exposes spawn/stop/restart
/// operations. On farm startup `spawn_all_from_db` re-spawns every non-suspended,
/// non-deleted hub found in the `hubs` table.
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use sqlx::PgPool;
use tokio::process::Child;
use tokio::sync::RwLock;

struct HubProcess {
    port: u16,
    voice_port: u16,
    child: Child,
}

pub struct HubManager {
    hubs: RwLock<HashMap<String, HubProcess>>,
    /// Absolute path (or name on PATH) of the `wavvon-hub` binary.
    hub_bin: String,
    /// Externally reachable farm URL — passed to hub processes as `WAVVON_FARM_URL`.
    farm_url: String,
    /// Base port for allocating new hub process HTTP ports.
    base_port: u16,
    /// Base port for allocating new hub voice (WebTransport/QUIC over UDP) ports.
    /// Separate range from `base_port` so the two never collide.
    voice_base_port: u16,
}

impl HubManager {
    pub fn new(hub_bin: String, farm_url: String, base_port: u16, voice_base_port: u16) -> Self {
        Self {
            hubs: RwLock::new(HashMap::new()),
            hub_bin,
            farm_url,
            base_port,
            voice_base_port,
        }
    }

    /// Allocate the next free HTTP port for a new hub process.
    /// Scans occupied ports and returns `base_port + N` where N is the first gap.
    pub async fn allocate_port(&self) -> u16 {
        let hubs = self.hubs.read().await;
        let mut port = self.base_port;
        let occupied: std::collections::HashSet<u16> = hubs.values().map(|h| h.port).collect();
        while occupied.contains(&port) {
            port += 1;
        }
        port
    }

    /// Allocate the next free voice port for a new hub process. Same first-gap
    /// strategy as `allocate_port`, over the separate `voice_base_port` range.
    pub async fn allocate_voice_port(&self) -> u16 {
        let hubs = self.hubs.read().await;
        let mut port = self.voice_base_port;
        let occupied: std::collections::HashSet<u16> =
            hubs.values().map(|h| h.voice_port).collect();
        while occupied.contains(&port) {
            port += 1;
        }
        port
    }

    /// Spawn a hub child process.
    ///
    /// The hub binary is resolved from `WAVVON_HUB_BIN` env var, falling back to
    /// the path stored in `self.hub_bin`.
    ///
    /// `owner_pubkey` is passed as `WAVVON_OWNER_PUBKEY` so the hub seeds that key
    /// as the builtin-owner role on first boot. `voice_port` is passed as
    /// `WAVVON_VOICE_UDP_PORT` so multiple farm-spawned hubs on one box don't
    /// collide on the default 3001.
    pub async fn spawn_hub(
        &self,
        hub_id: &str,
        db_path: &str,
        port: u16,
        voice_port: u16,
        owner_pubkey: Option<&str>,
    ) -> Result<()> {
        let bin = std::env::var(wavvon_hub_env::HUB_BIN).unwrap_or_else(|_| self.hub_bin.clone());

        // Names come from `wavvon_hub_env` rather than string literals. This
        // call used to set WAVVON_HUB_HTTP_PORT, a name the hub never reads —
        // so every spawned hub ignored its carefully allocated port and bound
        // the default 3000, and the proxy routed to a port nothing listened
        // on. Nothing failed loudly, which is why it survived.
        let mut cmd = tokio::process::Command::new(&bin);
        // Tokio detaches a child on drop rather than killing it — see the same
        // note in agent/src/hub_manager.rs. Without this, dropping the manager
        // without calling `stop_hub` orphans a hub that keeps its port and
        // keeps writing to the shared default database, supervised by nobody.
        cmd.kill_on_drop(true)
            .env(wavvon_hub_env::HTTP_PORT, port.to_string())
            .env(wavvon_hub_env::VOICE_UDP_PORT, voice_port.to_string())
            .env(wavvon_hub_env::FARM_URL, &self.farm_url)
            // Our row id for this hub. It reports this back on its first
            // heartbeat, which is the only way we learn its pubkey and can
            // start routing `/hub/<serial>` to it.
            .env(wavvon_hub_env::FARM_HUB_ID, hub_id);
        if let Some(pk) = owner_pubkey {
            cmd.env(wavvon_hub_env::OWNER_PUBKEY, pk);
        }

        // `db_path` is still a leftover SQLite-era file path
        // (`{hubs_dir}/{hub_id}.db`) and there is no per-hub PostgreSQL
        // provisioning yet, so nothing is passed for the hub's database and
        // it falls back to its own default. Two hubs on one box therefore
        // share a database. That used to be invisible — the farm set
        // WAVVON_HUB_DB, which the hub has never read, so it looked handled.
        // Warn until provisioning lands (ROADMAP: farm multi-node data plane
        // lists per-node PostgreSQL as a prerequisite).
        tracing::warn!(
            hub_id,
            db_path,
            "no per-hub database provisioning: this hub will use the default \
             WAVVON_DATABASE_URL and share it with every other farm-spawned hub"
        );
        let child = cmd.spawn().with_context(|| {
            format!("Failed to spawn hub process for {hub_id} (binary: {bin:?})")
        })?;

        let mut hubs = self.hubs.write().await;
        hubs.insert(
            hub_id.to_string(),
            HubProcess {
                port,
                voice_port,
                child,
            },
        );
        tracing::info!(hub_id, port, voice_port, "Hub process spawned");
        Ok(())
    }

    /// Stop a running hub process (SIGTERM on Unix, TerminateProcess on Windows).
    pub async fn stop_hub(&self, hub_id: &str) -> Result<()> {
        let mut hubs = self.hubs.write().await;
        if let Some(mut proc) = hubs.remove(hub_id) {
            proc.child
                .kill()
                .await
                .with_context(|| format!("Failed to kill hub process {hub_id}"))?;
            tracing::info!(hub_id, "Hub process stopped");
        }
        Ok(())
    }

    /// Restart a hub process: stop it then re-spawn with the same db_path, port
    /// and voice_port.
    pub async fn restart_hub(
        &self,
        hub_id: &str,
        db_path: &str,
        port: u16,
        voice_port: u16,
    ) -> Result<()> {
        self.stop_hub(hub_id).await?;
        self.spawn_hub(hub_id, db_path, port, voice_port, None)
            .await
    }

    /// Whether a hub process is currently tracked as running.
    pub async fn is_running(&self, hub_id: &str) -> bool {
        self.hubs.read().await.contains_key(hub_id)
    }

    /// Return the port the named hub process is listening on, if running.
    pub async fn port_of(&self, hub_id: &str) -> Option<u16> {
        self.hubs.read().await.get(hub_id).map(|h| h.port)
    }

    /// Re-spawn all non-suspended, non-deleted hubs from the DB.
    /// Called once at farm startup.
    ///
    /// Hubs created before `voice_port` existed have it NULL — allocate and
    /// persist one now rather than falling back to the fatal-collision default.
    pub async fn spawn_all_from_db(&self, db: &PgPool) -> Result<()> {
        // process_port/voice_port are INTEGER (i32) — decoding as i64 fails sqlx's
        // type check at runtime (same latent bug the proxy had; see proxy.rs).
        #[allow(clippy::type_complexity)]
        let rows: Vec<(String, String, i32, Option<i32>, Option<String>)> = sqlx::query_as(
            "SELECT id, db_path, process_port, voice_port, owner_pubkey FROM hubs
             WHERE suspended_at IS NULL AND deleted_at IS NULL AND process_port IS NOT NULL",
        )
        .fetch_all(db)
        .await
        .context("Failed to query hubs for startup spawn")?;

        for (hub_id, db_path, port, voice_port, owner_pubkey) in rows {
            let port = port as u16;
            let voice_port = match voice_port {
                Some(vp) => vp as u16,
                None => {
                    let vp = self.allocate_voice_port().await;
                    let _ = sqlx::query("UPDATE hubs SET voice_port = $1 WHERE id = $2")
                        .bind(vp as i32)
                        .bind(&hub_id)
                        .execute(db)
                        .await;
                    vp
                }
            };
            if let Err(e) = self
                .spawn_hub(&hub_id, &db_path, port, voice_port, owner_pubkey.as_deref())
                .await
            {
                tracing::warn!(hub_id, error = %e, "Failed to spawn hub on startup (skipping)");
            }
        }

        Ok(())
    }

    /// Allocate an HTTP port and a voice port and persist both to the `hubs`
    /// row, then spawn. Returns the allocated `(port, voice_port)`.
    pub async fn allocate_and_spawn(
        self: &Arc<Self>,
        db: &PgPool,
        hub_id: &str,
        db_path: &str,
        owner_pubkey: Option<&str>,
    ) -> Result<(u16, u16)> {
        let port = self.allocate_port().await;
        let voice_port = self.allocate_voice_port().await;

        // Persist ports before spawning so a restart can re-use them.
        sqlx::query("UPDATE hubs SET process_port = $1, voice_port = $2 WHERE id = $3")
            .bind(port as i64)
            .bind(voice_port as i64)
            .bind(hub_id)
            .execute(db)
            .await
            .context("Failed to persist hub ports")?;

        self.spawn_hub(hub_id, db_path, port, voice_port, owner_pubkey)
            .await?;
        Ok((port, voice_port))
    }
}
