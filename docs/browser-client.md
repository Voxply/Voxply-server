# Browser Client

A second client at `client/voxply-web/` that hosts the same React UI as
the desktop ([client.md](client.md)) but with no Tauri shell. The hub's
HTTP + WebSocket API is unchanged; what changes is the platform layer
sitting between the UI and the network.

The browser client is **feature-subset, not feature-parity**: voice is
deferred (no raw UDP from the browser), and everything voice-adjacent
falls back to a read-only view. Text, DMs, E2E, screen share, admin
flows all work.

---

## Project layout

New crate-less project at `client/voxply-web/`. Same React 19 +
TypeScript + Vite versions as `voxply-desktop` — share a `package.json`
catalog if the toolchain allows; otherwise pin identically.

```
client/voxply-web/
├── index.html
├── package.json
├── vite.config.ts
└── src/
    ├── main.tsx            entry point (same shape as desktop)
    ├── App.tsx             shared with desktop (see below)
    ├── platform/           the adapter layer — this is the new code
    │   ├── index.ts        re-exports the public API: invoke replacement
    │   ├── session.ts      module-level HubSession map (the AppState equivalent)
    │   ├── http.ts         fetch() helpers with auth/error wrapping
    │   ├── ws.ts           HubWebSocket class (one per connected hub)
    │   ├── storage.ts      IndexedDB / sessionStorage wrappers
    │   └── commands/       one file per command group: hubs.ts, channels.ts,
    │                      messages.ts, dms.ts, alliances.ts, admin.ts, ...
    ├── identity/
    │   ├── store.ts        IndexedDB read/write of the seed
    │   ├── crypto.ts       DH derive, encrypt/decrypt, signing
    │   └── recovery.ts     BIP39 wrap/unwrap of the 32-byte seed
    ├── components/         shared with desktop
    └── styles.css          copy of desktop/src/styles.css
```

### Sharing UI with the desktop

The components directory is the load-bearing reuse. Three options were
considered:

| Option | Verdict |
|---|---|
| Copy `components/` into both trees | Drift risk; rejected |
| npm workspace with a `voxply-ui` shared package | Cleanest but reshuffles the desktop tree; deferred to a later refactor |
| Vite alias + filesystem path back to `voxply-desktop/src/components` | Pick this for v1 |

`voxply-web/vite.config.ts` resolves `@components` and `@shared` to
`../voxply-desktop/src/components` and `../voxply-desktop/src/...`. No
file duplication, no package machinery. The same trick covers
`types.ts`, `utils/`, and the reconnect-backoff hook.

CSS is a verbatim copy of `styles.css` for v1, not a fork. When the
desktop adds a token (a theme variable, a new component class) the
browser copy must follow. A later refactor lifts both files into a
shared package.

---

## Platform adapter pattern

The desktop UI calls `invoke("command_name", args)` ~120 times across
`App.tsx` and its components. The browser cannot import `@tauri-apps/api`
— so the adapter exposes the same logical functions and the UI imports
from `@platform` instead of `@tauri-apps/api/core`.

```ts
// src/platform/index.ts
export async function invoke<T>(cmd: string, args?: object): Promise<T>;

// or a typed surface that matches what the UI actually calls:
export const platform = {
  hubs: { add, list, setActive, remove, ping, previewInfo, reorder },
  channels: { list, create, rename, move, reorder, delete, ... },
  messages: { get, send, edit, delete, addReaction, removeReaction, search },
  dms: { listConversations, listMessages, send, create, encrypt, decrypt },
  voice: { /* throws NotSupportedInBrowser */ },
  identity: { getPublicKey, getRecoveryPhrase, recoverFromPhrase },
  prefs: { loadBlocked, saveBlocked, loadPinned, savePinned, ... },
  // ...
};
```

Pick the **typed surface**. The string-keyed `invoke()` works as a v1
crutch but the typed surface lets TypeScript check call sites and gives
the UI a clean migration path: today it imports `invoke` from
`@tauri-apps/api/core`; tomorrow both desktop and browser import the
same `platform` object, and the desktop's `platform` is a thin wrapper
around `invoke`. That convergence is the real prize.

