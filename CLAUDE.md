# CLAUDE.md

Guidance for Claude Code (claude.ai/code) working in this repository.

## What this repo is

The **Wavvon server** — a Rust workspace for a self-hosted, federated voice+text
community platform. This repo is self-contained: you can clone, build, test and
run a hub from it alone.

Sibling repos (you don't need them checked out):

| Repo | Contents |
|---|---|
| [Wavvon-clients](https://github.com/Wavvon/Wavvon-clients) | Web + desktop clients, shared TS packages, voice crate |
| [Wavvon-discovery](https://github.com/Wavvon/Wavvon-discovery) | Optional public hub directory site |
| [Wavvon-docs](https://github.com/Wavvon/Wavvon-docs) | Architecture wiki (70+ docs) + `openapi.yaml` |

**Read the wiki before grepping.** Start at
[docs/README.md](https://github.com/Wavvon/Wavvon-docs/blob/main/docs/README.md)
for the reading order. Wiki links below point at that repo; clone it alongside
this one if you want them offline.

Commit to **`develop`**. See `CONTRIBUTING.md`.

---

## Commands

```bash
cargo check --workspace
cargo check --all-targets --workspace    # `check` alone does NOT compile tests
cargo clippy --all-targets --workspace
cargo fmt --all                          # required before every commit; CI gates on --check
cargo test --workspace -- --test-threads=4   # bounded: the default saturates Postgres
                                             # max_connections and flakes with PoolTimedOut
cargo test -p wavvon-hub                 # single crate
cargo build --release
```

Hub CLI (binary at `target/release/wavvon-hub`):

```bash
./target/release/wavvon-hub migrate      # apply DB migrations
./target/release/wavvon-hub doctor       # pre-flight checks (ports, TLS, disk)
./target/release/wavvon-hub admin stats
./target/release/wavvon-hub admin users set-owner <pubkey>   # bootstrap ownership
./target/release/wavvon-hub backup FILE
./target/release/wavvon-hub restore FILE
./target/release/wavvon-hub rotate-key
```

Farm and seed have equivalent `migrate` subcommands. All binaries are configured
via `WAVVON_*` environment variables or a config file — see the
[hub operator guide](https://github.com/Wavvon/Wavvon-docs/blob/main/docs/hub-operator-guide.md).
To get a hub running locally, use the **`run-hub`** skill in `.claude/skills/`.

---

## Architecture

### `hub` — the community server

Entry: `crates/hub/src/main.rs` -> `lib.rs` -> `server.rs`

Axum HTTP + WebSocket (port 3000), WebTransport/QUIC voice relay over UDP 3001
(voice transport v2 — the raw-UDP and `/voice/ws` relays it replaced are gone),
sqlx + Tantivy FTS. Handles: channels (text + voice + forum + banner + spawner),
messages, polls/events (incl. organizer staging), roles + role categories,
moderation, E2E DMs, federation (outbox DMs, alliances, federated ban lists),
bots/webhooks, soundboard, recovery-contact attestation, and background workers
for DM outbox delivery, ban-list sync, data retention + rotation-request expiry,
and cert maintenance.

Layout under `crates/hub/src/`:
- `routes/` — one file per HTTP resource group
- `auth/` — Ed25519 challenge-response
- `federation/` — hub-to-hub HTTPS + WebSocket
- `state.rs` — `AppState`
- `permissions.rs` — role-based access control
- `capabilities.rs` — the feature-string list served by `GET /info`
- `db/migrations.rs` — schema (additive only)
- `db/version.rs` — minimum supported PostgreSQL server, checked before migrations

### `identity` — wire-format authority

Entry: `crates/identity/src/lib.rs`. Pure Rust, no async, no I/O.

Defines every signed envelope type: `HomeHubList`, `SubkeyCert`, `PairingOffer`,
DM envelopes, prefs blob, recovery request/attestation
(`wavvon/recovery-request/v1`, `wavvon/recovery-attestation/v1`), and more. Each
has a versioned tag (`b"wavvon/<name>/v1\0"`) and length-prefixed binary
encoding. Also: BIP39 recovery phrases (24 words <-> secret key), X25519 ECDH
from an Ed25519 seed for E2E DMs, AES-256-GCM, PoW helpers.

**Every client must match this crate byte-for-byte.** Test vectors live in
`crates/identity/tests/wire_vectors.rs` and are mirrored in the clients repo
(TypeScript and desktop Rust), each asserting the same vectors. Changing an
envelope is a cross-repo operation — use the **`wire-format-change`** skill.

### `store` — database abstraction

Entry: `crates/store/src/lib.rs`. Trait-based: `HubStore` = `AuthStore +
UserStore + ChannelStore + MessageStore + RoleStore + DmStore + FederationStore +
BotStore + ...`. The hub holds `Arc<dyn HubStore>`. PostgreSQL is the one and
only backend (`crates/store/src/impls/`). The trait split's value today is error
normalization and keeping SQL out of route handlers — prefer it over raw
`sqlx::query` in new hub code.

### `hub-env` — cross-process config keys

Entry: `crates/hub-env/src/lib.rs`. Dependency-free; holds only the **names** of
the `WAVVON_*` env keys that cross a process boundary, shared by hub, farm and
agent.

It exists because those names were string literals on both sides and drifted:
farm and agent spent months setting `WAVVON_HUB_DB` and `WAVVON_HUB_HTTP_PORT`,
names the hub has never read, so spawned hubs ignored their allocated port and
shared one database — silently. Never spell such a name as a literal.
`wavvon_hub::settings` has a test that sets every spawnable key and asserts it
reaches `Settings`.

### The rest

- **`farm`** — fleet control plane: hub lifecycle (spawn, monitor, stop), server registration, reverse-proxy to hub processes, farm-level SSO. Partially implemented; see the wiki's `farm-model.md` and `farm-impl.md`.
- **`agent`** — fleet worker node. Reverse-connects to farm over WebSocket, spawns and monitors local hub processes on its behalf. No HTTP surface.
- **`seed`** — cross-farm discovery registry. Farms publish signed self-listings; the discovery site queries it for the global hub catalog.
- **`demo-seed`** — populates a running hub with realistic demo data for screenshots.
- **`bot-kit`, `ttt-bot`, `discord-import`** — bot SDK, example bot, importer.

---

## Non-obvious constraints

**Migrations are additive only.** `crates/hub/src/db/migrations.rs` uses only
`CREATE TABLE IF NOT EXISTS` and `ALTER TABLE ADD COLUMN`. Never `DROP`,
`TRUNCATE`, or destructive schema changes. Wrap column additions so an "already
exists" error is silently ignored.

**PostgreSQL: one backend, a declared floor, a configurable pool.** PostgreSQL is
the only backend, and reopening that needs a new entry in the wiki's
`decisions.md` — the reasoning is recorded there. The hub declares a **minimum
server version** (`db/version.rs`, currently PG 14) and checks it *before*
migrations, so an old server gets a sentence instead of a half-applied schema; CI
runs the matrix against both ends of the supported range. Pool size is
`WAVVON_DB_MAX_CONNECTIONS` (default 5) on hub and farm — it caps concurrent
*queries*, not concurrent users, and the sum across hubs sharing one server must
stay under its `max_connections`.

**List endpoints paginate with one dialect:** an array plus `limit` and a keyset
cursor. No envelope, no offset paging, no second shape. A paginated endpoint also
needs a client that *pages* — raising a cap without one just moves the silent
truncation to a bigger number.

**Adding a hub feature a client branches on? Add its capability string.**
`GET /info` carries `capabilities`, and clients decide what to render by testing
membership — never by comparing `version`. One sorted list in
`crates/hub/src/capabilities.rs`; add the line in the same commit that adds the
feature. This bites here more than elsewhere because each hub serves its own
baked-in web client and that client is multi-hub — the copy served by hub A talks
to hubs B and C, so there is no "client and server update together". Strings
never change spelling once published (that's a removal, and removals wait for a
major).

**Duplicate endpoints get folded, not versioned side by side.** Two route
families over one table means two half-specified behaviours that disagree —
`/moderation/channels/{id}/bans` and `/channels/{id}/bans` did exactly that and
silently erased ban reasons. `/channels/{id}/bans` is now the only channel-ban
API; `/channels/{id}/members` was deleted (it ignored its `channel_id` and
returned the `/users` rows).

**Silent fallthroughs are the bug class to watch for here.** Duplicated env-var
name literals let farm/agent configure hubs with keys the hub never read, and a
client's WebSocket enum matched unknown events as `Other => {}`, so four hub
features were simply absent with no symptom. Both cost months. When you add a
catch-all arm or a cross-process string, make the unknown case say something.

**Recovery/attestation signing uses the identity key the hub knows the user by**
(the roster pubkey), NOT a derived multi-device master key — contacts are
designated and requests approved against hub-known pubkeys. The master key signs
only multi-device material (subkey certs, etc.).

**Two-axis state model.** Community-axis state (channels, messages, roles) lives
on community hubs. Personal-axis state (prefs, DM history, block/mute/ignore,
home hub list, custom themes, drafts) lives on the user's home hub(s). Don't mix
them.

**Identity is a keypair, not an account.** No email, password, or username.
Identity = Ed25519 public key (hex). Multi-device via BIP39 master phrase +
signed subkey certs. Auth accepts an optional subkey cert — see
`resolve_canonical_identity` in `crates/hub/src/auth/handlers.rs`; don't bypass
it in endpoints that key off user identity.

---

## Tests

Integration tests in `crates/hub/tests/*_flow.rs` use `axum_test::TestServer`
against a **fresh, isolated PostgreSQL database per test**
(`common::create_test_db()` creates it via `PgPoolOptions` and runs the real
migrations). Set `TEST_DATABASE_URL` if your server isn't
`postgres://postgres:postgres@localhost:5432`. Locally,
`docker run -d -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:16-alpine`
suffices.

New endpoints need at minimum a happy-path and a rejection test.

A `PoolTimedOut` failure is almost always `max_connections` exhaustion under
parallelism, not a real fault — re-run the single binary, or cap with
`-- --test-threads=4`.

**A test that can skip itself must be unable to skip in CI.** Print
`WAVVON-TEST-SKIPPED:` and CI fails on it (`build.yml`); `cargo test` runs with
`--nocapture` there because it otherwise swallows the output of passing tests,
and the guard would grep a log that cannot contain what it looks for. Skips are
legitimate — `pg_dump` and a built `wavvon-hub` are not on every dev box — but a
green run that did not execute the test is worse than no test. A backup test
"passing" in 0.00s is what that looks like.

**A new config variant needs a test that executes it, not one that tests its
inputs.** `hub_isolation = 'schema'` shipped with unit tests over the URL it
builds and nothing that had ever started a hub behind one; the failure mode was
migrating into `public` and silently sharing it again. Enumerable config means
one end-to-end test per value.

---

## Conventions

- Code comments in **English**, and only when the WHY is non-obvious. Don't explain WHAT.
- No comments in GitHub Actions workflow files — explain the choice in the commit message or the docs.
- Design decisions go in the wiki's `decisions.md` (newest entry at the top: decision / alternatives / tradeoff / outcome). Mark superseded entries; don't delete them.
- Competitor references are allowed — factual, no logos, no disparagement.
