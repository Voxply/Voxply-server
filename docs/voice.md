# Voice

Real-time voice over UDP, Opus-encoded, with RNNoise denoise and voice
activity detection. Lives in `shared/voxply-voice` so client and server
agree on the wire format.

## Pipeline

```
mic capture (cpal)
   ↓
RNNoise denoise + VAD
   ↓
Opus encode
   ↓
UDP packet (server/voxply-hub UDP relay)
   ↓
Opus decode
   ↓
playback (cpal)
```

## Files

| Stage              | File |
|--------------------|------|
| Pipeline orch.     | `shared/voxply-voice/src/pipeline.rs` |
| Audio capture      | `shared/voxply-voice/src/capture.rs` |
| Denoise + VAD      | `shared/voxply-voice/src/denoise.rs` |
| Opus codec         | `shared/voxply-voice/src/codec.rs` |
| UDP transport      | `shared/voxply-voice/src/transport.rs` |
| Wire protocol      | `shared/voxply-voice/src/protocol.rs` |
| Audio output       | `shared/voxply-voice/src/playback.rs` |
| Device enumeration | `shared/voxply-voice/src/devices.rs` |

## Why UDP, not WebRTC

- Predictable latency under loss (we control retransmission policy: none).
- Smaller dependency footprint.
- We already have hub identity for auth — we don't need DTLS-SRTP machinery.

## Why RNNoise + VAD

- RNNoise is small, real-time, and good enough for voice.
- VAD avoids transmitting silence (saves bandwidth + reduces background
  noise on the channel).

## Hub-side relay

The hub's UDP listener (default port 3001) receives encrypted/signed Opus
frames from users currently in voice on a channel and fans them out to
the other connected users on that channel. Frames are not transcoded;
the hub is just an SFU-style relay.

> Note: there's no separate "voice channel" type. Every Voxply channel
> is both text and voice — joining voice is something a user does
> *in* a channel, not a property of the channel itself. See
> [decisions.md](decisions.md).

## Self-mute / self-deafen

Client-side. Self-mute stops capture; self-deafen stops decoding incoming
streams. Neither involves the hub — it's purely UI state. (Hub-side
mute, e.g. moderator mute, is a different mechanism — see roles and
moderation.)

## What's not done

- E2E encryption between voice participants (today the hub sees frames
  as it relays them — see [`threat-model.md`](threat-model.md))
- Cross-hub voice (alliance-wide voice rooms)
- Per-user gain / spatial audio
