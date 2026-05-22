# Voxply

A decentralized platform where players can hang out, talk, and play
together. Voice chat, text messaging, federated alliances of hubs, and
community-built games — all keypair-based identity, no central servers.

## Features

- **Channels** — every channel is **unified text + voice**: chat
  history and voice in the same room. Categories nest channels (and
  other categories). Drag-drop reorder, collapse/pin, search, markdown,
  code blocks, attachments (3 MB), reactions, replies, mentions, /me
  actions, edit/delete.
- **Voice** — Opus over UDP with RNNoise denoise, voice activity
  detection, push-to-talk, self-mute / self-deafen.
- **Direct messages** — federated outbox with retry, attachments,
  typing indicator, unread tracking, sort by activity.
- **Alliances** — multi-hub groups. Hubs share channels, messages, and
  reactions across the alliance via federation.
- **Hub federation** — independent hubs peer over HTTPS + WebSocket.
- **Identity** — Ed25519 keypair, 24-word BIP39 recovery phrase, no
  accounts, no passwords.
- **Roles & moderation** — custom roles with priority and permissions,
  ban / mute / timeout / kick, channel ban, voice mute, talk power, hub
  approval queue.
- **Notifications** — three-state per scope (all / mentions only /
  silent), system tray badge, OS notifications, mention sound.
- **Themes** — Calm (default), Classic, Linear, Light.

## Architecture

```
server/
├── voxply-hub/          Hub server (axum + SQLite + WebSocket + UDP voice)
└── voxply-seed/         Discovery scaffold (not in active use)

client/
└── voxply-desktop/      Tauri + React desktop client

shared/
├── voxply-identity/     Ed25519 keypairs, signing, recovery phrases
└── voxply-voice/        Audio pipeline (cpal + Opus + RNNoise)
```

## Quick start

### Hub server

```bash
cargo run -p voxply-hub
# Listens on http://0.0.0.0:3000 (HTTP) and 0.0.0.0:3001 (voice UDP).
# Override with VOXPLY_HTTP_PORT / VOXPLY_VOICE_UDP_PORT.
# Set VOXPLY_TLS_CERT and VOXPLY_TLS_KEY for HTTPS.
```

For a real production deployment (systemd unit, TLS, backups, upgrades),
see [`docs/hosting.md`](docs/hosting.md).

### Desktop client

```bash
cd client/voxply-desktop
npm install
npm run tauri dev
```

The window opens with an "Add a hub" prompt; paste your hub URL
(`http://localhost:3000` for a local dev hub) to connect.

## Building

```bash
cargo build                       # all Rust crates
cargo test                        # hub + shared crate tests
cd client/voxply-desktop && npm run tauri build   # desktop release
```

## Documentation

- [`docs/`](docs/README.md) — architecture, federation, identity,
  alliances, voice, data model, client structure, decisions, threat
  model, and glossary. Start at [`docs/README.md`](docs/README.md).
- [`ROADMAP.md`](ROADMAP.md) — what's next, known issues, undesigned
  wishlist, and explicit "won't do" decisions.

## Built with AI assistance

This project was built with substantial help from
[Claude](https://claude.ai) (Anthropic's AI assistant). I direct the
product, architectural choices, and tradeoffs; Claude drafts most of
the code, tests, and documentation, which I then review and accept,
adjust, or rewrite.

Calling this out for transparency — it's not a fully hand-written
project, and pretending otherwise wouldn't be honest.

The wiki at [`docs/`](docs/README.md) is intentionally LLM-friendly
(file:line pointers, navigable index, "why" over "what") so Claude
stays useful as the codebase grows.

## License

[GNU Affero General Public License v3.0](LICENSE). Network use of a
modified version requires offering the corresponding source to those
users — important for a federated platform like this.
