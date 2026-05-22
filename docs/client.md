# Desktop Client

Tauri 2 + React 19 + TypeScript. Two halves:

- **Rust shell** (`client/voxply-desktop/src-tauri/`) — file I/O, voice,
  OS notifications, system tray, OS-native dialogs. Communicates with
  the UI via Tauri commands.
- **React UI** (`client/voxply-desktop/src/`) — everything visual.

## React entry

- `src/main.tsx` — boots React
- `src/App.tsx` — state container. Holds all hooks, effects, and
  handlers; renders only by composing top-level components.
- `src/components/` — all visual components.

**State in `App.tsx`**: identity, hub list, active hub, channel list,
messages, DMs, alliances, blocked users, notifications, theme, voice
state, and all UI transient state (modal open flags, form drafts,
context menu positions).

### Component tree (top-down)

`App.tsx` renders one of three full-page views:

| Condition | Component |
|---|---|
| `showHubAdmin` | `HubAdminPage` |
| `showSettings` | `SettingsPage` |
| otherwise | main layout (below) |

The main layout is:

```
HubSidebar          — leftmost icon bar: hub list + DM button
ChannelSidebar      — channel/DM list, alliances, games, voice footer
ContentArea         — chat area, game iframe, member sidebar
```

Overlays rendered unconditionally when their state is set:
`AddHubModal`, `CreateChannelModal`, `InstallGameModal`, `EditGameModal`,
`FriendsModal`, `ChannelContextMenu`, `UserContextMenu`,
`EditDescriptionModal`, `ChannelBansModal`, `ChannelPalette`, `Lightbox`.

### Component categories

**Page-level** (own the full viewport):
`HubAdminPage`, `SettingsPage`

**Layout panels** (receive all state as props, no internal state except
DnD sensors):
`HubSidebar`, `ChannelSidebar`, `ContentArea`

**Modals and overlays** (opened by App.tsx, closed via `onClose` prop):
`AddHubModal`, `CreateChannelModal`, `InstallGameModal`, `EditGameModal`,
`FriendsModal`, `EditDescriptionModal`, `ChannelBansModal`,
`ChannelContextMenu`, `UserContextMenu`, `ChannelPalette`, `Lightbox`

**Primitives** (self-contained, used by multiple parents):
`Avatar`, `TypingIndicator`, `MessageContent`, `MessageReactions`,
`ReactionPicker`, `Attachments`, `UserListGrouped`, `MicLevelMeter`,
`SortableItems`, `Icons`, `ThemePicker`, `PttKeyBinder`,
`ImagePicker`, `AvatarEditor`, `WelcomeRecoveryBlock`

**Admin sub-components** (used inside `HubAdminPage`):
`MemberRow`, `RoleEditor`, `RoleCreator`, `InvitesSection`,
`AlliancesSection`, `ChannelBansModal`

**Settings sub-components** (used inside `SettingsPage`):
`ProfileTab`, `RestoreIdentitySection`, `MicLevelMeter`

## Persistence (per-device)

JSON files in Tauri's app-data directory, owned by the Rust shell:

| File                  | Purpose                                  |
|-----------------------|------------------------------------------|
| `identity.json`       | The Ed25519 keypair (one per device)     |
| `hubs.json`           | Known hubs: URL + nickname + last token  |
| `prefs.json`          | UI prefs (theme, notification scopes)    |
| `blocked_users.json`  | Per-device pubkey block list             |

These do **not** sync across devices today. (See [decisions.md](decisions.md).)

## Tauri commands

Defined in `client/voxply-desktop/src-tauri/src/lib.rs`. A non-exhaustive
list:

- `load_identity` / `save_identity` — keypair persistence
- `load_hubs` / `save_hubs` — hub list
- `load_blocked_users` / `save_blocked_users` — per-device block list
- `preview_hub_info` — pre-add fetch of hub name/icon
- `clear_local_data` — wipes all of the above (double-confirmed in UI)
- voice control commands (start/stop capture, mute, deafen, device select)

## Themes

Four: Calm (default), Classic, Linear, Light. Theme tokens are CSS
variables applied at the root via `data-theme`; switching is just a
dataset change. Light overrides shadow tokens too — the dark-mode
shadow values would look heavy on a light background.

## Voice surface in the sidebar

Each channel that has anyone in voice renders the participants nested
underneath it in the sidebar — names indented under the channel row.
The data comes from `GET /voice/participants` (polled every 5s); the
channel row itself shows a `🎙️ N` count badge when there are people in
voice. Double-clicking any channel joins voice on it.

The user's mute mic and deafen toggles live in the sidebar footer next
to the settings gear, and only render when the user is in voice on
some channel. The button shape is `.btn-icon-gear` shared with the
gear; an `.active` modifier turns it red while toggled on.

## WebSocket lifecycle

The Tauri side opens one WebSocket per connected hub and forwards
events to React. When the connection drops (hub restart, network blip),
the client emits `hub-ws-status: connected=false`. The React side
handles this by scheduling an automatic reconnect with exponential
backoff (1s, 2s, 4s, … capped at 30s) — no user action required. The
existing "Reconnect" button in the banner is a manual override that
resets backoff and tries immediately.

