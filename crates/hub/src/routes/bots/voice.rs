use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::state::AppState;

use super::models::bot_session;
use crate::auth::middleware::AuthUser;

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
