# Changelog

All notable changes to Voxply are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning follows [Semantic Versioning](https://semver.org/). `v0.x.y`:
minor bumps signal breaking wire-protocol changes; patch bumps are compatible.

## [Unreleased]

### Added
- Cross-platform packaging: NSIS installer (Windows), universal DMG (macOS), AppImage (Linux)
- Auto-update via `tauri-plugin-updater` with Ed25519-signed update payloads
- GitHub Actions CI: build-check on PR, full release build on `git tag v*`
- Hub Docker image (distroless) and `docker-compose.yml` for self-hosters
- E2E encrypted 1:1 DMs: AES-256-GCM with X25519 key agreement and Ed25519-signed envelopes
- Screen share: hub-relayed WebM chunks, source picker, viewer panel, optional webcam
- Hub discovery layer 3: signed public hub profiles (`GET/PUT /profile/:pubkey`)
- Multi-device pairing via QR code, SubkeyCert chain, and master identity
- Federation: cross-hub messaging, DM routing, and hub-to-hub authentication
- Voice channels: Opus audio, VAD, push-to-talk, per-channel role permission
- Roles & permissions system with built-in @everyone and Owner roles
- Hub moderation: bans, mutes, channel bans, voice mutes
- In-hub games via iframe manifests
- Hub alliances for cross-hub channel sharing

## [0.1.0] - 2026-05-01

Initial development build. Core hub+client architecture established.
