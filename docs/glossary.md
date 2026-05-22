# Glossary

Voxply terms, alphabetized.

**Alliance** — a named group of peer hubs sharing channels, reactions,
and (future) voice/games. Voxply's differentiator. See
[alliances.md](alliances.md).

**Channel** — a place in a hub. Every channel is **unified text + voice**:
chat history persists, and users can join voice in the same room.
Categories (`is_category=1` channels) are containers that hold other
channels but no messages. Channels can be shared with an alliance.

**Federation** — the umbrella term for hub-to-hub communication. Today
covers federated DMs and alliance message sharing. See
[federation.md](federation.md).

**Hub** — a single Voxply server instance. One community lives in one
hub. Hubs are independent — no central authority.

**Hub farm** (future) — one operator running many hubs on shared
infrastructure with quotas and a farm-internal directory. Not built.

**Identity** — an Ed25519 keypair. Your identity is your public key. See
[identity.md](identity.md).

**Invite token** — a signed blob a hub admin generates to grant another
hub permission to join an alliance. Delivered out of band.

**Outbox** — the `dm_outbox` table holding pending federated DMs. The
DM worker drains it.

**Peer** / **peer hub** — another hub this hub knows about (URL +
pubkey). Stored in `peer_hubs`.

**Recovery phrase** — 24 BIP39 words that deterministically yield an
Ed25519 keypair. The phrase **is** the identity; pasting it on a device
replaces that device's identity.

**Role** — a bundle of permissions assigned to users. Custom per hub.
Roles have priority for moderation hierarchy.

**Three-state notifications** — per-scope notification mode: all /
mentions only / silent. The scope is a channel, DM, or hub.

**Voice (in a channel)** — Voxply has no "voice channel" type. Every
channel is both text and voice; "in voice" means a user has joined the
real-time audio room of a channel. The hub's UDP relay (port 3001 by
default) is an SFU-style fan-out. See [voice.md](voice.md).
