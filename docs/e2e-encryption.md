# End-to-End Encrypted Direct Messages

DMs today are plaintext in the hub's SQLite (`dm_messages.content`,
`server/voxply-hub/src/db/migrations.rs:432-443`). The hub operator can
read every conversation. This doc designs E2E encryption that lets hubs
**store and relay** without **decrypt**. v1 covers 1:1 text DMs and their
attachments only.

---

## Threat model

| Protects against | Does NOT protect against |
|---|---|
| Hub operator reading message content | Metadata — who↔who, when, frequency, sizes |
| DB dump from a compromised hub | Hub-injected fake plaintext (signature catches it for ciphertexts; legacy plaintext is unsigned) |
| Subpoena of hub data at rest | Recipient private key compromise |
| Hub-to-hub federation interception (encrypted payload crosses the wire) | Endpoint compromise (malware on either device) |

The hub still routes, queues, and broadcasts. Sender/recipient pubkeys,
conversation IDs, timestamps, and attachment sizes are all visible to it.
Hiding metadata is out of scope; that's a different design (cover traffic,
mix networks) and not pursued in v1.

---

## Key material

Each user already has an Ed25519 **identity key** for signing
(`shared/voxply-identity/src/lib.rs:23-127`). For E2E we add an
**X25519 DH key** for key agreement, deterministically derived from the
same seed — no new key storage.

| Key | Curve | Use | Storage |
|---|---|---|---|
| `identity_key` | Ed25519 | Sign messages, certs, profiles | `~/.voxply/identity.json` (existing) |
| `dh_key` | X25519 | ECDH for message encryption | Derived on demand from identity seed |

### Derivation

Standard `ed25519_sk_to_x25519` (the same trick as libsodium's
`crypto_sign_ed25519_sk_to_curve25519`):

1. Take the 32-byte Ed25519 seed (`signing_key.to_bytes()`).
2. `SHA-512(seed)` → 64 bytes; first 32 bytes become the X25519 scalar
   candidate.
3. Apply X25519 clamping (`scalar[0] &= 248; scalar[31] &= 127; scalar[31] |= 64`).
4. That clamped scalar is the X25519 private key; `X25519(scalar, basepoint)`
   gives the public DH key.

**Caveats** (document, do not paper over):

- This couples the DH key to the identity key. A compromise of the
  identity seed is a compromise of the DH key. They share the same secret.
- The conversion is well-studied and safe for this exact pair (Ed25519
  signing + X25519 DH from the same seed). Do **not** generalise to "any
  signing key derives an encryption key" — the safety is specific to this
  curve pair and clamping recipe.
- Subkeys (`SubkeyCert`) do not derive their own DH keys. See "Multi-device"
  below — DH key always comes from the **master** seed.

### Publication

New endpoint on each home hub:

```
PUT /identity/:pubkey/dh-key
    body: { dh_pubkey_hex, signature_hex }
    signature signs: "voxply/dh-key/v1\0" + pubkey + dh_pubkey
→ 200 | 401 (auth) | 400 (sig mismatch)

GET /identity/:pubkey/dh-key
→ { dh_pubkey_hex, signature_hex, published_at } | 404
```

Public read (no auth) so anyone can fetch a DH key to start a
conversation. Write is authenticated (the user must be signed in) **and**
signature-verified against `pubkey` — the hub stores it as-is and any
relying client re-verifies. Storage is a new `dh_keys` row keyed by
`pubkey`. Replicated across the user's home hub list, same shape as
`home_hub_designations`.

The signing pattern follows the existing wire types in
`shared/voxply-identity/src/wire.rs:32-66` — domain-separated prefix,
length-prefixed strings, identity-key signature.

---

## 1:1 message encryption (v1: static ECDH + AES-GCM)

For Alice → Bob in conversation `conv_id`:

| Step | Operation |
|---|---|
| 1 | Alice fetches Bob's published DH pubkey from his home hub (or cache) |
| 2 | `shared = X25519(alice_dh_priv, bob_dh_pub)` |
| 3 | `key = HKDF-SHA256(shared, salt=conv_id, info="voxply/dm-key/v1")` |
| 4 | `nonce = random(12)` |
| 5 | `ct = AES-256-GCM(key, nonce, plaintext_json)` where `plaintext_json` is `{ content, attachments? }` |
| 6 | `sig = Ed25519.sign(identity_key, canonical_bytes)` (see §"Message authentication") |
| 7 | Send `{ ciphertext_hex, nonce_hex, dh_pubkey_hex, signature_hex }` to the hub |

Bob decrypts symmetrically: rederives `shared = X25519(bob_dh_priv, alice_dh_pub_from_envelope)`,
derives the same key, AES-GCM-opens with the nonce.

### Why static ECDH (and what it costs)

