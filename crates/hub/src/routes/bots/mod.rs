mod admin;
mod bot_api;
mod external;
mod models;
pub mod screenshare;
pub mod voice;

// Re-export all public items so server.rs paths remain unchanged.
pub use admin::{
    admin_audit_log, admin_get_bot_capabilities, admin_get_bot_channel_scope,
    admin_set_bot_capabilities, admin_set_bot_channel_scope,
};
pub use bot_api::{bot_ack_events, bot_poll, bot_send_message};
pub use external::{
    admin_list_external_bots, ext_accept_invite, ext_bot_me, ext_invite_bot, ext_list_bots,
    ext_remove_bot, ext_update_bot_commands, ext_update_bot_profile, ext_update_bot_subscriptions,
};
// Re-export the audit log types that tests or other modules may reference.
pub use models::{AuditLogEntry, AuditLogQuery, AuditLogResponse};