### Session state

The desktop holds session state (active hub id, the WS task handles,
per-hub tokens) in a `tauri::State<AppState>`. The browser equivalent is
a module-level singleton in `platform/session.ts`:

```ts
interface HubSession {
  hub_id: string;
  hub_url: string;       // e.g. "https://hub.example.com"
  hub_pubkey: string;    // from GET /info, pinned per hub
  token: string;         // session token from POST /auth/verify
  ws: HubWebSocket | null;
}

const sessions = new Map<string, HubSession>();   // hub_id → session
let activeHubId: string | null = null;
```

This is exactly the `AppState` model — a process-global map of hubs to
sessions, with one designated as active. Multi-hub falls out for free:
the browser keeps as many `HubSession` rows (and WebSocket connections)
as the desktop does, and "switch active hub" is the same dataset change
followed by the same UI refetch.

Persistence: `sessions` is rehydrated on app load from sessionStorage
(or localStorage; see §"Auth flow"). The user's tab acts as a fresh
process — there is no equivalent of the Rust process surviving across
launches, so the rehydrate step replays "list saved hubs, attempt to
connect WS" on load.

---

## Identity in the browser

The desktop reads `~/.voxply/identity.json` from Rust. The browser
substitutes IndexedDB and pure-TypeScript crypto.

### Libraries

| Use | Library | Why |
|---|---|---|
| Ed25519 signing | `@noble/ed25519` | Audited, pure TS, no WASM. Matches the byte-level format of `ed25519-dalek` |
| Ed25519 → X25519 conversion | `@noble/curves/ed25519` (`edwardsToMontgomeryPriv`) | The seed→scalar→clamp recipe is exposed directly; SubtleCrypto has no equivalent |
| BIP39 recovery phrase | `bip39` | Same wordlist and entropy format as the desktop's `voxply_identity` crate |
| AES-GCM + HKDF | `@noble/ciphers` + `@noble/hashes` | See §"E2E crypto" — kept consistent with the noble stack to avoid SubtleCrypto's import ceremony |
| IndexedDB | `idb` | Tiny Promise wrapper; the raw IndexedDB API is callback hell |

### Storage schema

One IndexedDB database `voxply`, one object store `identity`, one record:

```ts
interface IdentityRecord {
  id: "main";                  // fixed key, single-record store
  seed_hex: string;            // 32-byte Ed25519 seed, hex-encoded — same format as desktop identity.json
  security_nonce: number;      // mirrors voxply_identity::Identity::security_nonce
  security_level: number;      // mirrors voxply_identity::Identity::security_level
}
```

The `seed_hex` format is deliberately byte-identical to the desktop's
`identity.json.seed` field. A user who exports the hex from the browser
can paste it into the desktop (and vice versa) and end up with the same
keypair. The recovery phrase is the other interop path — a 24-word
BIP39 phrase that maps to the same seed on either client.

### Export / import UI

A new pair of buttons in `SettingsPage` (the browser build shows both;
the desktop build can adopt the explicit "show seed hex" later):

| Action | Surface |
|---|---|
| **Export seed hex** | Shows 64-char hex. Copy-to-clipboard. Warning: "Anyone with this string can impersonate you." |
| **Export recovery phrase** | Shows the 24-word phrase. Same warning. |
| **Restore from seed hex** | Paste field. Validates hex, length 64, replaces IndexedDB record. |
| **Restore from recovery phrase** | Paste field. Same wordlist as desktop. |

Restoration wipes session storage (tokens) too, since the new identity
will not authenticate against hubs registered under the old key.

---

## Auth flow

The desktop's `add_hub` command is sequential HTTP + one signature.
TypeScript reimplements step-for-step:

