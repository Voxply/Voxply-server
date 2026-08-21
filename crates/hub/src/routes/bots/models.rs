use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::routes::bot_models::{BotCommandDef, BotMeta, BotSubscription};

// ---------------------------------------------------------------------------
// Audit log route types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AuditLogQuery {
    pub event_type: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct AuditLogEntry {
    pub seq: i64,
    pub event_type: String,
    pub at: i64,
    pub actor_pubkey: Option<String>,
    pub target_pubkey: Option<String>,
    pub channel_id: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Serialize)]
pub struct AuditLogResponse {
    pub entries: Vec<AuditLogEntry>,
    pub next_cursor: Option<i64>,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The authenticated bot behind a session token.
pub struct BotSession {
    pub public_key: String,
    pub display_name: String,
}

/// Resolve an authenticated session to the bot that owns it.
///
/// Every bot is an external bot (decisions.md, "Every bot is an external
/// bot"): it holds its own Ed25519 keypair and reaches this the same way a
/// member does — challenge-response, then a session token — so there is no
/// bot-specific auth path left. This only adds the `is_bot` check, which is
/// what separates a bot session from a human one.
///
/// The display name prefers `bot_profiles.name` (what the bot calls itself)
/// over `users.display_name` (what the inviting admin typed), so a bot that
/// renames itself is not shown under a stale label.
pub async fn bot_session(
    db: &sqlx::PgPool,
    user: &crate::auth::middleware::AuthUser,
) -> Result<BotSession, (StatusCode, String)> {
    let row: Option<(bool, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT u.is_bot, p.name, u.display_name
         FROM users u LEFT JOIN bot_profiles p ON p.pubkey = u.public_key
         WHERE u.public_key = $1",
    )
    .bind(&user.public_key)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;

    let (is_bot, profile_name, user_name) =
        row.ok_or((StatusCode::UNAUTHORIZED, "Unknown identity".to_string()))?;

    if !is_bot {
        return Err((
            StatusCode::FORBIDDEN,
            "This endpoint is for bots; the session belongs to a member".to_string(),
        ));
    }

    Ok(BotSession {
        public_key: user.public_key.clone(),
        display_name: profile_name
            .or(user_name)
            .unwrap_or_else(|| "bot".to_string()),
    })
}

// ---------------------------------------------------------------------------
// DB row types
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
pub struct EventRow {
    pub id: String,
    pub event_type: String,
    pub payload: String,
    pub created_at: i64,
}

// ---------------------------------------------------------------------------
// Admin request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SetCapabilitiesRequest {
    pub capabilities: Vec<String>,
}

#[derive(Serialize)]
pub struct CapabilitiesResponse {
    pub bot_pubkey: String,
    pub capabilities: Vec<String>,
}

/// GET /admin/bots/:pubkey/capabilities response (bot-capability-layer.md
/// §1, §6 Phase 1 item 2 follow-up): the three sets side by side so the
/// admin panel can render requested vs. granted vs. what's actually live.
#[derive(Serialize)]
pub struct CapabilitiesReadResponse {
    /// Self-declared capabilities (`bot_profiles.capabilities`). A bot that
    /// has declared nothing has nothing effective, whatever it was granted.
    pub requested: Vec<String>,
    /// Admin-granted rows from `bot_capability_grants`.
    pub granted: Vec<String>,
    /// `effective_capabilities()` output -- the only set any runtime gate trusts.
    pub effective: Vec<String>,
}

// ---------------------------------------------------------------------------
// Bot API request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BotSendRequest {
    pub channel_id: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct PollQuery {
    pub since: Option<i64>,
}

#[derive(Serialize)]
pub struct EventInfo {
    pub id: String,
    pub event_type: String,
    pub payload: String,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct AckRequest {
    pub ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// External bot system types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct InviteBotRequest {
    pub pubkey: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Serialize)]
pub struct InviteBotResponse {
    pub invite_token: String,
}

#[derive(Deserialize)]
pub struct AcceptInviteRequest {
    pub pubkey: String,
    pub signature_over_token: String,
    pub bot_meta: BotMeta,
}

#[derive(Serialize)]
pub struct AcceptInviteResponse {
    pub status: String,
}

/// `GET /admin/bots/external` row (bots.md §4 "Admin UI"): the admin
/// management view over `users` rows with `is_bot=1`, unlike `BotListEntry`
/// (the member-facing `GET /bots` directory) this includes pending invites,
/// removed bots, and the admin-only local note.
#[derive(Serialize)]
pub struct ExternalBotAdminInfo {
    pub public_key: String,
    pub display_name: Option<String>,
    pub local_note: Option<String>,
    pub approval_status: &'static str,
    pub last_seen_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct SetChannelScopeRequest {
    #[serde(default)]
    pub channel_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct ChannelScopeResponse {
    pub bot_pubkey: String,
    pub channel_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct BotListEntry {
    pub pubkey: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    /// Profile-declared game descriptor (bot-capability-layer.md §11): the
    /// per-hub bot directory's Play affordance. Absent = no game declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<crate::routes::bot_models::GameLaunchCard>,
    pub commands: Vec<BotCommandSummary>,
}

#[derive(Serialize)]
pub struct BotCommandSummary {
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct BotMeResponse {
    pub pubkey: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage_url: Option<String>,
    pub capabilities: Vec<String>,
    pub commands: Vec<BotCommandDef>,
}

#[derive(Deserialize)]
pub struct UpdateCommandsRequest {
    pub commands: Vec<BotCommandDef>,
}

#[derive(Deserialize)]
pub struct UpdateSubscriptionsRequest {
    pub subscriptions: Vec<BotSubscription>,
}

#[derive(Serialize)]
pub struct SetSubscriptionsResponse {
    pub count: usize,
}

// ---------------------------------------------------------------------------
// External bot DB row helpers
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
pub struct BotProfileRow {
    pub pubkey: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub description: Option<String>,
    pub webhook_url: Option<String>,
    pub homepage_url: Option<String>,
    pub capabilities: String,
}

#[derive(sqlx::FromRow)]
pub struct BotCommandRow {
    pub name: String,
    pub description: String,
    pub args: Option<String>,
    pub scope: String,
    pub privileged: bool,
    pub cooldown_seconds: i64,
}
