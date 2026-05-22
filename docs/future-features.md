# Future Features

Designed-but-not-built features. Each section is the design we'd start
from when the time comes. None of this is shipping today.

> See also: [farm-model.md](farm-model.md) for the multi-hub server
> layer, and [gaming.md](gaming.md) for the game distribution platform.

## Anti-spam — proof-of-work + hub certifications

**Problem**: decentralized identity means bots can generate keypairs
instantly. Without friction, a hub can be flooded by fresh keys.

**Two-layer defense planned:**

### Layer 1 — proof-of-work levels

- Client computes a SHA-256 puzzle tied to its keypair (leading-zero hash).
- Each level takes exponentially more CPU: level 15 ≈ 1 min, level 23 ≈
  30 min, level 30 ≈ 8 hours.
- Hub sets a minimum level to connect.
- Proof stored in the identity file. Hub verifies instantly with one
  hash check. Cannot be faked — pure math.

PoW primitives already exist in
`shared/voxply-identity/src/pow.rs`; they aren't enforced yet.

### Layer 2 — hub certification (reputation)

- Hub signs a statement: "user X has been a member since Y in good
  standing."
- Signature is verifiable by anyone (hub's pubkey is published via `/info`).
- Users collect certifications from multiple hubs — a reputation
  portfolio.
- Other hubs can require certifications from trusted hubs.

### Also considered

- **Invite-only hubs** — admin issues invite codes. Simple, effective for
  private communities. Already shipped.
- **Per-IP rate limiting** — secondary barrier. Already in place on
  `/auth/*` and write endpoints.
- **Account age alone** — too weak. Easily faked by pre-generating keys.

### Order of implementation

PoW first (foundational, math-based). Hub certification later (requires
trust decisions). Invites are already the quick option for private hubs.

---

## Moderation enhancements — channel ban, voice mute, talk power

Beyond today's ban/mute/kick/timeout:

- **Channel ban** — block a user from specific channels (text + voice).
  New `channel_bans` table (channel_id × pubkey). Check on channel
  access.
- **Voice mute** — user can hear but can't speak. Hub stops forwarding
  their audio packets. New `voice_mutes` table.
- **Talk power** — channels carry a `min_talk_power` threshold for
  their voice side. Users get talk power from their role. Below
  threshold = can read/post text and listen in voice, but can't
  transmit. Users can "raise hand" to request permission.

**Why deferred**: basic moderation covers the essentials. Channel-level
controls and voice moderation are the next layer once the basics are
proven.

> Some of these (`voice_mutes`, talk power) have admin UI scaffolding
> already; the enforcement is partial. Check
> `server/voxply-hub/src/routes/moderation.rs` and
> `routes/role_models.rs` for current state.

---

## Identity recovery — beyond the recovery phrase

The recovery phrase ([identity.md](identity.md)) is shipped. These are
the next layers, none built:

1. **Backup / export** — explicit export-import of `identity.json` with
   a passphrase wrapper. The file already exists at
   `~/.voxply/identity.json`; this is just UX.
2. **Device linking** — master keypair authorizes per-device sub-keys.
   Revoke a lost device from another. Layers cleanly on the recovery
   phrase: phrase becomes the master seed; existing single-key
   identities migrate by treating themselves as "device 0."
3. **Recovery contacts** — designate trusted keypairs that can reclaim
   your roles or hub ownership if your key is lost. Hub-side, not
   identity-side.

**Why deferred**: the recovery phrase covers the "I formatted my PC"
case. Multi-device and social recovery are real needs, but only when
users actually want them.

---

## Bots and integrations

**Status**: future direction, not built. Tracked as task #148.

**Goal**: first-class bot support — automated identities that can read
channels, post messages, and react to events.

The pubkey-based identity is well-suited: a bot is an Ed25519 identity
with no recovery phrase and a long-lived token. Two likely shapes:

- **Bots-as-users** — bot identities are regular member rows with a
  `bot` flag and elevated permissions. Posts via the existing
  `/messages` endpoint. Federates the same way users do. Fits everything
  we've built.
- **Outbound webhooks** — hub POSTs to external URLs on events (channel
  message, voice join, etc.). One-way but trivial to integrate with
  existing tools.

Both shapes are valid; picking which (or whether to support both) is
the design exercise.

**Security**: bots get scoped tokens, not full user permissions. Token
rotation is owner-pubkey-gated. Per-bot rate limits. See
[threat-model.md](threat-model.md).

**How to apply now**: when suggesting features that touch identity,
permissions, or messaging APIs, keep the bot model in mind so we don't
paint ourselves into a corner.

---

## Multi-device pairing

**Status**: design committed. The canonical docs are
[multi-device.md](multi-device.md) (identity + QR pairing) and
[home-hub.md](home-hub.md) (storage layer for personal-axis state).
Read those — the writeup below is the *pre-decision* exploration kept
for historical context only and may drift from the committed design.

**Goal**: let one user have Voxply on multiple devices (phone + desktop)
under a single identity. Today every device generates its own keypair
and is treated as a separate user. Pasting the recovery phrase on a
second device replaces that device's identity with the first device's,
which works as a "I formatted my PC" recovery story but is awkward
for "I want both devices online at the same time."

**Status**: design pending. UX direction agreed (QR-code pairing flow,
same model as the well-known messaging apps). Underlying protocol not
yet picked — the two foundational choices below are the next thing to
decide before we write any pairing code.

### Identity model — pick one

| Option | What it is | Cost |
|---|---|---|
| **Shared keypair** | All devices have the same private key. QR-pairing transfers the key. Hubs see one pubkey = one person. | Simple. ~1-2 weeks. **No revocation** — losing one device means rotating the key everywhere. |
| **Master + device subkeys** | A master keypair (derived from the recovery phrase as a seed) signs per-device subkeys. Each device's subkey signs daily traffic; the master proves "this subkey is mine." | Proper revocation, proper sovereignty. Hub protocol changes (hubs verify subkey signatures). Multi-month. The recovery phrase becomes an HD-wallet seed; existing single-key identities migrate as "device 0." |

The second option is what `decisions.md` calls "the right thing later"
and is forward-compatible with today's keypair model. The first option
is the dirty-but-fast v0 that gets users multi-device immediately at
the cost of needing a rewrite when revocation comes up.

### State sync — separate decision

Each device today has its own JSON files for hub list, prefs, blocked
users, friends, voice settings. Multi-device means these need to live
*somewhere* shared. The choices, ranked from least invasive to most:

1. **No sync** — each device keeps its own list. Same identity, but
   you re-add hubs on each device. Simple but feels broken.
2. **One of the user's hubs holds an encrypted prefs blob** — pick a
   home hub (or the first one) to be the "sync hub." Encrypted with a
   key derived from the master seed. Other devices fetch the blob.
   Reintroduces "pick a primary," which we explicitly punted earlier.
3. **Every hub the user is on replicates the blob** — fancy, invites
   consistency bugs across hubs that don't agree on the blob version.
4. **Separate identity service** — a central component. Conflicts with
   the federated pillar. Off the table.

Option 1 is the v0 path. Option 2 is the right destination. Option 3 is
over-engineering. Option 4 is the wrong direction.

### Recommended path forward

1. Write a real design doc (likely `docs/multi-device.md` when it
   grows past this section) that picks the identity model + the sync
   model + sketches the QR pairing protocol message-by-message.
2. Decide together; the call here matters because it's hard to undo.
3. Build to that design.

**Don't start coding before the doc.** The architectural choice is
load-bearing — the "right" answer is multi-month and changes hub
protocol; the "fast" answer is ~1-2 weeks but needs to be replaced
when revocation matters. Both are real, neither is obviously correct.

### What's already in place that helps

- The recovery phrase already deterministically yields a keypair. If
  we go master+subkey, the same phrase becomes the seed and existing
  identities migrate cleanly.
- `localStorage.voxply.recoveryAcknowledged` flag from the onboarding
  block tracks whether the user has backed up their phrase — useful
  precondition for "you can pair another device."
- Per-device storage already isolates hub list, prefs, blocked users,
  voice settings — these are the things that'd need to sync.

### What's already shipped toward this

- `subkey_revocations` and `subkey_certs` tables in the DB schema.
- `GET/POST /identity/revocations/{master}` and
  `GET/POST /identity/devices/{master}` endpoints in `identity.rs`.
- QR pairing state machine (`pairing.rs`): offer, claim, complete, poll.
- **Auth enforcement**: HTTP middleware and WS handshake now check
  `subkey_revocations` and return 401 for revoked keys. The table is
  empty in the single-key model today, so behaviour is unchanged — but
  revocations will be honoured the moment multi-device keys are issued.

### What's not done

QR code generation UI, device list screen, subkey issuance on pairing
completion, per-hub revocation propagation, encrypted prefs blob sync.

---

## Nested channels

**Goal**: let users build an arbitrary tree of categories and channels.
Remember Voxply channels are **unified text + voice** ([decisions.md](decisions.md)) —
a "channel" in the tree is one room where both chat and voice live.

```
GamesCategory                      ← depth 1 (category)
├── LeagueOfLegendsCategory        ← depth 2 (category)
│   ├── AllianceSection            ← depth 3 (category)
│   │   ├── raid-planning          ← depth 4 (channel — leaf)
│   │   └── lounge                 ← depth 4 (channel — leaf)
│   └── TeamSection                ← depth 3 (category)
│       └── strats                 ← depth 4 (channel — leaf)
└── DotaCategory                   ← depth 2 (category)
    └── general                    ← depth 3 (channel — leaf)
```

Each leaf is a channel — chat history and voice in the same place. The
intermediate nodes are categories (containers).

**Why**: today's flat "category > channel" caps community organization at
two levels. Game communities, in particular, want topic > sub-topic >
section before getting to actual channels — and we don't know in advance
how deep any community will want to go.

### Rules

- **Configurable depth cap** — hubs are sovereign, but admins can
  optionally set a `max_channel_depth` hub setting (integer ≥ 1).
  `0` means unlimited. Default is `0`. The cap is enforced server-side
  on create and move operations.
- **Categories are containers.** They hold other categories and/or
  channels. They can't hold messages or voice (`is_category=1` rows).
- **Categories can't live at max depth.** A category at the deepest
  allowed level would be an empty container — nowhere to put children.
  Invariant: a category may only be created/moved to depth ≤
  `(max_channel_depth − 1)`. Channels (leaves) may go to any depth up
  to `max_channel_depth`. When `max_channel_depth = 0` (unlimited) this
  restriction doesn't apply.
- **Channels are leaves.** Each channel is unified text + voice and can
  sit at any depth.
- **Permissions cascade** — a deny on a parent applies to children
  unless the child explicitly overrides. Same model as a file system.

### Hub setting — `max_channel_depth`

Stored in the hub settings table (new row, key `max_channel_depth`,
default `"0"`). Surfaced in the hub admin panel under a "Structure"
section. The UI should show a helper like:

> "0 = unlimited. If set to 4, categories can nest up to depth 3 and
> channels up to depth 4."

Enforcement is server-side so API clients can't bypass it.

### Data model

Today's `channels` table is already self-referential — it has
`parent_id TEXT REFERENCES channels(id)` and `is_category INTEGER` —
the schema **already supports nesting**. What's missing is the UI,
route validation, and the hub setting:

- The drag-drop UI in the desktop client treats categories as one level
  and channels as their children, with no recursion.
- There's no UI to make a category a child of another category, even
  though the data model accepts it.
- `max_channel_depth` needs a row in the hub settings table and a field
  in the admin panel.

Work breakdown:
- Allow drag-drop that nests a category under another category.
- Allow drops that nest a channel under any category at any depth.
- Reject drops that would create a cycle (a node into its own descendant).
- Reject drops / creates that violate `max_channel_depth` or the
  category-at-max-depth invariant.

No schema migration needed for channels. One new settings row.

### Open implementation questions

- **Drag-drop with arbitrary depth** — the only forbidden move is a
  cycle (dropping a node into one of its own descendants). Visual
  indentation past ~6 levels needs a strategy: horizontal scroll,
  auto-collapse, or breadcrumb-style display in the sidebar.
- **Permission override UI** — when a child explicitly grants what its
  parent denies, that override needs a clear UI affordance so admins
  understand what's happening.
- **Permalinks** — today's `#general` becomes
  `Games / LoL / Alliance / #raid-planning`. Permalink format: keep
  the channel id only and resolve display path client-side.
- **No migration needed** — both categories and channels can already
  live at the root (`parent_id NULL`) or nested under a category in
  today's schema. Existing data is unchanged; new nesting is opt-in
  whenever an admin decides to nest something.

### What we explicitly don't want

- **Channel-as-container** (a channel that holds messages AND has
  sub-channels). This would confuse users. Keep the `is_category`
  distinction sharp: containers vs. leaves.

---

## Forum channel type

**Status**: future design. No design committed yet.

**Goal**: a channel variant where the content is an indexed list of
*posts* (each with a title) rather than a continuous message stream.
Users browse posts, open one, and reply inside it. Useful for
announcements, Q&A boards, patch notes, bug reports — anywhere a
timeline feed is the wrong shape.

### How it differs from a regular channel

| | Regular channel | Forum channel |
|---|---|---|
| Primary content | Continuous message stream | Ordered list of posts |
| Each entry | Message (no title) | Post with title + body |
| Replies | Thread hanging off a message | Reply thread inside the post |
| Voice | Yes (unified text + voice) | No — posts-only, no voice |
| Search | Full-text on messages | Full-text on post titles + bodies |

Forum channels are leaves in the channel tree (same as regular
channels) and live at the same depth positions. They carry a new
`channel_type` discriminant: `"text"` (default today) vs `"forum"`.

### Data model sketch

New `posts` table:
```
posts(id, channel_id, author_pubkey, title TEXT, body TEXT,
      created_at, edited_at, is_pinned)
```

New `post_replies` table (or reuse `messages` with a `post_id` FK):
```
post_replies(id, post_id, author_pubkey, body TEXT,
             created_at, edited_at, reply_to_id)
```

The `channels` table gains a `channel_type TEXT DEFAULT 'text'`
column. No other existing tables change.

### Moderation

Forum posts follow the same moderation model as messages — hub
moderators can delete posts and replies, channel bans apply to posts
too. The existing moderation routes extend naturally.

### Federation

Posts and replies federate the same way messages do (alliance shared
channels). The federation envelope gets a `post` event type alongside
`message`. Out-of-scope for the first iteration — federated forums
can be added once the local model is stable.

### Why deferred

Regular channels cover the "live conversation" shape well. Forums
need a distinct UI (post list view, post detail view, reply thread)
and new DB tables. Worth doing when communities ask for it, but not
blocking anything today.

---

## Server tags — federated portable badges

**Status**: future design. Tracked as task #98. No design committed yet.
