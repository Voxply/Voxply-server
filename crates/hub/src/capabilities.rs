//! What this hub can do, as a list of strings clients test membership in.
//!
//! Advertised on `GET /info` as `capabilities`. Clients decide what to render
//! by asking "is this string in the list", **never** by comparing `version` —
//! see decisions.md ("Hub capabilities are advertised, not inferred from a
//! version number"). `version` stays in `/info` for display.
//!
//! Why this matters here more than in most federated products: each hub bakes
//! a web client into its own image and serves it, and that client is
//! multi-hub. The copy served by hub A talks to hubs B and C, so there is no
//! "client and server update together" — the client's version is decided by
//! whichever hub the user happened to open and bears no relation to the hubs
//! it then talks to.
//!
//! **Adding a feature? Add its string here, in the same commit.** A forgotten
//! capability fails visibly (the feature never appears on this hub) rather
//! than silently. The list only ever grows — a removed string means every
//! older client stops offering the feature, which is a breaking change and
//! waits for a major (decisions.md, "Wire changes are additive").
//!
//! Keep it sorted; the test below enforces that so two features added in
//! parallel conflict in the merge instead of landing twice.

/// Capability strings this build advertises.
///
/// Seeded with the cases where a newer client would otherwise call an
/// endpoint an older hub does not have, or silently get a worse answer.
pub const CAPABILITIES: &[&str] = &[
    // One bot model: bots are invited by Ed25519 pubkey and authenticate on
    // the normal session path. A client that does not see this string is
    // talking to a hub that still has `POST /admin/bots`, so its admin panel
    // must offer the hub-minted-token flow instead of an invite field.
    "bots.external",
    // `/info` carries `max_attachment_bytes` and hub admin can change it. A
    // client that does not see this string is talking to a hub whose cap is a
    // compile-time 3 MB, so it must keep using its own constant rather than
    // trusting a field that will not be there.
    "limits.attachments",
    // `GET /users`, `GET /conversations/{id}/messages` and `GET
    // /admin/reports` honour `limit` + a keyset `cursor`. Without this the
    // hub ignores both and returns one truncated page — a client that pages
    // to exhaustion against an older hub sees a short list, not an error.
    "list.cursor",
    // Device pairing: subkey certs presented at `/auth/verify`, and the
    // ECIES-wrapped canonical DH material a paired device needs for DMs.
    "pairing.subkey",
    // Recovery contacts: signed `wavvon/recovery-request/v1` and
    // `wavvon/recovery-attestation/v1` envelopes and the endpoints behind
    // them.
    "recovery.attestation",
    // WebRTC screen-share v2 signalling (SDP/ICE relay). Mirrors the older
    // `screen_share_v2` boolean, which stays for clients that read it.
    "screenshare.v2",
    // Voice over WebTransport/QUIC with E2E sender keys (voice-transport-v2).
    // The raw-UDP and `/voice/ws` relays it replaced are gone, so a client
    // that does not see this string has no voice path to this hub at all.
    "voice.wt",
    // The WS answers `ping` with `pong`, echoing the nonce. Without this a
    // client measuring latency would wait for a reply that never comes and
    // show a dead "—" forever, so the readout has to be gated on it.
    "ws.ping",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_and_unique() {
        let mut sorted = CAPABILITIES.to_vec();
        sorted.sort_unstable();
        assert_eq!(CAPABILITIES, sorted.as_slice(), "keep CAPABILITIES sorted");

        sorted.dedup();
        assert_eq!(CAPABILITIES.len(), sorted.len(), "duplicate capability");
    }

    /// One spelling, so a client's string literal either matches or the
    /// capability was never advertised — no "was it a dash or a dot" bugs.
    #[test]
    fn names_are_lowercase_dotted() {
        for cap in CAPABILITIES {
            assert!(!cap.is_empty(), "empty capability string");
            assert!(
                cap.split('.').all(|part| !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())),
                "capability {cap:?} must be lowercase alphanumeric segments joined by dots",
            );
        }
    }
}
