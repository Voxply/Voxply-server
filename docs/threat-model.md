# Threat Model

First-pass writeup. Treat this as a *guide for design decisions* —
when adding a feature, check whether it shifts something below from
"defended" to "weakened" (or vice-versa).

## Identity & trust roots

- **Identity** is a per-device Ed25519 keypair, generated locally.
- **Hub identity** is the same shape — a hub is just a peer with a
  long-lived keypair stored next to its DB.
- **No central authority.** There is no PKI, no email verification,
  no out-of-band attestation. Trust is established by direct
  invitation (a friend shared a hub URL) or by users choosing to
  install the same software.
- **Recovery phrase** (24-word BIP39) is the single backup mechanism.
  Lose it and lose access; share it and lose ownership.

## What we defend against (today)

| Threat                                      | Defense                                                                                  |
| ------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Brute-force auth                             | Challenge/response signed with Ed25519. Token is opaque, server-stored, expires.         |
| Account takeover via reused password        | N/A — there are no passwords.                                                            |
| Spam / login flooding                        | Per-IP rate limit on `/auth/*` and write endpoints (token bucket).                       |
| Random members posting in private channels   | Channel bans, role permissions, hub bans. Owner role is non-revokable.                   |
| Unwanted voice transmission in a channel     | Voice mute (hub-wide), per-channel min talk power, channel ban.                          |
| Hub admin abuse of own users                 | **Not defended.** Admin can read everything, ban anyone. Trust the hub or leave it.      |
| Lost/stolen device                           | Recovery phrase rotates the identity; old key remains valid until everyone notices.       |
| Mention noise / harassment                   | Three-state notification mode (all/mentions/silent), block-by-pubkey would help (TODO).   |
| Malicious file attachments                   | 3 MB cap, no execution path; client renders as data URLs (sandboxed in webview).         |

## What we *don't* defend against (yet)

These are known gaps. Each shapes a future feature decision.

- **Plaintext DMs at rest.** Every hub a DM passes through stores
  the cleartext. Federation makes this worse — your message lives
  on the recipient's hub forever. **Mitigation later: E2E encryption
  per-recipient with sender-pubkey signing; group DM is harder.**
- **Hostile peer hubs.** Federation auth proves identity but not
  good behavior. A peer hub can lie about messages it forwards,
  drop alliance reactions, fabricate "you were mentioned" events.
  **Mitigation: signed-by-author messages so peers can't tamper;
  not implemented.**
- **Hostile alliance members.** When Hub A joins Alliance X with
  Hub B, Hub B's admin can read every shared channel from Hub A
  including any private-but-shared channels. **Mitigation: only
  share what you'd public-post; treat alliance channels as semi-
  public.**
- **Hub server compromise.** Whoever has the hub's `hub_identity.json`
  can impersonate the hub, sign federation calls, accept arbitrary
  alliance joins. Same blast radius as any centralized service losing
  its database.
- **Key compromise / no revocation.** A leaked private key is leaked
  forever. Generating a new identity loses your friend connections,
  channel membership, etc. **Mitigation later: a master key →
  device key model with revocation lists.**
- **Metadata leaks.** Even with E2E DM bodies, peers see
  who-talked-to-whom-when. Onion-routing the federation transport
  would fix this; out of scope.
- **Voice plaintext on the hub UDP relay.** Voice packets are
  encoded but not encrypted to the hub. The hub operator can
  passively wiretap voice. **Mitigation: peer-to-peer voice when
  topology allows, or SRTP-style E2E.**
- **Bot abuse.** Once bots ship (#148), a compromised bot token
  posts anywhere the bot has permission. Per-bot rate limits +
  scoped tokens are the planned defense.

## Decisions this should drive

- **Bots and integrations (#148):** bots get scoped tokens, not full
  user permissions. Token rotation is owner-pubkey-gated.
- **Alliance peers:** any "private" channel marked alliance-shared
  is effectively public to alliance admins. UI should warn before
  sharing.
- **Federated reactions (open from #124):** if peer hubs forward
  reactions, they can also fabricate them. Sign reactions by
  reactor pubkey when this ships.
- **Cross-hub friends (medium pri):** friend list is a bidirectional
  pubkey assertion; a peer hub claiming "alice and bob are friends"
  isn't authoritative — both sides need their own copy.

## Out of scope (probably forever)

- **Anonymity from the hub operator.** A hub knows your pubkey,
  IP, when you connect. If you want anonymity, run your own hub.
- **Resistance to legal compulsion.** A hub operator served with
  process can hand over everything they have. Encryption helps for
  DMs in flight; nothing helps once the operator decides to log.