| Property | Static ECDH (v1) | Double Ratchet (v2) |
|---|---|---|
| Confidentiality at rest | yes | yes |
| Forward secrecy (past msgs safe if key leaks later) | **no** | yes |
| Post-compromise security | no | yes |
| Implementation surface | ~200 LoC | several thousand LoC, state machine, key store |
| Group support | not natively | per-sender ratchet |

v1 ships static ECDH. v2 swaps in Signal's Double Ratchet. The envelope
shape already carries `dh_pubkey_hex` per message, which is the same slot
the ratchet's ephemeral key would live in — the protocol version bump is
the migration trigger, no schema change.

---

## Message authentication

AES-GCM authenticates the ciphertext under the derived key, but two
parties share the key — Bob can't distinguish "Alice sent this" from
"someone with the key forged it". For DMs that's only Alice, but the
hub is the relay and we don't want a malicious hub injecting valid-looking
plaintexts after a key compromise. The Ed25519 signature on the envelope
binds the ciphertext to Alice's identity key, which the hub does not have.

**Canonical bytes** (domain-separated, length-prefixed, matches the
pattern in `shared/voxply-identity/src/wire.rs`):

```
"voxply/dm-ciphertext/v1\0"
|| len_prefixed(conv_id)
|| len_prefixed(ciphertext_hex)
|| len_prefixed(nonce_hex)
|| len_prefixed(dh_pubkey_hex)
```

### Envelope (stored verbatim by the hub)

```json
{
  "sender_pubkey":   "...",
  "conv_id":         "...",
  "ciphertext_hex":  "...",
  "nonce_hex":       "...",
  "dh_pubkey_hex":   "...",
  "signature_hex":   "..."
}
```

The hub verifies the signature before storing (so it can't be used as a
write-amplification target for garbage). The hub does **not** verify the
ciphertext makes sense — it can't; only the recipient can.

---

## Group DMs — deferred to v2

v1 group DMs (`conv_type = 'group'`, ≥3 members) stay **plaintext**. The
mixed-conversation UI (§"Client-side changes") covers the visual signal.

**Why deferred**: pairwise static ECDH for N members means N(N−1)/2 key
agreements and re-encrypting every message N−1 times. Plus membership
churn (add/remove) without a sender-key scheme means key reuse across
membership generations — bad.

**v2 sketch** (sender-key, Signal-style):

| Element | Shape |
|---|---|
| Sender key | One ChaCha20-Poly1305 symmetric key per `(group, sender)` pair |
| Distribution | Sender encrypts the sender-key once **per recipient** under their published DH key; pushes those wrapped blobs to the hub |
| Use | Sender encrypts each message with their sender key + a per-message counter nonce; recipients decrypt with the wrapped key they received |
| Membership change | Sender rotates their sender key and re-distributes to the new membership set |
| Forward secrecy | Per-sender symmetric ratchet on each message |

Not building this in v1. A code-level marker in the encrypt path
(`if conv_type == "group" return plaintext`) and a clear note in the
design doc is the bookkeeping.

---

## Hub-side changes

The hub is **storage and relay**. No key material lives on the hub.

| Change | File | Note |
|---|---|---|
| `dm_messages.is_encrypted` BOOLEAN DEFAULT 0 | `server/voxply-hub/src/db/migrations.rs` | Additive `ALTER TABLE` — legacy rows stay 0 |
| `dm_messages.ciphertext_json` TEXT NULL | same | Holds the envelope JSON for encrypted msgs; `content` stays NULL when encrypted |
| New table `dh_keys (pubkey PK, dh_pubkey_hex, signature_hex, published_at)` | same | One row per user; replicated across the home hub list like `home_hub_designations` |
| `GET/PUT /identity/:pubkey/dh-key` routes | new `routes/dh_keys.rs` | Mirrors the existing identity-keyed write+read shape |
| `send_dm` accepts encrypted envelopes | `server/voxply-hub/src/routes/dms.rs:132-288` | New `SendDmRequest` variant; on encrypted, verifies signature, persists envelope, leaves `content` NULL |
| `list_dm_messages` returns the envelope when `is_encrypted=1` | same file, ~290 | `content` field becomes `Option<String>`; client decodes ciphertext locally |
| Federated DM delivery carries the envelope | `FederatedDmRequest` in `routes/dm_models.rs` | New optional `encrypted_envelope` field; existing `content/signature` stays for legacy plaintext peers |

**Hub validation**: on encrypted send, the hub verifies the Ed25519
signature on the envelope before INSERT. Garbage envelopes get a 400.
This is the same defense pattern as `home_hub_designations` writes.

**What breaks**: search. `messages` channel search and DM search both
operate on `content`. Encrypted DMs are search-invisible to the server;
they would have to be searched client-side after decrypt, which we don't
build in v1. Document the gap; users see "encrypted messages aren't
indexed" in the search UI.

---

## Client-side changes

