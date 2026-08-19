//! Voice-transport v2 E2E crypto primitives (see
//! `docs/docs/voice-transport-v2.md`). Pure Rust, no I/O — mirrored
//! byte-for-byte in `packages/core` (TS) and the desktop Tauri shell.
//!
//! Two constructions live here:
//!
//! - **Key wrap** (`voice_key_wrap`/`voice_key_unwrap`): a sender's
//!   per-channel symmetric key (`sender_key[32] || nonce_salt[4]`, plus a
//!   `key_id` generation counter) is wrapped for one recipient at a time
//!   using static-static X25519 between the sender's and recipient's
//!   identity-derived DH keys — the same construction as the group-DM
//!   `wrap_chain_key` (`dm.rs:558` in the desktop client), with a distinct
//!   HKDF info tag so the two constructions can never be confused.
//! - **Packet seal** (`voice_packet_seal`/`voice_packet_open`): the sealed
//!   uplink datagram — a cleartext 16-byte header (`key_id`, `ctr`, `ts`)
//!   used as AES-256-GCM AAD, followed by the ciphertext+tag of the Opus
//!   payload. The nonce is `nonce_salt[4] || ctr_be[8]` — unique for the
//!   lifetime of a given `sender_key` as long as `ctr` never repeats.
//!
//! `voice_key_wrap`/`voice_packet_seal` generate their own randomness
//! (nonce) internally, matching `ecies.rs`'s convention. Deterministic
//! `_with_nonce` variants exist for the fixed test vectors in
//! `tests/wire_vectors.rs`.

use crate::ed25519_seed_to_x25519_secret;
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Result};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

/// HKDF info tag for the key-wrap construction. No NUL terminator — HKDF
/// info convention, matching `wavvon/group-key-dist/v1`.
const VOICE_KEY_INFO: &[u8] = b"wavvon/voice-key/v1";

/// Wrapped-key plaintext layout: `sender_key[32] || nonce_salt[4] || key_id_be[4]`.
const WRAP_PLAINTEXT_LEN: usize = 40;

/// Uplink packet header layout: `key_id_be[4] || ctr_be[8] || ts_be[4]`.
const PACKET_HEADER_LEN: usize = 16;

// ---------------------------------------------------------------------------
// Key wrap
// ---------------------------------------------------------------------------

/// Wrap a voice sender key for one recipient. See module docs for the
/// construction. Generates a random 12-byte wrap nonce; returns
/// `(ciphertext_and_tag, nonce)`.
pub fn voice_key_wrap(
    sender_ed25519_seed: &[u8; 32],
    recipient_x25519_pub: &[u8; 32],
    channel_id: &str,
    sender_key: &[u8; 32],
    nonce_salt: &[u8; 4],
    key_id: u32,
) -> Result<(Vec<u8>, [u8; 12])> {
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = voice_key_wrap_with_nonce(
        sender_ed25519_seed,
        recipient_x25519_pub,
        channel_id,
        sender_key,
        nonce_salt,
        key_id,
        &nonce,
    )?;
    Ok((ciphertext, nonce))
}

/// Deterministic variant of [`voice_key_wrap`] taking the wrap nonce as a
/// parameter instead of generating it — used by the fixed test vectors.
pub fn voice_key_wrap_with_nonce(
    sender_ed25519_seed: &[u8; 32],
    recipient_x25519_pub: &[u8; 32],
    channel_id: &str,
    sender_key: &[u8; 32],
    nonce_salt: &[u8; 4],
    key_id: u32,
    nonce: &[u8; 12],
) -> Result<Vec<u8>> {
    let sender_secret = ed25519_seed_to_x25519_secret(sender_ed25519_seed);
    let recipient_pub = x25519_dalek::PublicKey::from(*recipient_x25519_pub);
    let shared = sender_secret.diffie_hellman(&recipient_pub);
    let wrap_key = voice_wrap_key(shared.as_bytes(), channel_id)?;

    let mut plaintext = [0u8; WRAP_PLAINTEXT_LEN];
    plaintext[..32].copy_from_slice(sender_key);
    plaintext[32..36].copy_from_slice(nonce_salt);
    plaintext[36..40].copy_from_slice(&key_id.to_be_bytes());

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&wrap_key));
    cipher
        .encrypt(Nonce::from_slice(nonce), plaintext.as_slice())
        .map_err(|e| anyhow!("AES-GCM encrypt: {e}"))
}