| Step | Call | Body |
|---|---|---|
| 1 | `GET {hub_url}/info` | — | Returns `{ hub_name, hub_pubkey, ... }`. Pin `hub_pubkey` in the session record. |
| 2 | `POST {hub_url}/auth/register` | `{ public_key, display_name }` | Idempotent; existing users get 200. |
| 3 | `POST {hub_url}/auth/challenge` | `{ public_key }` | Returns `{ challenge: hex }`. |
| 4 | (local) | — | `signature = ed25519.sign(challenge_bytes, seed)`. |
| 5 | `POST {hub_url}/auth/verify` | `{ public_key, challenge, signature }` | Returns `{ token }`. Token is the session bearer. |

All subsequent requests carry `Authorization: Bearer {token}` (and the
WebSocket carries it in the query string, matching what the desktop
does — see `routes/ws.rs`).

### Token storage

| Where | Lifetime | Use case |
|---|---|---|
| **sessionStorage** (default) | Cleared on tab close | Shared/public computers; user logs in fresh each session |
| **localStorage with TTL** (opt-in) | Until TTL expires or user signs out | "Remember me" for personal devices |

The opt-in is a single checkbox in the add-hub modal. The TTL is the
token's natural expiry (hub-side; the client refreshes by re-signing a
challenge before expiry, same fallback path the desktop already has via
`reauth_session` in `lib.rs`).

The seed lives in IndexedDB regardless; only the **token** policy
differs. The seed is what proves identity; tokens are just cached
proofs.

---

## WebSocket management

The desktop spawns a Tokio task per hub that owns the WS, runs
subscribe/unsubscribe traffic, and emits Tauri events to React. The
browser equivalent is one TypeScript class per hub:

```ts
class HubWebSocket {
  constructor(hub_url: string, token: string, handlers: WsHandlers);
  send(msg: WsCommand): void;
  subscribeChannel(channel_id: string): void;
  unsubscribeChannel(channel_id: string): void;
  close(): void;
}

interface WsHandlers {
  onMessage:        (m: WsServerMessage) => void;
  onDm:             (m: WsDmEvent) => void;
  onTyping:         (e: WsTypingEvent) => void;
  onVoiceState:     (e: WsVoiceEvent) => void;        // browser still shows participant counts
  onScreenShare:    (e: WsScreenShareEvent) => void;
  onStatusChange:   (connected: boolean) => void;
}
```

**Reconnect.** Reuse the algorithm from
`voxply-desktop/src/hooks/useReconnectBackoff.ts` — same exponential
backoff (1s, 2s, 4s, …, cap 30s), same manual "Reconnect" button
override. The hook itself is portable as-is; copy it via the Vite alias.

**Event dispatch.** Three options were on the table:

| Option | Pro | Con |
|---|---|---|
| Custom DOM events | Decoupled; multiple listeners; built-in | Strings; no type safety; awkward to remove |
| Tiny EventEmitter | Typed events; easy off() | One more dependency; same shape as option 1 |
| **Callback map** | Typed; explicit; matches the desktop's existing pattern | Single subscriber per event type |

**Pick the callback map.** The desktop has exactly one subscriber per
event type (the `App.tsx` state container), and `App.tsx` is the
authoritative state owner — see [client.md](client.md). Multi-subscriber
dispatch would invite "let component X also listen for messages" which
breaks the single-state-container convention. The callback map is the
direct analogue of the Tauri event listener registration in `App.tsx`.

---

## E2E crypto in the browser

[e2e-encryption.md](e2e-encryption.md) defines the wire format. The
browser must produce byte-identical envelopes.

### Library choice

SubtleCrypto has Ed25519 and X25519 in current Chrome and Firefox, but
the **Ed25519 seed → X25519 scalar** conversion (SHA-512, clamp) is
*not* a standard WebCrypto operation. We need a non-WebCrypto path for
that one step. Once the X25519 scalar is in hand, both options are
viable:

