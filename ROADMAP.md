# Voxply Roadmap

Tracks **what's next, what's broken, and what we'd like to build but
haven't designed yet**. Everything else — architecture, design rationale,
shipped features, design questions — lives in the wiki at
[`docs/`](docs/README.md).

## 🔨 Next up

_(nothing queued — see wishlist below)_

## 🚧 Blocked

- **Android client icons** — placeholder solid-color PNGs in place. Waiting on logo (see [`docs/brand.md`](docs/brand.md)). Run `cargo tauri icon <1024x1024.png>` once ready.

## 📌 Wishlist (undesigned)

Things we want to build but haven't committed to a design yet. Designed
items live in the wiki — see
[`future-features.md`](docs/future-features.md),
[`farm-model.md`](docs/farm-model.md),
[`gaming.md`](docs/gaming.md).

- **External bots** — bot developers build and host their own service with its own Ed25519 keypair; bot self-declares `is_bot: true` at auth time and can join multiple hubs. Internal service accounts already shipped; this adds the third-party ecosystem layer (bot directory, invite-by-pubkey flow, slash commands)
- **Performance ceiling** — load test WS broadcast, search, voice relay
- **Accessibility + i18n** — keyboard nav audit, screen-reader, localization

## ⚠️ Known issues

_(none currently)_

## 💤 Won't do

- **Load-aware DM routing across a user's hubs** — failover only; load-balancing needs gossip + cross-hub consistency. See [decisions.md](docs/decisions.md)
- **Concurrent mic test while in voice** — two cpal input streams unreliable cross-platform; live meter covers it