State lives in two refs: `reconnectTimers` (per-hub setTimeout IDs) and
`reconnectAttempts` (per-hub backoff counters). Both clear on success
or when the user leaves the hub.

## State organisation

**Single flat state container.** All ~172 `useState`/`useRef`/`useEffect`
hooks, all effects, and all event handlers live in `App.tsx`. Components
are pure renderers; they receive values and callbacks as props and hold
no application state of their own (only local UI concerns: DnD sensors,
input focus).

**No per-domain custom hooks. No application-state context.** This was
considered and rejected — see [decisions.md](decisions.md). The short
version: handlers and effects are heavily cross-domain (a single hub
switch touches eight "domains"; the hub-WS event handler writes into
nine of them), so a domain split would either leak its dependencies
through its public surface or force the coordinating handlers back up
into App.tsx anyway. Context would also weaken TypeScript inference
and make the two consumer components untestable without a provider.

**Logical groupings inside App.tsx** (these are reading-order labels,
not modules):

| Group | Owns |
|---|---|
| Identity | `publicKey`, `recoveryPhrase`, `profiles`, `defaultProfileId` |
| Hubs | `hubs`, `activeHubId`, `hubConnected`, `reconnectingHubs`, `pingByHub`, `hubNotifyMode`, reconnect refs |
| Channels | `channels`, `selectedChannel`, `pinnedChannels`, `collapsedCategories`, `unreadByChannel`, `channelNotifyMode`, `voicePartByChannel`, `voiceActiveUsers` |
| Messages | `messages`, `editingMessageId`, `editingDraft`, `replyTarget`, `pendingAttachments`, `inputText`, `searchQuery`, `searchResults`, `searchOpen`, `stickToBottom` |
| Typing | `typingByKey`, `dmTypingByKey` and their debounce refs |
| DMs | `view`, `conversations`, `selectedConversation`, `dmMessages`, `unreadDms` |
| Alliances | `userAlliances`, `allianceChannels`, `selectedAllianceChannel`, `allianceMessages` |
| Voice | `voiceChannelId`, `selfMuted`, `selfDeafened`, audio devices, `vadThreshold`, `voiceMode`, `pttKey`, `micLevel`, `micTesting` |
| Friends | `friends`, `pendingFriends`, friend-request form drafts |
| Games | `installedGames`, `selectedGame`, install/edit form drafts |
| Settings | `theme`, `mentionPingEnabled`, `blockedUsers`, settings tab |
| Hub admin | `myRoles`, `myApprovalStatus`, `adminMembers`, `adminBans`, `adminVoiceMutes`, `adminInvites`, `adminRoles`, `pendingMembers`, `requireApproval`, hub-edit drafts |
| UI flags | modal open flags, context menu positions, lightbox, toast, error, palette open, member sidebar hidden |

**Convention for additions**: when adding state, place it next to the
existing group that owns the same data. Effects go below their state
group. Handlers that span groups go below all the state they touch.

**What is allowed to leave App.tsx**:
- Pure helpers (URL builders, formatters, sort comparators) → `src/utils/`.
- Small, single-purpose custom hooks for *mechanism* not *domain* —
  e.g., a debounce helper, a reconnect-backoff helper. These are fine
  if they take their dependencies as arguments and return primitives.

**What is not allowed**:
- Domain hooks (`useHubs`, `useMessaging`, `useVoice`, ...).
- Context providers for application state.
- Components owning state that should be in App.tsx (drafts, selections,
  toggles that other components also need to see).

## Conventions

- **`App.tsx` owns state; components own rendering.** All hooks, effects,
  and event handlers live in `App.tsx`. Components receive what they need
  as props and call handlers via callbacks — no internal state except
  local UI concerns (DnD sensors, input focus).
- **Props-only data flow.** The fat prop interfaces on `ContentArea`
  (~50 props) and `ChannelSidebar` (~30 props) are deliberate — they
  are direct children of `App.tsx`, so the "drilling" is one level
  deep, and the explicit interface keeps the components testable and
  type-checked. See [decisions.md](decisions.md).
- **No state library**. React state covers everything. Context is not
  used for application state either (see "State organisation" above).
- **No router**. Internal "pages" are just conditionally rendered panels.
- **Client never trusts the hub for permissions** — it shows or hides UI
  based on what the hub returns; the hub re-checks every action.

## Tests

The Tauri Rust side has unit tests in `src-tauri/src/lib.rs` under a
`#[cfg(test)] mod tests` block. We deliberately avoid testing Tauri
commands directly (they need a real `AppHandle` / `State` / runtime);
instead we cover the boundary logic that doesn't need any of that —
URL encoding, serde shapes (so an old prefs file still round-trips),
and small pure helpers. Run with `cargo test` from `src-tauri/`.

To grow the suite: any function that takes plain values and returns
plain values is fair game. Anything that touches `dirs::data_dir()`
needs a refactor to take a base path before it's testable.

The React side has no test framework wired up yet — that's a separate
future task.

## What's not done

- Mobile client
- Web client (would need WebRTC voice)
- Plugin system / theme marketplace