| Path | DH derive | HKDF | AES-GCM | Verdict |
|---|---|---|---|---|
| Hybrid: noble for derive, WebCrypto for symmetric | `@noble/curves` | `SubtleCrypto.deriveKey` | `SubtleCrypto.encrypt` | Three import ceremonies per message |
| **All-noble** | `@noble/curves` | `@noble/hashes/hkdf` | `@noble/ciphers/aes` | One stack, smaller surface, easy to audit against the Rust side |

**Pick all-noble.** The hybrid path saves nothing (noble is already in
the bundle for the conversion step), and going all-noble means the
crypto module is a flat sequence of byte operations — no `CryptoKey`
import/export boilerplate, no `Promise<CryptoKey>` plumbing. The
bundled-size delta is small; the audit-clarity delta is large.

### Module shape

`src/identity/crypto.ts`:

```ts
function dhKeypairFromSeed(seedHex: string): {
  dhPriv: Uint8Array;  // 32 bytes, X25519 scalar (clamped)
  dhPub:  Uint8Array;  // 32 bytes, X25519 public key
};

function encryptDm(
  convId:        string,
  plaintext:     string,
  recipientDhPub: Uint8Array,
  myDhPriv:      Uint8Array,
  mySigningSeed: Uint8Array,
): {
  sender_pubkey:  string;
  conv_id:        string;
  ciphertext_hex: string;
  nonce_hex:      string;
  dh_pubkey_hex:  string;
  signature_hex:  string;
};

function decryptDm(
  convId:    string,
  envelope:  object,
  myDhPriv:  Uint8Array,
): string;

function signBytes(msg: Uint8Array, seedHex: string): string;  // hex
```

The envelope shape matches the Rust producer in
`voxply-desktop/src-tauri/src/lib.rs` (the `encrypt_dm` Tauri command);
the canonical signing bytes match the format defined in
[e2e-encryption.md](e2e-encryption.md) §"Message authentication"
(domain-separated prefix, length-prefixed strings).

`signBytes` covers DH-key publication (§"Publication" in
[e2e-encryption.md](e2e-encryption.md)) and auth challenge signing
(§"Auth flow" above).

---

## Voice — deferred

Voice is Opus-over-UDP via a hub-side relay (see [voice.md](voice.md)).
The browser cannot open raw UDP sockets; the only realistic path is
WebRTC, which would require the hub to terminate ICE/DTLS-SRTP and
transcode to/from the existing Opus relay. That's its own design
project, not this one.

**Browser v1 behaviour**:

- Voice channels still appear in the channel list. They behave like
  text channels for messaging.
- The "join voice" button is replaced by a tooltip:
  *"Voice is not available in the browser client. Open Voxply on your
  desktop to join."*
- Voice participant lists still render (the `/voice/participants`
  endpoint is read-only). The browser user sees who is in voice; they
  just cannot hear or speak.
- Voice population badges (`🎙️ N`) still render.
- The mute/deafen footer buttons are hidden in the browser (they only
  make sense while in voice).

This degrades gracefully: a user with the browser on one screen and the
desktop on another can chat in text on either, while voice continues to
work on the desktop.

---

## Screen share — minimal changes

Screen share already runs in the WebView layer of the desktop client
(`getDisplayMedia()` + a WebSocket binary frame stream). The only
Tauri-side dependency is the `get_hub_ws_info` command, which returns
`{ hub_url, token }` for the active session.

In the browser this is a one-line lookup in `platform/session.ts`:

```ts
export function get_hub_ws_info(): { hub_url: string; token: string } {
  const s = sessions.get(activeHubId!)!;
  return { hub_url: s.hub_url, token: s.token };
}
```

Everything else — `getDisplayMedia`, the WebSocket frame protocol, the
`<video>` element with MediaSource — is already browser-native.

---

## Feature parity

