# Screen Share

Live screen and webcam sharing inside a voice channel. Any approved
member can share their screen (window or whole monitor) plus an
optional webcam stream; everyone else in the channel sees the live
view in a viewer panel alongside the chat.

v1 ships as a **hub-relayed WebSocket chunk stream** — the existing
WS infrastructure carries WebM chunks from sharer to hub to viewers.
v2 (deferred) upgrades the transport to WebRTC P2P with the hub as
SDP/ICE signaler. The user-visible feature surface is identical
between the two transports; the change is internal.

---

## Where it lives

| Concept | Lives on |
|---|---|
| Capture (sharer's MediaStream) | Client only — never leaves the sharer's machine in raw form |
| Encoded chunks in flight | Hub WebSocket, fan-out to channel subscribers |
| Stream metadata (who's sharing, source type) | Hub runtime state, not persisted |
| Viewer layout / volume | Client only — per-device prefs |

Screen share is **channel-scoped**, not hub-scoped. A user shares
into channel X; only members of channel X who currently have it
selected (or have its viewer panel open) receive the stream. No
persistent storage — the hub is a relay, not a recorder.

---

## Capture (sharer side)

Built on the browser `getDisplayMedia()` API, which is available in
the Tauri 2 webview on Windows, macOS, and Linux. The OS-native
picker handles source selection — we do not roll our own.

### Source types

| Source | `getDisplayMedia` reports | UI label |
|---|---|---|
| Whole monitor | `displaySurface: "monitor"` | "Entire screen" |
| Application window | `displaySurface: "window"` | "Window" |
| Single browser tab | `displaySurface: "browser"` | Not exposed (Voxply doesn't run in a browser context for sharing) |

The picker is invoked once with broad constraints; the user's
selection dictates which surface is captured. Voxply does not need
to filter by source type — the OS picker already presents the right
list per OS.

### Constraints

Requested on the sharer's `getDisplayMedia` call:

```
{
  video: {
    frameRate: { ideal: 30, max: 60 },
    width:     { max: 1920 },
    height:    { max: 1080 },
  },
  audio: <bool from host config>,   // see "Audio" below
}
```

Resolution caps at 1080p in v1. Higher resolutions are reachable via
a later "high quality" preset; in v1 the cap keeps bitrate bounded
and decoding cheap on weak viewer hardware.

### Webcam (optional second stream)

A separate `getUserMedia({ video: true, audio: false })` call,
encoded and sent as an **independent stream ID** over the same WS
session. The viewer sees it as a small overlay on top of the screen
share view (picture-in-picture style). Disabling the webcam toggles
the second stream off without affecting the screen share.

Webcam audio is **not** captured (the user's mic is already handled
by the voice pipeline in `shared/voxply-voice/`). Mixing webcam mic
into the screen-share audio path would double-mic the user.

Webcam-only sharing (no screen) is allowed: same path, just the
webcam stream and no screen stream. Useful for video calls.

---

## Transport — v1 (hub-relayed WS chunks)

### Encoding

The sharer's `MediaStream` is fed into a `MediaRecorder` configured
for WebM containers with VP8 video and Opus audio. VP9 is the future
upgrade target but VP8 has wider hardware-decode support today.

```
MediaRecorder({
  mimeType: "video/webm;codecs=vp8,opus",
  videoBitsPerSecond: 2_500_000,
  audioBitsPerSecond:    96_000,
}).start(<chunk_ms>)
```

`chunk_ms` (timeslice argument to `start()`) controls how often a
chunk fires. Tradeoff:

| `chunk_ms` | Latency | Overhead | Choice |
|---|---|---|---|
| 100 ms | ~150 ms end-to-end | many small frames, more WS overhead | dev default for testing |
| 250 ms | ~350 ms end-to-end | balanced | **v1 default** |
| 1000 ms | ~1.1 s end-to-end | very low overhead, high latency | too laggy for live share |

Each chunk carries one or more keyframes plus deltas. The first
chunk in a session is special — it carries the WebM init segment
(EBML header + Segment header) that downstream MSE buffers need
before they can decode anything.

### Wire format

Two new envelope variants on the existing typed message channel in
`server/voxply-hub/src/routes/chat_models.rs` (the same enum that
already carries `Subscribe` / `Unsubscribe` at
lines 175-181 (`SubscribeAll` removed — hub auto-subscribes on connect)):

```
// Client → Hub
ScreenShareStart {
  channel_id: String,
  stream_id: String,       // sharer-chosen UUID; distinguishes screen vs webcam
  kind: "screen" | "webcam",
  mime: String,            // "video/webm;codecs=vp8,opus"
  has_audio: bool,
}
ScreenShareChunk {
  channel_id: String,
  stream_id: String,
  seq: u32,                // monotonic per stream
  is_init: bool,           // first chunk carries the init segment
  // chunk bytes are sent in the **next** binary WS frame
}
ScreenShareStop {
  channel_id: String,
  stream_id: String,
}

// Hub → Client (broadcast to channel subscribers)
ScreenShareStarted { /* same fields as Start, plus sharer_pubkey */ }
ScreenShareChunkOut { /* same fields as Chunk, plus sharer_pubkey */ }
ScreenShareStopped  { /* same fields as Stop, plus sharer_pubkey */ }
```

Chunk bytes are sent as a **binary WS frame immediately following**
the `ScreenShareChunk` text envelope (so the envelope describes the
next binary frame). This avoids base64'ing every chunk into JSON.
The hub correlates them by per-connection sequence.

### Hub routing

The hub keeps an in-memory map per channel:

```
HashMap<channel_id, ActiveShare {
  sharer_pubkey,
  streams: HashMap<stream_id, StreamMeta { kind, mime, has_audio, last_seq }>,
}>
```

On `ScreenShareChunk` arrival, the hub looks up the channel's
current subscriber set (same list it uses for chat broadcast) and
forwards the envelope + binary frame to each subscriber except the
sharer. No transcoding, no buffering, no persistence.

### Viewer buffering

Viewers receive chunks and feed them into a `MediaSource` (MSE)
attached to an HTML `<video>` element:

1. On `ScreenShareStarted`, create a `MediaSource`, attach to a new
   `<video>` element, add a `SourceBuffer` matching the announced
   `mime`.
2. On each `ScreenShareChunkOut` + binary frame, append the bytes
   to the `SourceBuffer`. The init chunk (`is_init: true`) must be
   appended **first**; later chunks rely on its header.
3. If a viewer joins mid-stream, they don't have the init segment.
   The hub caches the init chunk per active stream and pushes it
   to any late joiner before forwarding new chunks. Without this
   the viewer's `SourceBuffer` rejects every chunk.
4. On `ScreenShareStopped`, the viewer calls
   `mediaSource.endOfStream()` and tears down the element.

The `<video>` element runs in `autoplay muted` until the user
interacts (browser autoplay policy) — the volume slider re-engages
audio on click.

### Latency expectations

- Encode: ~50 ms (one chunk timeslice plus encoder pipeline)
- Hub relay: ~5–20 ms (WS forward, no transcoding)
- Network: variable, dominated by client uplink
- Decode + render: ~50–100 ms (MSE append + paint)

Total end-to-end: **300–500 ms** at the v1 250 ms chunk default.
Acceptable for "watch what I'm doing"; not acceptable for tight
gameplay co-op. v2 (WebRTC) targets sub-100 ms for the latter case.

### Bandwidth

At 2.5 Mbps video + 96 kbps audio, the sharer uploads ~2.6 Mbps.
The hub fans out N×2.6 Mbps for N viewers. With 10 viewers in a
channel the hub egress is ~26 Mbps per share — within range for a
well-hosted hub, painful for a home-server hub. This is the
scaling pain point that motivates v2.

---

## Transport — v2 (WebRTC P2P, future)

Trigger conditions for the v1 → v2 migration:

- Single-hub egress regularly saturates uplink during share sessions.
- Latency complaints from users doing co-op / pair-programming.
- Webcam adoption pushes per-viewer cost up further (two streams).

Sketch:

1. Sharer creates an `RTCPeerConnection` per viewer. Adds the
   `MediaStream` tracks directly (no MediaRecorder).
2. SDP offer/answer + ICE candidates are exchanged over the existing
   hub WS as new envelopes (`ScreenShareSignal { to_pubkey, sdp,
   candidate }`). The hub validates that both ends are members of
   the same channel, then forwards.
3. Video bytes flow direct between peers; the hub carries zero
   media data.
4. Falls back to TURN relay (hub-operated or third-party) for
   peers behind symmetric NATs.

Cost: sharer uploads N copies for N viewers (no SFU yet). An SFU
tier — hub-operated media server — is a v3 problem.

The v1 envelope names (`ScreenShareStart` etc.) are reused for v2
signalling so the client code branches at the transport layer,
not at the protocol layer.

---

## Audio

### Screen audio

Captured via `getDisplayMedia({ audio: true })`. Carries OS-level
output of the selected source (the window's audio for window
capture; system-wide for monitor capture, OS-permitting).

The host's **"Include system audio"** toggle in the source picker
flips the `audio` constraint before calling `getDisplayMedia`. Off
by default.

### macOS caveat

macOS does not expose system audio to `getDisplayMedia` without a
**virtual audio driver** (BlackHole, Loopback, etc.) routing system
output back as an input device. The picker UI on macOS shows a
notice: "System audio capture on macOS requires a virtual audio
driver. See [docs link]."

We do not bundle a driver. We document the well-known options. The
toggle stays available on macOS even without a driver — the call
simply produces silent audio if no driver is present, with no error.

### Webcam mic

Not captured. The user's mic is already in the voice channel via
the cpal pipeline (`shared/voxply-voice/src/capture.rs`). Capturing
it twice would echo or double-mic.

### Viewer volume

Pure client-side: an HTML `<video>` `volume` property on the
viewer's element. No round-trip to the hub, no per-viewer state on
the sharer side. Each viewer's setting is local and persists in
client prefs.

---

## Hub-side changes

| Change | Where |
|---|---|
| New WS envelopes (`ScreenShareStart`/`Chunk`/`Stop` + `*Started`/`ChunkOut`/`Stopped`) | `server/voxply-hub/src/routes/chat_models.rs` (extend the enums at line 175 and 196) |
| Binary frame correlation (envelope ↔ next binary frame) | `server/voxply-hub/src/routes/ws.rs` |
| In-memory `ActiveShare` map per channel | `server/voxply-hub/src/state.rs` (sibling of `voice_channels`) |
| Init-chunk cache per active stream | Same map, fixed-size byte buffer per stream |
| Permission check on `ScreenShareStart` | Reuses existing channel-membership + role check |
| At-most-one-sharer-per-channel enforcement | Reject `ScreenShareStart` if `ActiveShare` exists; allow same sharer to add a second stream (webcam) |

No DB schema changes. No persistent storage. Active-share state
dies with the process; on restart, in-flight shares are dropped
and clients reconnect normally.

The voice channel itself does not change — screen share rides on
the chat WS, not the voice UDP relay. (Putting it on UDP would
require new reliability primitives the WS path gives us for free.)

---

## Client-side — host (sharer) UI

### Share button

A new icon button in the voice channel toolbar (next to mute /
deafen / leave), visible only when the user is connected to that
channel's voice. Clicking it opens the source picker modal.

### Source picker modal

| Element | Behaviour |
|---|---|
| Source list | Output of `getDisplayMedia()` OS picker — monitor and window thumbnails |
| Webcam toggle + device picker | Off by default; lists `enumerateDevices()` videoinput |
| "Include system audio" toggle | Off by default; macOS shows the driver notice when on |
| Quality preset | "Balanced" (v1 default), "Performance" (720p/lower bitrate), "High" (deferred) |
| Confirm button | Triggers `getDisplayMedia` + optional `getUserMedia`, starts the stream |

The OS picker is invoked when the user clicks Confirm, not on modal
open — this is a `getDisplayMedia` requirement (must be a user
gesture).

### Sharing-active UI

While sharing:

- The share button becomes a **Stop sharing** button (red).
- A small indicator strip above the channel content shows
  "You're sharing [Window Title] · [bandwidth] · Stop".
- The sharer does **not** see their own video echoed in the viewer
  panel (loop avoidance).

### Quality / bandwidth indicator

Reads `MediaRecorder.requestData()` size over time, displays a
rough kbps figure. Not a precise meter — just enough for the host
to know if their stream is healthy.

---

## Client-side — viewer UI

The viewer panel mounts inside the channel content area, above or
beside the chat depending on layout. Per-viewer settings persist in
client prefs.

### Layout options

| Layout | Where the video sits |
|---|---|
| Docked top | Above the message list; chat scrolls below |
| Docked side | Left/right of message list; chat fills remaining width |
| Floating overlay | Resizable, draggable popout that stays on top within the app window |
| Fullscreen | Takes over the content area; chat collapses to a sidebar |

Default: docked top. Layout is per-viewer, not enforced by the
sharer or hub.

### Frame size

Within docked layouts, a height slider (or three presets:
small/medium/large) controls how much vertical space the video
takes. The video maintains aspect ratio.

### Multiple streams (screen + webcam)

When both streams are active, the webcam renders as a small
draggable overlay on top of the screen share, **inside the same
panel**. The viewer can hide the webcam overlay without affecting
the screen share.

### Volume slider

Standard slider, 0–100%, persisted per channel. Muted by default
until first interaction (browser autoplay policy). Separate
volume for screen audio and webcam audio if both are present
(though v1 webcam has no audio, so the slider only shows when
screen audio is present).

---

## Permissions

| Default | Behaviour |
|---|---|
| Any approved member of the hub can share into any channel they have voice access to | Matches voice rules — sharing is "richer voice" |
| Admins can restrict sharing to specific roles | New `channel_permissions` flag: `can_screen_share` |
| Hub-wide toggle: "Allow screen share" on/off | Hub settings → Voice |

The permission gate is checked hub-side on `ScreenShareStart`. The
client also greys out the share button when the permission check
would fail (read from the same role/permission state the rest of
the UI uses).

The home hub list (personal-axis) is not involved — screen share
is community traffic, flowing direct between the sharer and the
community hub.

---

## Open questions

- **Record-to-file** (sharer- or hub-side recording for later
  playback). Touches storage, retention, and consent. Deferred —
  not in v1. If added later, sharer-side recording (the sharer's
  own `MediaRecorder` output saved to disk) is the lowest-friction
  shape.
- **Mobile viewing**. Once the mobile client lands, MSE support is
  uneven (iOS Safari historically gated MSE behind specific
  contexts). May force the v2 WebRTC migration earlier on mobile.
- **Max simultaneous sharers per channel**. v1 enforces one. Two
  (e.g. presenter + camera operator) is a plausible v2 ask.
  Decision is deferred until the use case shows up.
- **Codec negotiation**. v1 hardcodes VP8/Opus. If we want VP9 or
  AV1, the `ScreenShareStart` envelope already carries `mime` so
  viewers can refuse incompatible streams — but we have no
  negotiation handshake. Add one if/when a second codec ships.
- **Init-chunk cache eviction**. The hub holds an init segment per
  active stream forever. Bounded by "one sharer per channel" and
  cleared on `ScreenShareStop`, but a sharer who crashes without
  sending Stop leaks the entry until the WS disconnect fires. Use
  the existing WS disconnect handler to clean up.
- **PoW or rate-limit on `ScreenShareStart`**. Spamming starts is
  cheap. Probably fine while permissions gate sharing to members,
  revisit if abused.
