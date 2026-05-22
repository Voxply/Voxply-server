# Voxply Docs

A navigable map of Voxply for humans and LLMs. Optimized for **why** and
**where**, not **what** — code is authoritative for what; this wiki tells
you the rationale and points you to the right files.

## Reading order

If you're new, read in this order:

1. [architecture.md](architecture.md) — what runs where, the three crates
2. [identity.md](identity.md) — keypairs, recovery, auth
3. [federation.md](federation.md) — how hubs talk to each other
4. [alliances.md](alliances.md) — multi-hub groups (Voxply's differentiator)
5. [voice.md](voice.md) — Opus + UDP relay + denoise pipeline
6. [data-model.md](data-model.md) — DB schema map
7. [client.md](client.md) — Tauri + React desktop client
8. [decisions.md](decisions.md) — design rationale (why federated, why no central server, etc.)
9. [threat-model.md](threat-model.md) — what we defend against, what we don't
10. [glossary.md](glossary.md) — terms

### Running a hub

- [hosting.md](hosting.md) — practical guide: systemd, TLS, backups, upgrades
- [packaging.md](packaging.md) — cross-platform packaging, code signing, auto-update, CI/CD, hub Docker image

### Onboarding & anti-abuse

- [lobby-bot-survey.md](lobby-bot-survey.md) — security-level lobby, "not a bot" challenge, role questionnaire (designed, not built)

### Future direction (designed, not built)

11. [farm-model.md](farm-model.md) — multi-hub server layer + 5-layer architecture
12. [gaming.md](gaming.md) — game distribution platform + tier progression
13. [home-hub.md](home-hub.md) — personal-axis state: home hub list, replication, DM canonicalization
14. [multi-device.md](multi-device.md) — master+subkey identity, QR pairing protocol
15. [e2e-encryption.md](e2e-encryption.md) — E2E encrypted DMs: X25519 from Ed25519 seed, static ECDH + AES-GCM, signed envelopes
16. [future-features.md](future-features.md) — anti-spam, moderation, recovery, bots
17. [browser-client.md](browser-client.md) — second client (no Tauri), platform adapter, IndexedDB identity, voice deferred
18. [android-client.md](android-client.md) — Tauri 2 Android wrapper around the browser platform layer, side-loaded APK
19. [install-android.md](install-android.md) — end-user guide: enable unknown sources, download APK, Play Protect warning
20. [brand.md](brand.md) — motto, one-liner, logo brief and asset checklist

## Find by feature

Reading order is for learning the system end-to-end. This section is for
"I know what I'm looking for" lookups. A feature can span multiple docs.

### Identity & access
- **Keypair, recovery phrase, auth** — [identity.md](identity.md)
- **Roles & permissions** — [data-model.md](data-model.md), [decisions.md](decisions.md)
- **Moderation (ban / mute / timeout / kick, approval queue)** — [data-model.md](data-model.md)
- **Local block / ignore (per device)** — [client.md](client.md)

### Messaging
- **Text channels & categories** — [data-model.md](data-model.md), [client.md](client.md)
- **Drag-drop channel/category reorder** — [client.md](client.md)
- **Markdown, code blocks, /me actions** — [client.md](client.md)
- **Reactions (local + federated)** — [data-model.md](data-model.md), [federation.md](federation.md)
- **Replies / threading** — [data-model.md](data-model.md)
- **Mentions** — [data-model.md](data-model.md)
- **Edit / delete messages** — [data-model.md](data-model.md)
- **Attachments (3 MB)** — [data-model.md](data-model.md)
- **Search per channel** — [data-model.md](data-model.md)
- **Typing indicators (channel + DM)** — [client.md](client.md)
- **Pin / unpin channels** — [client.md](client.md)
- **Direct messages (federated outbox)** — [federation.md](federation.md), [data-model.md](data-model.md)
- **E2E encrypted DMs (designed, not built)** — [e2e-encryption.md](e2e-encryption.md)
- **Friends (local + cross-hub via stored hub URL)** — [federation.md](federation.md)

### Voice (in any channel — every channel is unified text + voice)
- **Opus codec + UDP relay** — [voice.md](voice.md)
- **RNNoise denoise + VAD** — [voice.md](voice.md)
- **Push-to-talk** — [voice.md](voice.md)
- **Self-mute / self-deafen** — [voice.md](voice.md)
- **Voice participant list in sidebar** — [client.md](client.md)
- **Screen share + webcam (designed, not built)** — [screen-share.md](screen-share.md)

### Federation
- **Hub-to-hub auth** — [identity.md](identity.md), [federation.md](federation.md)
- **Alliances (multi-hub groups)** — [alliances.md](alliances.md)
- **Shared channels across alliance** — [alliances.md](alliances.md)
- **Federated DMs (outbox model)** — [federation.md](federation.md)
- **Federated reactions on alliance reads** — [federation.md](federation.md)

### Notifications & UI
- **Three-state notifications (all / mentions / silent)** — [data-model.md](data-model.md), [client.md](client.md), [decisions.md](decisions.md)
- **System tray + OS notifications + sound** — [client.md](client.md)
- **Window title unread count** — [client.md](client.md)
- **Themes (Calm / Classic / Linear / Light)** — [client.md](client.md)
- **Quick channel switcher (Ctrl+K)** — [client.md](client.md)
- **Hub drag-drop reorder, /info preview, clear local data** — [client.md](client.md)

### Future direction (designed, not built)
- **Anti-spam (PoW + hub certifications)** — [future-features.md](future-features.md)
- **Channel ban, voice mute, talk power** — [future-features.md](future-features.md)
- **Identity recovery (device linking, recovery contacts)** — [future-features.md](future-features.md)
- **Bots & integrations** — [future-features.md](future-features.md)
- **Nested channels + max depth hub setting** — [future-features.md](future-features.md)
- **Forum channel type (post-list variant)** — [future-features.md](future-features.md)
- **Server tags (federated portable badges)** — [future-features.md](future-features.md)
- **Home hub list (personal-axis state, DM canonicalization)** — [home-hub.md](home-hub.md)
- **Multi-device pairing (QR, master+subkey)** — [multi-device.md](multi-device.md)
- **Farm model (multi-hub server, SSO, discovery)** — [farm-model.md](farm-model.md)
- **Gaming platform (tiers, SDK, sandbox)** — [gaming.md](gaming.md)

## How to use this wiki

- **For LLMs**: each file is self-contained and small enough to read whole.
  File:line pointers (`server/voxply-hub/src/routes/messages.rs:42`) lead to
  authoritative code. Don't copy code from the wiki — read the source.
- **For humans**: same, but you can also follow the markdown cross-links.

## How to maintain this wiki

- **Add a "why" before a "what"**. If something is obvious from the code
  (a function name, a type signature), don't repeat it here.
- **File:line pointers, not code copies**. Code rots; pointers force you
  to look at current source.
- **Update on intent change, not on code change**. If the *reason* a thing
  exists changes, update the wiki. Renaming a function? Don't bother.
- **Keep files under ~200 lines**. Split when they grow past that.

## Related docs

- [`../ROADMAP.md`](../ROADMAP.md) — what's next, known issues, undesigned wishlist
- [`../README.md`](../README.md) — public-facing project intro