| Feature | Desktop | Browser v1 | Notes |
|---|---|---|---|
| Text channels | yes | yes | Same hub HTTP/WS |
| DMs | yes | yes | |
| E2E encrypted DMs | yes | yes | All-noble crypto stack |
| Screen share | yes | yes | Already WebView-native |
| Voice (Opus/UDP) | yes | **no** | UDP not reachable from browser |
| Voice participant list (read-only) | yes | yes | `/voice/participants` |
| Reactions | yes | yes | |
| Attachments | yes | yes | base64 in JSON, same shape |
| Hub admin (members, roles, bans, invites) | yes | yes | |
| Roles & permissions | yes | yes | |
| Invites (create, revoke, accept via link) | yes | yes | Deep links land as `?invite=` URL params |
| Alliances | yes | yes | |
| Games (iframe) | yes | yes | Already iframed |
| Friends (local + cross-hub) | yes | yes | |
| Hub discovery (directory) | yes | yes | |
| Multi-hub | yes | yes | Multiple `HubWebSocket` instances |
| Identity: generate | yes | yes | `@noble/ed25519` keygen |
| Identity: restore from seed | yes | yes | Hex paste, byte-identical format |
| Identity: restore from recovery phrase | yes | yes | BIP39, same wordlist |
| Auto-update | yes | n/a | Browser reloads; hub serves new assets |
| Deep links (`voxply://`) | yes | partial | Browser can register a protocol handler; falls back to query-string invites |
| System tray | yes | **no** | Browser has no tray |
| OS notifications | yes | yes | `Notification` API; needs user permission grant |
| Window title unread count | yes | yes | `document.title` mutation |
| Themes (Calm/Classic/Linear/Light) | yes | yes | Shared CSS, `data-theme` |
| Local block list | yes | yes | IndexedDB instead of JSON file |
| Pinned channels / collapsed categories | yes | yes | IndexedDB instead of JSON files |

---

## Open questions

| Question | Working answer |
|---|---|
| **PWA / offline support** | Defer. v1 is online-only. A service worker that caches the shell + a subset of recent messages is a v2 feature; the message cache needs an eviction policy and integration with the hub's message-history pagination. |
| **Push notifications (Web Push API)** | Hub does not implement Web Push today. Designing it means a new VAPID-keyed push subscription per device, a hub-side queue keyed on the user's subscription endpoint, and a `service-worker.js` to render notifications. Defer; the in-tab `Notification` API covers the foreground case. |
| **CSP headers from the hub** | The hub must emit `Content-Security-Policy` permissive enough to allow `connect-src` to its own origin (HTTPS + WSS) plus any alliance peers the client might federate-fetch from. Concrete header design is a follow-up; v1 hubs may run without CSP and the browser client logs a warning. |
| **Session persistence policy** | sessionStorage by default; localStorage + TTL opt-in (see §"Auth flow"). Revisit once the farm model lands — SSO across a farm changes the default. |
| **Distribution: hosted page vs. browser extension** | Hosted is v1 (any hub can serve the browser client from its own origin alongside the API; this keeps the federated story consistent — there is no central voxply.app). A browser extension would let us claim `voxply://` deep links and use `chrome.storage.local` instead of IndexedDB, but it's not worth the maintenance until the hosted version is stable. |
| **Cross-origin WS to other hubs** | The browser client connects to *one* hub at a time per `HubSession`, same-origin to the assets. Multi-hub means multiple cross-origin WS connections; each target hub must accept the client's origin (or `*`). Spell this out in [hosting.md](hosting.md) when the browser client ships. |
| **Mobile browsers** | Out of scope for v1. The layout assumes desktop viewport; a responsive pass is a separate project. |

---

## Cross-references

- Desktop client structure and state conventions: [client.md](client.md)
- E2E envelope format (must match byte-for-byte): [e2e-encryption.md](e2e-encryption.md)
- Identity model and seed format: [identity.md](identity.md)
- Voice pipeline (why it can't ride along in v1): [voice.md](voice.md)
- Hub HTTP routes: `server/voxply-hub/src/routes/mod.rs`
- Tauri commands the adapter replaces: `client/voxply-desktop/src-tauri/src/lib.rs`
- Shared types the browser also consumes: `client/voxply-desktop/src/types.ts`
