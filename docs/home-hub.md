# Home Hubs — Personal-Axis State

**Status**: design — no code yet. This doc supersedes the
"Client connects directly to many hubs" stance in
[decisions.md](decisions.md) for personal-axis state. Community traffic
(channel messages, voice, alliances) is unchanged: clients still
connect to community hubs directly.

## What it is

A user designates **one or more home hubs** — an ordered list of hubs
that hold state about *the user* (devices, prefs, DMs, friends).
Community hubs hold state about *a community* (channels, members,
voice, alliances). Home hubs do not proxy community traffic; the
client still talks to each community hub directly.

The list exists for redundancy. With one home hub, the user's personal
state has a single point of failure. With N hubs in the list, any one
of them can serve reads, and writes go to all reachable members so
the others catch up. The list is master-signed, so consumers can
verify it without trusting any individual hub.

| Axis | What's there | Where it lives |
|---|---|---|
| Community | Channels, messages-in-channels, voice, members, roles, alliances | The community's hub. Direct client→hub connection. |
| Personal | Device list, prefs, DMs, friends, notification settings | The user's home hub list. User-chosen, replicated across N hubs. |

## Why we changed direction

The original "no home hub" call (see the superseded entry in
[decisions.md](decisions.md)) rejected a home hub on the grounds that
it would reintroduce a primary-hub UX problem. That reasoning still
holds *for community traffic*. But it forced personal-axis state into
bad shapes:

- **Multi-device** had no place to publish a device list or
  revocations. Every community hub would have needed its own copy,
  drifting silently.
- **DM delivery** sprayed messages across all the user's hubs with
  failover, which works for resilience but means no canonical inbox
  view — devices logged into different hubs see different DM lists.
- **Prefs and friends** were per-device or per-hub, with no place to
  unify them.

A home hub list is the realistic anchor for all of that. The "primary
hub UX problem" the original decision feared is mitigated by making
the list pluralized: there is no single primary, the user picks a few
hubs they trust, and the system tolerates any of them being down.

The shape of the change: home hubs are on the **control plane** for
personal state and the **data plane** for personal-axis bytes (DM
inboxes, prefs blobs, device certs). They are **not** on the data
plane for community bytes — those still flow direct.

## What lives on the home hub list

Replicated across every hub in the list. Writes go to all reachable
members; reads can hit any.

