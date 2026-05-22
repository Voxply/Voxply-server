# Architecture

Voxply is three crates plus one TypeScript client, all in one repo.

## The three layers

```
shared/   ── pure logic, no I/O, used by both client and server
server/   ── hub: long-running daemon, holds state for one community
client/   ── desktop app: connects to one or many hubs
```

### `shared/voxply-identity`

Ed25519 keypairs, BIP39 recovery phrases, proof-of-work helpers. No
networking, no storage. Both the hub and the desktop client depend on it
so that signing and verification use the exact same code.

- Lib entry: `shared/voxply-identity/src/lib.rs`
- Recovery phrases: `shared/voxply-identity/src/recovery.rs`
- PoW helpers (anti-spam, future): `shared/voxply-identity/src/pow.rs`

### `shared/voxply-voice`

Audio pipeline: capture → denoise (RNNoise) → encode (Opus) → transport
→ decode → playback. Used by the desktop client and (in some flows) the
hub voice relay.

- Pipeline orchestration: `shared/voxply-voice/src/pipeline.rs`
- Codec: `shared/voxply-voice/src/codec.rs`
- UDP transport: `shared/voxply-voice/src/transport.rs`
- Wire protocol: `shared/voxply-voice/src/protocol.rs`

See [voice.md](voice.md) for the full data flow.

### `server/voxply-hub`

A single hub. Owns:
- An axum HTTP+WebSocket API (port 3000 by default).
- A UDP voice relay (port 3001 by default).
- A SQLite database (`hub.db`).
- An outbox worker for federated DMs (`dm_worker.rs`).
- A federation client for talking to other hubs.

Entry: `server/voxply-hub/src/main.rs` → `server.rs` (router setup).

Key submodules:
- `auth/` — challenge-response signature auth (see [identity.md](identity.md))
- `routes/` — every HTTP endpoint, one file per resource
- `federation/` — hub-to-hub HTTP client + handlers
- `db/migrations.rs` — schema (see [data-model.md](data-model.md))

### `client/voxply-desktop`

Tauri 2 (Rust shell) + React 19 (UI). The Rust side handles file I/O,
voice, and OS integration; the React side is everything you see.

- React entry: `client/voxply-desktop/src/main.tsx` → `App.tsx`
- Tauri commands (Rust ↔ JS bridge): `client/voxply-desktop/src-tauri/src/lib.rs`

See [client.md](client.md) for the structure.

## Federation, briefly

Hubs are independent. They peer over HTTPS + WebSocket using their own
Ed25519 keypairs as identity. There's no central directory — you connect
to a hub by URL. Federation enables:

- **DMs across hubs** — sender's hub queues to recipient's hub via outbox.
- **Alliances** — named groups of peer hubs sharing channels and reactions.

See [federation.md](federation.md) for the protocol and
[alliances.md](alliances.md) for alliances.

## Why this shape

- **Hubs over a central server**: communities own their data and their
  moderation policy. Federation lets them stay connected without a single
  operator. (See [decisions.md](decisions.md).)
- **Shared crates**: identity and voice rules must agree exactly between
  hub and client; one crate prevents drift.
- **Tauri over Electron**: smaller binaries, native voice, real OS APIs.