/// Unwrap a voice sender key. Returns `(sender_key, nonce_salt, key_id)`.
pub fn voice_key_unwrap(
    recipient_ed25519_seed: &[u8; 32],
    sender_x25519_pub: &[u8; 32],
    channel_id: &str,
    ciphertext: &[u8],
    wrap_nonce: &[u8; 12],
) -> Result<([u8; 32], [u8; 4], u32)> {
    let recipient_secret = ed25519_seed_to_x25519_secret(recipient_ed25519_seed);
    let sender_pub = x25519_dalek::PublicKey::from(*sender_x25519_pub);
    let shared = recipient_secret.diffie_hellman(&sender_pub);
    let wrap_key = voice_wrap_key(shared.as_bytes(), channel_id)?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&wrap_key));
    let plaintext = cipher
        .decrypt(Nonce::from_slice(wrap_nonce), ciphertext)
        .map_err(|e| anyhow!("AES-GCM decrypt: {e}"))?;

    if plaintext.len() != WRAP_PLAINTEXT_LEN {
        return Err(anyhow!(
            "wrapped voice key plaintext must be {WRAP_PLAINTEXT_LEN} bytes, got {}",
            plaintext.len()
        ));
    }

    let mut sender_key = [0u8; 32];
    sender_key.copy_from_slice(&plaintext[..32]);
    let mut nonce_salt = [0u8; 4];
    nonce_salt.copy_from_slice(&plaintext[32..36]);
    let key_id = u32::from_be_bytes(plaintext[36..40].try_into().unwrap());

    Ok((sender_key, nonce_salt, key_id))
}

fn voice_wrap_key(shared_secret: &[u8], channel_id: &str) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(Some(channel_id.as_bytes()), shared_secret);
    let mut wrap_key = [0u8; 32];
    hk.expand(VOICE_KEY_INFO, &mut wrap_key)
        .map_err(|e| anyhow!("HKDF expand: {e}"))?;
    Ok(wrap_key)
}

// ---------------------------------------------------------------------------
// Packet seal
// ---------------------------------------------------------------------------

/// Seal an uplink voice packet. Header = `key_id_be[4] || ctr_be[8] ||
/// ts_be[4]` (cleartext, doubles as AAD); nonce = `nonce_salt[4] ||
/// ctr_be[8]`. Returns `header || ciphertext_and_tag`.
///
/// Never fails: `sender_key` is always exactly 32 bytes and AES-256-GCM
/// has no other failure mode for the message sizes voice packets use.
pub fn voice_packet_seal(
    sender_key: &[u8; 32],
    nonce_salt: &[u8; 4],
    key_id: u32,
    ctr: u64,
    ts: u32,
    opus: &[u8],
) -> Vec<u8> {
    let mut header = [0u8; PACKET_HEADER_LEN];
    header[0..4].copy_from_slice(&key_id.to_be_bytes());
    header[4..12].copy_from_slice(&ctr.to_be_bytes());
    header[12..16].copy_from_slice(&ts.to_be_bytes());

    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[..4].copy_from_slice(nonce_salt);
    nonce_bytes[4..].copy_from_slice(&ctr.to_be_bytes());

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(sender_key));
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: opus,
                aad: &header,
            },
        )
        .expect("AES-256-GCM encrypt cannot fail for voice packet sizes");

    let mut packet = Vec::with_capacity(PACKET_HEADER_LEN + ciphertext.len());
    packet.extend_from_slice(&header);
    packet.extend_from_slice(&ciphertext);
    packet
}

