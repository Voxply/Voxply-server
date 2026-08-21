use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

use super::models::bot_session;
use crate::auth::middleware::AuthUser;

#[derive(Deserialize)]
pub struct VoiceJoinRequest {
    pub channel_id: String,
}

#[derive(Serialize)]
pub struct VoiceJoinResponse {
    pub ws_url: String,
    pub channel_id: String,
}

/// POST /bots/{id}/voice/join
///
/// Tells the hub which voice channel the bot wants to join. The bot must
/// authenticate as itself via `Authorization: Bearer <bot_token>` and the
/// `{id}` path parameter must match its own public key.
///
/// Returns the main hub WebSocket URL the bot should connect to with its
/// token as `?token=<bot_token>`, then send a `voice_join` message for
/// `channel_id` (voice-transport-v2.md: there is no dedicated voice
/// WebSocket anymore — every voice participant, bot or human, joins the
/// same way and gets a WebTransport session from the `voice_joined` reply).
pub async fn bot_voice_join(
    Path(bot_id): Path<String>,
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<VoiceJoinRequest>,
) -> Result<Json<VoiceJoinResponse>, (StatusCode, String)> {
    let bot = bot_session(&state.db, &user).await?;

    // Caller must be the bot identified by the path parameter.
    if bot.public_key != bot_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the bot itself may call this".into(),
        ));
    }

    // Verify channel exists and is not a category.
    let channel_exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM channels WHERE id = $1 AND is_category = false")
            .bind(&req.channel_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;

    if channel_exists.is_none() {
        return Err((StatusCode::NOT_FOUND, "Channel not found".into()));
    }

    // No gate here on purpose, and now for a sound reason rather than a
    // structural one: this endpoint hands back a URL, it does not admit the
    // bot to the channel. `can_speak_voice` is enforced where the join
    // actually happens, in `handle_voice_join`
    // (`routes/ws/handlers/voice.rs`), and since every bot is an external
    // bot it now covers all of them — the carve-out this comment used to
    // describe (self-service bots never populated `bot_profiles`, so the
    // gate silently skipped them) died with that system.

    // Return the path the bot should connect to: the main hub WS. The bot
    // already knows the hub base URL; it connects to /ws?token=<bot_token>
    // and sends {"type":"voice_join","channel_id":<id>}.
    let ws_url = "/ws".to_string();

    Ok(Json(VoiceJoinResponse {
        ws_url,
        channel_id: req.channel_id,
    }))
}

#[derive(Deserialize)]
pub struct VoiceLeaveRequest {
    pub channel_id: String,
}

/// DELETE /bots/{id}/voice/leave
///
/// Removes the bot from the specified voice channel using the same cleanup
/// path as a normal WebSocket disconnect. Idempotent — calling it when the
/// bot is not in the channel is a no-op.
pub async fn bot_voice_leave(
    Path(bot_id): Path<String>,
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<VoiceLeaveRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let bot = bot_session(&state.db, &user).await?;

    if bot.public_key != bot_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the bot itself may call this".into(),
        ));
    }

    // Trigger the same cleanup as a normal voice leave.
    crate::routes::ws::leave_voice(&state, &bot.public_key, &req.channel_id).await;

    Ok(StatusCode::NO_CONTENT)
}