| Concern | Storage | Encryption |
|---|---|---|
| Device registry (subkey certs + revocations) | Plaintext, master-signed | Signatures only — entries are public-by-design |
| Prefs blob (hub list, voice settings, theme, blocked users) | Opaque ciphertext | AEAD with key derived from master seed |
| DM inbox (canonical, dedup'd by message id) | Plaintext today, E2E later (see [threat-model.md](threat-model.md)) | None today |
| Friend list (canonical, future) | Plaintext, master-signed | None |
| Notification settings, read cursors (future) | Inside the prefs blob | Inherits prefs encryption |
| Pairing-protocol short-lived state (offers, claims) | Plaintext, TTL'd | None — bound to one-shot tokens |

Identity-bearing entries (certs, revocations, friend list) are signed
by the **master** so any home hub can serve them without being
trusted to fabricate them. Content-bearing entries (prefs, DMs) are
either encrypted to the user or accepted as plaintext-on-trust until
E2E ships.

## What does NOT live on the home hub list

- Channel messages, members, permissions — community hub.
- Voice traffic — direct UDP between client and community hub.
- Alliances — community-hub-to-community-hub.
- Hub-to-hub federation auth — every hub still has its own keypair.
- Anything about communities the user is *not* a member of.

The rule: if it's about a *place*, community hub. If it's about a
*person*, home hub list.

## The home hub list — replication model

The list is **client-driven**. No hub-to-hub gossip is required.

### The designation

The master signs:

```
HomeHubList {
  master_pubkey: Ed25519PublicKey,
  hubs:          Vec<String>,    // ordered URLs, slot 0 = preferred
  issued_at:     u64,
  sequence:      u64,            // strictly increasing, defeats rollback
  signature:     Ed25519Signature,
}
```

Every home hub in the list stores the latest designation it has seen.
Devices and friend hubs that resolve "where do I write/read for user
X?" fetch the designation, verify the master signature, and use the
list. Higher `sequence` wins on conflicts.

### Writes

For master-signed entries (certs, revocations, designations,
friend-list entries):

- Client writes to every home hub in the list that's reachable, in
  parallel.
- A write succeeds when *at least one* hub accepts it. The client
  retries the others with backoff until they catch up.
- Consumers don't care which hub served the read — signatures
  self-authenticate.

For the encrypted prefs blob:

- Each blob is versioned (`blob_version: u64`, monotonically
  increasing per-write).
- Client writes the new version to every reachable home hub.
- Readers fetch from the first reachable hub; if the version looks
  stale relative to what the device last wrote, the device pulls
  from another and takes the highest version. Last-writer-wins is
  safe because only the user writes their own blob.

For DMs see [DM delivery](#dm-delivery--list-based-failover).

### Reads

Devices and other consumers prefer the **first reachable** hub in the
list (slot 0). Fall through on timeout or 5xx. The list ordering is
the user's preference, not a correctness requirement — any hub in
the list is authoritative.

### Eventual consistency

Devices act as the replication carrier. Every paired device, on each
connect to a home hub, ensures its own subkey cert is present on
that hub. Devices that learned about a new revocation from any home
hub re-publish it to others if missing. This makes the home hub list
eventually consistent through the user's own devices — which is the
same federated pattern the rest of Voxply uses (clients are the
orchestrators, hubs are the dumb stores).

### Adding and removing hubs

Adding or removing a home hub is the routine operation, not a special
"move home hub" event:

1. From any paired device, user edits their home hub list.
2. Master signs a new `HomeHubList` designation with `sequence + 1`.
3. Device pushes the new designation to every hub mentioned in either
   the old or the new list (so departing hubs know they're out and
   joining hubs know they're in).
4. New hubs in the list bootstrap by pulling state from existing
   hubs (or, if none are reachable, the user's devices re-upload).
5. Departing hubs may keep stale data; clients ignore them on the
   next read because they're not in the current designation.

There is no "promote secondary to primary" — every hub in the list is
equal-status. Reordering the list changes preference, not authority.

## DM delivery — list-based failover

Today's DM design ([decisions.md](decisions.md) "DM failover") gives a
sender an ordered list of delivery hubs. The home hub model **is**
that list — it just happens to also hold devices, prefs, etc.

1. Sender's hub tries each URL in the recipient's home hub list, in
   order, until one accepts the DM.
2. The accepting hub stores the DM in the canonical inbox table and
   forwards a copy to the other home hubs as soon as they're
   reachable. Dedupe is by stable `message_id`.
3. Devices fetch DM history from any hub in the list. Because all
   accepting hubs forward to peers, the inboxes converge.
4. If every home hub is unreachable, the sender's hub queues in its
   outbox (existing behavior in `dm_worker.rs`) and retries.

The mirror-forward step reuses `FederationClient::post_dm` with a
`mirror=true` flag so receiving hubs distinguish "I'm an inbox copy"
from "I'm the original recipient." A peer hub that sees a duplicate
`message_id` short-circuits.

## Picking, moving, self-hosting

The user picks the list at first launch after upgrade. Defaults
suggested by the client: the first 1-2 hubs in the user's hub list.
The user can edit any time.

**Self-hosting** stays the privacy-conscious answer. A user who wants
no third-party hub seeing personal-state metadata can run a
single-user hub purely as their sole home hub. With a list, they can
also run two single-user hubs in different locations for redundancy
and put both in their list — same shape as anyone else, just narrower
trust.

**Mixing trust levels**: nothing prevents a user from having a
trusted self-hosted hub at slot 0 and a community-run hub at slot 1
as a fallback. Reads prefer slot 0 (the trusted one), writes still go
to both. If slot 0 is down for a week, slot 1 still serves; when slot
0 returns, devices catch it up.

## Friend list (future)

Today friends are per-hub with optional `hub_url` for cross-hub
friends. With a home hub list, the canonical friend list moves there:

- Add/remove/accept happen against the home hub list (write-to-all).
- Each entry is master-signed.
- Cross-hub friend requests route through home hub lists: Alice's
  home hubs ask Bob's home hubs on Alice's behalf, signed by their
  respective masters.

This is **v3-or-later** — out of scope for the multi-device launch.
The current per-hub friend implementation keeps working until then.

## Threat model deltas

New surfaces vs. [threat-model.md](threat-model.md):

| Surface | Mitigation |
|---|---|
| A home hub sees device count, pairing/revocation timestamps, DM volume metadata | User-chosen, in a user-controlled list. Self-host one slot for the metadata-conscious user. Document clearly. |
| A hostile home hub withholds revocation entries | Other home hubs in the list still serve them. Clients cross-check across the list and alert on divergence. With N≥2, withholding is easy to detect. |
| A hostile home hub fabricates revocations or certs | Cannot — entries are master-signed. The hub can only choose what to serve, not what to forge. |
| A hostile home hub drops DMs | Other home hubs receive the mirror. A single hub silently dropping inbound DMs is detectable when the user reads from another slot. |
| Compromised home hub serves stale prefs blob | Acceptable — blob is encrypted, attacker can't read it. Version counter detects skew. |
| Designation rollback (replay an old `HomeHubList`) | `sequence` is strictly increasing; consumers reject lower sequences than the latest they've seen. |
| Whole list goes dark | Master can mint a new designation pointing anywhere. Fallback discovery (below) recovers. |

The list-of-N model does not prevent a coordinated hostile-quorum
attack (every home hub colludes), but it raises the bar from "trust
one hub" to "trust at least one hub in the list." For most users that's
the right tradeoff.

## Failure modes

- **One hub in the list is down**: transparent — others serve. Writes
  retry with backoff until that hub catches up.
- **All hubs in the list are down (transient)**: cached state on each
  device keeps working for community traffic. Pairing, prefs writes,
  and DM delivery to this user pause until at least one hub returns.
- **All hubs in the list are gone (permanent)**: any device with the
  phrase mints a fresh designation pointing anywhere; devices and
  friends learn about it through the propagation rules below.
- **Hostile single hub**: user removes it from the list (master
  signs a new designation; departing hub gets the new list to know
  it's out, even if it ignores it).

### Designation propagation

Devices and friends learn about a new `HomeHubList` through:

- **Paired devices**: each device polls every home hub it knows of
  for the latest designation; takes the one with the highest valid
  `sequence`.
- **Friend hubs**: when a friend's hub tries to deliver a DM and
  finds an outdated list, the destination hub returns
  `410 Gone, latest_designation: {...}` so the friend's hub
  refreshes.
- **Community hubs**: the user's client updates each community hub's
  record of "this user's home hubs" via an authenticated request on
  next connect.
- **Fallback list in subkey certs**: each cert can carry a small set
  of "designation-of-last-resort" hubs the user has authorized to
  hold the latest designation. Used only when the active list is
  fully unreachable.

## Files this will touch

Pointers, not code copies — wiki convention.

- `server/voxply-hub/src/routes/identity.rs` (new) — device registry,
  revocations, pairing endpoints, prefs blob, designation storage.
- `server/voxply-hub/src/routes/home_hub_dms.rs` (new) — canonical DM
  inbox endpoints distinct from the federation DM endpoints.
- `server/voxply-hub/src/dm_worker.rs` — list-walking failover, plus
  mirror-forward step for DMs accepted by any home hub.
- `server/voxply-hub/src/migrations.rs` — `subkey_certs`,
  `revocations`, `pairing_offers`, `prefs_blobs`,
  `home_hub_designations`, `dm_inbox_canonical`, `dm_mirror_forwards`.
- `server/voxply-hub/src/federation/handlers.rs` — accept
  designation refreshes from peer hubs.
- `client/voxply-desktop/src-tauri/src/lib.rs` — home hub list
  setting, designation cache, write-to-all replication.
- `client/voxply-desktop/src/` — list editor UI, divergence alert UI,
  per-slot status indicator.

## Consumers — what reads/writes the home hub list

This doc is the storage layer. The features that *use* it have their
own docs:

- [multi-device.md](multi-device.md) — devices, prefs blob,
  pairing protocol, identity (master + subkeys).
- DMs through home hubs — see [DM delivery](#dm-delivery--list-based-failover) above.
- Future: canonical friend list, notification settings, read cursors.

When adding a new personal-axis feature, ask: "is this state about
the user or about a community?" If user, it lives here, replicated
across the list.
