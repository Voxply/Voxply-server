# Farm Model (future)

Voxply today is 1:1 — one `voxply-hub` process hosts exactly one hub.
The **farm model** is a planned future layer that lets one server host
many hubs.

> Not built. This page captures the design so we don't paint ourselves
> into a corner with current work.

## Three terms

- **Farm** — one server process operated by one person/organization.
- **Hub** — an independent community living on a farm (today's "hub").
- **Channels / users / messages** — inside a hub (current model).

The farm is a new layer **above** hub. A single farm can host many hubs.

## Why a farm layer

- Self-hosters can run "communities for my friends" without a separate
  server per group.
- Farm operators set policy: max hubs per user, who can create hubs,
  whether the directory is public.
- Farm-internal hub directory = discovery without a global registry.

## The five-layer mental model

Bottom to top:

1. **Identity** — Ed25519 keypair (today)
2. **Hub** — a community: channels, users, voice (today)
3. **Hub federation** — peers, alliances, federated DMs (today, partial)
4. **Farm** — one server hosting many hubs, operator-set policy (future)
5. **Farm clusters / cross-farm discovery** — operator-claimed clusters
   + open-network discovery via the seed crate (future)

Each layer is a separate concern. Don't conflate them in conversation:

| Phrase                                       | Layer |
|----------------------------------------------|-------|
| "Federation" (today)                         | 3     |
| "I want one server for many communities"     | 4     |
| "I run 3 servers; group them together"       | 5     |
| "How do users find hubs they don't know yet?"| 5     |

## Identity stays public-key, full stop

The farm model does NOT turn users into farm-accounts. The pubkey
remains the canonical identity:

- Farm tokens are session credentials, scoped to that farm. Not identity.
- The same pubkey works on every farm.
- DMs are addressed `(pubkey, farm_url)` — pubkey says **who**, farm URL
  says **where to currently route to**.
- Friends, recovery phrase, federation — all keyed on pubkey.

We will NOT add: per-farm signup with email/password, a central "Voxply
Account" service, addresses like `name@farm.com`, farm tokens not tied
to a user signature.

The farm is **hosting + SSO + inbox.** Not identity.

## Farm-level SSO

Auth moves from per-hub to per-farm:

- Farm exposes `/auth/challenge` and `/auth/verify`.
- User keypair signs a farm-issued challenge → farm-issued session token.
- Hubs verify the **farm's signature** on the token. No re-auth per hub.
- Security level proof (PoW, see [future-features.md](future-features.md))
  also lives at the farm — prove once, applies to every hub.
- Each hub still owns its own user record (roles, per-hub display name,
  bans). Auth is factored out; per-hub state stays.

Implication: hubs are no longer self-contained crypto islands. The farm
is the trust root. Migrating a hub between farms requires explicit
export/import; sessions don't survive the move.

## Discovery: four vectors, all opt-in

1. **Direct URL** — friend invites, links shared anywhere. Voxply
   itself never ships a global directory.
2. **Farm directory** — farm operator decides whether the farm publishes
   a `/hubs` listing.
3. **Hub visibility flag** — hub admin decides whether the hub appears
   in the farm directory.
4. **User-curated** — public profile at `GET /profile/<pubkey>` lists
   the user's chosen hubs, signed by their key. Per-hub opt-in. Strongest
   real-world growth vector — social, self-policing, spam-resistant.

Visibility controls listing, not access. A hub hidden from the farm
directory is still reachable by direct URL.

A hub is discoverable only if the farm AND the hub admin both opt in
(for the directory path), or if a user puts it on their profile (for the
social path).

## Future: DMs and games at the farm level

When farms exist, both move up a layer:

- **DMs**: per-farm inbox, not per-hub. `dm_messages` / `dm_outbox`
  become farm-scoped. The retry worker is one farm-level supervisor
  instead of one per hub. See [federation.md](federation.md).
- **Games**: catalog, files, matchmaking, persistent state all live on
  the farm. Hubs opt in. See [gaming.md](gaming.md).

## Generic job queue (future refactor)

Today's `dm_worker.rs` is DM-specific. Once we have farms with multiple
async cross-farm operations (DMs, game invites, friend-request
federation, profile broadcasts), refactor into a generic `queue_worker`
with pluggable handlers per job kind:

```
queue_jobs(id, kind, payload_json, attempts, next_attempt_at, last_error, bounced_at)
register_handler("dm_delivery", deliver_dm)
register_handler("game_invite", deliver_game_invite)
```

One backoff/retry/dead-letter implementation, many job kinds.

## Implementation order (when the time comes)

Multi-month roadmap. Don't start without explicit direction:

1. Farm-level auth — move `/auth/*` to farm, issue verifiable tokens
2. Hub multi-tenancy — many hubs in one farm process
3. Public/private flag on hubs + farm `/hubs` listing endpoint
4. Client model update — connect to farms, browse hubs per farm
5. Hub creation API + per-creator quotas
6. Hub migration export/import
7. `/info` enhancements + deep links (`voxply://farm/hub/...`) for
   third-party indexers

Right time to start: when the user has 2+ hubs themselves OR a real
user complains about running multiple `voxply-hub` processes.

## How to apply

When a discussion involves "many hubs on one machine," "server hosting
hubs," or hub directories, this is the farm model. Don't confuse it with:

- **Hub federation** (layer 3 — hubs on different machines talking)
- **The seed crate** (layer 5 — cross-farm discovery)