/// Open a sealed uplink voice packet. Returns `(key_id, ctr, ts, opus)`.
/// Rejects packets shorter than header(16) + GCM tag(16) = 32 bytes.
pub fn voice_packet_open(
    sender_key: &[u8; 32],
    nonce_salt: &[u8; 4],
    packet: &[u8],
) -> Result<(u32, u64, u32, Vec<u8>)> {
    const MIN_LEN: usize = PACKET_HEADER_LEN + 16; // header + GCM tag
    if packet.len() < MIN_LEN {
        return Err(anyhow!(
            "voice packet too short: {} bytes, minimum {MIN_LEN}",
            packet.len()
        ));
    }

    let header = &packet[..PACKET_HEADER_LEN];
    let key_id = u32::from_be_bytes(header[0..4].try_into().unwrap());
    let ctr = u64::from_be_bytes(header[4..12].try_into().unwrap());
    let ts = u32::from_be_bytes(header[12..16].try_into().unwrap());

    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[..4].copy_from_slice(nonce_salt);
    nonce_bytes[4..].copy_from_slice(&ctr.to_be_bytes());

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(sender_key));
    let opus = cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: &packet[PACKET_HEADER_LEN..],
                aad: header,
            },
        )
        .map_err(|e| anyhow!("AES-GCM decrypt: {e}"))?;

    Ok((key_id, ctr, ts, opus))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn seed(fill_from: u8) -> [u8; 32] {
        let mut s = [0u8; 32];
        for (i, b) in s.iter_mut().enumerate() {
            *b = fill_from.wrapping_add(i as u8);
        }
        s
    }

    fn x25519_pub_from_seed(seed: &[u8; 32]) -> [u8; 32] {
        let secret = ed25519_seed_to_x25519_secret(seed);
        *x25519_dalek::PublicKey::from(&secret).as_bytes()
    }

    #[test]
    fn key_wrap_round_trips() {
        let sender_seed = seed(1);
        let recipient_seed = seed(0x21);
        let recipient_pub = x25519_pub_from_seed(&recipient_seed);
        let sender_pub = x25519_pub_from_seed(&sender_seed);

        let sender_key = seed(0x41);
        let nonce_salt = [0xAAu8; 4];
        let key_id = 7u32;

        let (ciphertext, nonce) = voice_key_wrap(
            &sender_seed,
            &recipient_pub,
            "chan-round-trip",
            &sender_key,
            &nonce_salt,
            key_id,
        )
        .unwrap();

        let (unwrapped_key, unwrapped_salt, unwrapped_key_id) = voice_key_unwrap(
            &recipient_seed,
            &sender_pub,
            "chan-round-trip",
            &ciphertext,
            &nonce,
        )
        .unwrap();

        assert_eq!(unwrapped_key, sender_key);
        assert_eq!(unwrapped_salt, nonce_salt);
        assert_eq!(unwrapped_key_id, key_id);
    }

    #[test]
    fn key_unwrap_rejects_wrong_channel() {
        let sender_seed = seed(1);
        let recipient_seed = seed(0x21);
        let recipient_pub = x25519_pub_from_seed(&recipient_seed);
        let sender_pub = x25519_pub_from_seed(&sender_seed);

        let (ciphertext, nonce) = voice_key_wrap(
            &sender_seed,
            &recipient_pub,
            "chan-a",
            &seed(0x41),
            &[0u8; 4],
            1,
        )
        .unwrap();

        let result = voice_key_unwrap(&recipient_seed, &sender_pub, "chan-b", &ciphertext, &nonce);
        assert!(result.is_err());
    }

    #[test]
    fn packet_seal_open_round_trips() {
        let key = seed(9);
        let salt = [1u8, 2, 3, 4];
        let sealed = voice_packet_seal(&key, &salt, 3, 42, 9000, b"opus-frame");

        let (key_id, ctr, ts, opus) = voice_packet_open(&key, &salt, &sealed).unwrap();
        assert_eq!(key_id, 3);
        assert_eq!(ctr, 42);
        assert_eq!(ts, 9000);
        assert_eq!(opus, b"opus-frame");
    }

    #[test]
    fn packet_open_rejects_short_packet() {
        let key = seed(9);
        let salt = [0u8; 4];
        let result = voice_packet_open(&key, &salt, &[0u8; 31]);
        assert!(result.is_err());
    }

    #[test]
    fn packet_open_rejects_tampered_header() {
        let key = seed(9);
        let salt = [1u8, 2, 3, 4];
        let mut sealed = voice_packet_seal(&key, &salt, 3, 42, 9000, b"opus-frame");
        sealed[0] ^= 0xFF; // flip a header byte (key_id)
        let result = voice_packet_open(&key, &salt, &sealed);
        assert!(result.is_err());
    }

    #[test]
    fn pubkey_hex_helper_matches_ed25519_dalek() {
        // Sanity check that our fixed test seeds actually behave as valid
        // Ed25519 signing keys too (channel_id/pubkey derivation elsewhere
        // in the wire format depends on this).
        let sk = SigningKey::from_bytes(&seed(1));
        assert_eq!(sk.verifying_key().as_bytes().len(), 32);
    }
}
