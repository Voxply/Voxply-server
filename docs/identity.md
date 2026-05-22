# Identity & Auth

Voxply has no accounts and no passwords. Identity is an Ed25519 keypair
held by the device. Everything else (membership, permissions, history)
hangs off the public key.

## Where identity lives

- **Generation**: `shared/voxply-identity/src/lib.rs` (Ed25519 + BIP39 phrase)
- **Recovery phrase**: `shared/voxply-identity/src/recovery.rs` (24 words)
- **Storage on the desktop client**: a JSON file in Tauri's app-data dir,
  written by the Rust side in `client/voxply-desktop/src-tauri/src/lib.rs`.

The recovery phrase **is** the secret — entering it on a device replaces
that device's identity. There is currently one identity per device.

## How auth works against a hub

Challenge-response, signature-based:

1. Client requests a challenge from the hub.
2. Hub returns a random nonce.
3. Client signs the nonce with its Ed25519 private key.
4. Client posts the signature + public key.
5. Hub verifies and issues a session token.

Code path: `server/voxply-hub/src/auth/handlers.rs` and
`server/voxply-hub/src/auth/middleware.rs`.

## Authorization (after auth)

A user's pubkey is matched to their hub-local membership row, which
carries their roles. Roles bundle permissions; see
`server/voxply-hub/src/permissions.rs` for the permission set and
`server/voxply-hub/src/routes/roles.rs` for role CRUD.

Common permissions: `manage_hub`, `manage_channels`, `manage_roles`,
`manage_users`, `send_messages`, `attach_files`, etc.

## Hub-to-hub auth (federation)

Same primitive, different actor: each hub also has its own Ed25519
keypair. When Hub A talks to Hub B, A signs requests as itself; B
verifies. See `server/voxply-hub/src/federation/client.rs` and
`federation/handlers.rs`.

## Recovery flow

1. User generates an identity → 24-word phrase shown once.
2. User pastes phrase on a new device (or the same device after wipe).
3. The phrase deterministically yields the Ed25519 keypair. Same phrase
   ⇒ same pubkey ⇒ same identity to every hub.

This is "one device per account" — pasting a phrase doesn't sync; it
*replaces* the device's identity. Both devices having the same phrase
means both have the same key, with no coordination between them.

## Why no master key (yet)

Discussed but **not implemented**. A future model could treat the
recovery phrase as a master seed and derive subkeys (per-device, per-app)
via HKDF. That layers on backward-compatibly: existing single-key
identities can be migrated by treating the phrase as both seed and direct
secret. We chose to ship the simple model first. See [decisions.md](decisions.md).

## Anti-spam (future, not shipped)

Proof-of-work knobs live in `shared/voxply-identity/src/pow.rs`. Idea: a
hub admin sets a PoW level for joins / messages. Real solution still
deferred — see ROADMAP.