| Change | Where |
|---|---|
| `Identity::dh_keypair() -> X25519KeyPair` | `shared/voxply-identity/src/lib.rs` (extends the existing `Identity`) |
| Tauri command `publish_dh_key` | `client/voxply-desktop/src-tauri` — runs on first launch post-upgrade, or after identity creation |
| Tauri command `fetch_dh_key(pubkey, hub_url)` | with a local cache, 24 h TTL, evict on signature-verify failure |
| DM send path: encrypt when recipient DH key is known | `client/voxply-desktop/src/...` DM send handler |
| DM send path: warn-then-send-plaintext otherwise | UI banner: "Recipient hasn't published an encryption key — this message will not be encrypted." User confirms or cancels |
| DM receive path: detect `is_encrypted`, decrypt locally | same handler that processes `dm_messages` and the `DmEvent::Message` WS stream |
| Lock-icon UI | Per-message indicator. Closed lock = E2E. Open lock = plaintext. Tooltip explains. Mixed conversations are allowed |
| Group DM banner | "Group DMs are not encrypted yet." Always shown above a group conversation; removed when v2 ships |

The DH keypair is derived on demand, not stored. The Ed25519 seed in
`~/.voxply/identity.json` is the only secret on disk; that file already
needs to be protected.

---

## Migration / rollout

No flag day. No "upgrade conversation" ceremony.

| State | Behaviour |
|---|---|
| New client → recipient with published DH key | Encrypt. Lock icon closed. |
| New client → recipient with no published DH key | Warn user. Send plaintext if confirmed. Lock icon open. |
| New client → legacy client (no E2E support) | Same as "no DH key published" — plaintext. |
| Legacy client → anyone | Plaintext, exactly as today. |
| Reading old plaintext messages | Renders verbatim. No lock icon. |

A conversation may contain a mix of plaintext and encrypted messages
during the transition. The per-message lock icon makes the state explicit.
After everyone in a conversation has upgraded **and** published a DH key,
every new message will be encrypted automatically — no user action needed.

---

## Attachment encryption

Same scheme, same key. The `plaintext_json` we encrypt in §"1:1 message
encryption" already includes `attachments`. The attachment bytes (base64
in `Attachment.data_b64`, see `routes/chat_models.rs`) are part of the
plaintext blob and ride inside the same AES-GCM ciphertext.

Cap stays the same (`MAX_ATTACHMENTS_BYTES`, ~3 MB). AES-GCM expansion is
16 bytes, negligible against the cap. No separate attachment ciphertext
table.

---

## Open questions

| Question | Working answer |
|---|---|
| **Identity regeneration** — user wipes and regenerates their identity; the new DH key can't decrypt old messages encrypted to the old key. | Keep old DH private keys in `~/.voxply/old_dh_keys.json` for decryption only. Never publish or encrypt with old keys. Eviction policy TBD (probably never — they're 32 bytes each). |
| **Multi-device** — Alice has two devices with different subkeys (`SubkeyCert`). Which DH key does Bob encrypt to? | Always the **master** seed's DH derivation. Master is the same across all of Alice's paired devices; subkeys do not derive their own DH keys. This matches the "two-axis state" rule — DH key is personal-axis, anchored to the master. |
| **Subkey-only devices** — what if a device was paired and only holds a subkey, not the master? | It receives the master's DH private key wrapped in the `PairingComplete` payload (the existing `wrapped_blob_key_hex` slot is the natural carrier; add a sibling `wrapped_dh_seed_hex` next to it). Document in `multi-device.md` cross-link. |
| **DH key rotation** for forward secrecy without a full ratchet | Out of scope for v1. v2 is the ratchet. A "rotate DH key" command that pushes a new key + signature would orphan past-messages on every recipient — not worth the half-measure. |
| **Hub-side abuse: encrypted blobs as a storage attack** | Same per-message size cap as plaintext today. No new attack surface beyond what plaintext DMs already allow. |

---

## What's deferred

- Group DM encryption (v2, sender-key sketch above)
- Forward secrecy / Double Ratchet (v2)
- Encrypted search (probably never; search becomes client-side post-decrypt if it returns at all)
- Voice/video/screenshare encryption — those are not in DMs yet; their own design will tackle SRTP / DTLS-SRTP when the time comes
- Encrypted typing indicators and read receipts — currently out-of-band signals; revisit if they leak useful content (typing doesn't, read receipts barely)
- Cover traffic / metadata hiding — not pursued

---

## Cross-references

- Identity model and Ed25519 seed: `shared/voxply-identity/src/lib.rs:23-127`
- Existing signed wire types (signing pattern this doc reuses): `shared/voxply-identity/src/wire.rs:32-191`
- Current plaintext DM storage and federation path: `server/voxply-hub/src/routes/dms.rs`
- DM schema: `server/voxply-hub/src/db/migrations.rs:432-471`
- Multi-device / master + subkey: [multi-device.md](multi-device.md)
- Home hub list (where DH keys are replicated): [home-hub.md](home-hub.md)
- Threat model (the broader view this doc slots into): [threat-model.md](threat-model.md)
