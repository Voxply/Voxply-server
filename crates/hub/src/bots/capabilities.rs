//! Effective-capability resolver (bot-capability-layer.md §1).
//!
//! Capabilities are requested by the bot (self-declared,
//! `bot_profiles.capabilities`) and granted by an admin
//! (`bot_capability_grants`). The only set any gate should trust is the
//! *effective* one -- the runtime never reads either source table directly.
//!
//! There is one bot system: a bot holds its own keypair, is invited by
//! pubkey, and self-declares what it wants in `bot_profiles.capabilities`
//! at auth/profile-update time. The gate is **requested ∩ granted** with no
//! exceptions — an admin can never silently hand a bot a capability it
//! never asked for, and a grant for a pubkey that declared nothing is
//! inert.
//!
//! This used to carry a second branch: self-service bots (the `bots` table,
//! bearer-token auth) had no way to declare anything, so a grant alone was
//! effective for any pubkey with no `bot_profiles` row. That system is gone
//! (decisions.md, "Every bot is an external bot") and with it the case
//! where an orphaned grant stood on its own.

use std::collections::HashSet;

use sqlx::PgPool;

/// The bot's effective capability set: requested ∩ granted. Empty for an
/// unknown pubkey, and empty for a bot that has been granted something it
/// never declared.
pub async fn effective_capabilities(db: &PgPool, bot_pubkey: &str) -> HashSet<String> {
    let granted: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT capability FROM bot_capability_grants WHERE bot_pubkey = $1",
    )
    .bind(bot_pubkey)
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    if granted.is_empty() {
        return granted;
    }

    let requested_json: Option<String> =
        sqlx::query_scalar("SELECT capabilities FROM bot_profiles WHERE pubkey = $1")
            .bind(bot_pubkey)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();

    match requested_json {
        Some(json) => {
            let requested: HashSet<String> = serde_json::from_str::<Vec<String>>(&json)
                .unwrap_or_default()
                .into_iter()
                .collect();
            requested.intersection(&granted).cloned().collect()
        }
        // No profile row means nothing was ever declared, so nothing is
        // effective — a grant is half of a handshake, not permission.
        None => HashSet::new(),
    }
}

/// Convenience: whether `capability` is in the bot's effective set.
pub async fn has_capability(db: &PgPool, bot_pubkey: &str, capability: &str) -> bool {
    effective_capabilities(db, bot_pubkey)
        .await
        .contains(capability)
}
