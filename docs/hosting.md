# Hosting a Hub

This is the practical guide for running `voxply-hub` as a real
service — not a dev `cargo run`. For architecture, see
[architecture.md](architecture.md). For the threat model, see
[threat-model.md](threat-model.md).

## What you need

- A Linux server with a public IP (or behind a reverse proxy with one).
- A domain name pointing at the server.
- Two open ports — one for HTTPS, one for voice UDP. Defaults are 3000
  (HTTP/WS) and 3001 (UDP voice). Pick whatever you like.
- TLS certificates if you're terminating TLS at the hub itself
  (Let's Encrypt is fine).
- Disk space for SQLite + attachments. A community-scale hub uses
  modest space; the database file grows roughly with message count
  and inline attachments (each capped at 3 MB).

## Build

```bash
git clone <repo-url> voxply
cd voxply
cargo build --release -p voxply-hub
# Binary lands at target/release/voxply-hub
```

For a server install, copy the binary to `/usr/local/bin/voxply-hub`.

## Configuration

The hub is configured entirely by environment variables — no config
file. The relevant ones:

| Variable                 | Purpose                                       | Default |
|--------------------------|-----------------------------------------------|---------|
| `VOXPLY_HTTP_PORT`       | HTTP/WebSocket port                           | `3000`  |
| `VOXPLY_VOICE_UDP_PORT`  | Voice UDP relay port                          | `3001`  |
| `VOXPLY_TLS_CERT`        | Path to TLS cert (PEM). Enables HTTPS         | unset   |
| `VOXPLY_TLS_KEY`         | Path to TLS private key (PEM). Required with cert | unset |

The hub binds to `0.0.0.0` on both ports. Use a firewall to control
exposure if needed.

The data files (`hub.db` and `hub_identity.json`) live in the process
working directory — there's no env var for this. Set the working
directory in your service unit (`WorkingDirectory=` for systemd) to
control where they go.

## Running directly

For a quick test or a single-machine deployment, just run it:

```bash
VOXPLY_HTTP_PORT=3000 \
VOXPLY_VOICE_UDP_PORT=3001 \
VOXPLY_TLS_CERT=/etc/letsencrypt/live/hub.example/fullchain.pem \
VOXPLY_TLS_KEY=/etc/letsencrypt/live/hub.example/privkey.pem \
/usr/local/bin/voxply-hub
```

The first run creates `hub.db` (SQLite) and `hub_identity.json`
(the hub's Ed25519 keypair) in the working directory.

> **Important**: `hub_identity.json` is the hub's identity. Whoever
> has this file can impersonate the hub. Back it up to safe storage,
> and restrict file permissions to the running user.

## systemd unit

For production, run under systemd. Save this as
`/etc/systemd/system/voxply-hub.service`:

```ini
[Unit]
Description=Voxply hub server
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=voxply
Group=voxply
WorkingDirectory=/var/lib/voxply
ExecStart=/usr/local/bin/voxply-hub
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/voxply
PrivateTmp=true

# Configuration
Environment=VOXPLY_HTTP_PORT=3000
Environment=VOXPLY_VOICE_UDP_PORT=3001
Environment=VOXPLY_TLS_CERT=/etc/letsencrypt/live/hub.example/fullchain.pem
Environment=VOXPLY_TLS_KEY=/etc/letsencrypt/live/hub.example/privkey.pem

[Install]
WantedBy=multi-user.target
```

Then:

```bash
sudo useradd --system --home /var/lib/voxply --shell /usr/sbin/nologin voxply
sudo mkdir -p /var/lib/voxply
sudo chown voxply:voxply /var/lib/voxply
sudo systemctl daemon-reload
sudo systemctl enable --now voxply-hub
sudo systemctl status voxply-hub
```

Logs go to the journal: `journalctl -u voxply-hub -f`.

## TLS

Two ways:

1. **Hub terminates TLS** (simpler, what the env vars above do).
   Point `VOXPLY_TLS_CERT` and `VOXPLY_TLS_KEY` at PEM files. Works
   with Let's Encrypt out of the box; just give the `voxply` user read
   access to the cert files (a `getcert` group or ACL works).

2. **Reverse proxy terminates TLS** (nginx, Caddy, Traefik). Don't
   set the TLS env vars; the hub serves plain HTTP and the proxy
   handles HTTPS. Make sure the proxy forwards WebSocket upgrades.

Voice UDP is a separate port and **bypasses any HTTP proxy**. If you
use a reverse proxy for HTTPS, voice still hits port 3001 directly.
Open it in your firewall.

## Backups

The hub's state is two files:

- `hub.db` — SQLite database (users, channels, messages, alliances,
  everything except media URLs).
- `hub_identity.json` — Ed25519 keypair (the hub's federation identity).

Both live in the process working directory. A nightly cron is enough
for most communities:

```bash
#!/bin/sh
# /usr/local/bin/voxply-backup
set -e
BACKUP_DIR="/var/backups/voxply/$(date +%Y%m%d-%H%M)"
mkdir -p "$BACKUP_DIR"
sqlite3 /var/lib/voxply/hub.db ".backup '$BACKUP_DIR/hub.db'"
cp /var/lib/voxply/hub_identity.json "$BACKUP_DIR/"
# Optional: keep last 30 days
find /var/backups/voxply -maxdepth 1 -type d -mtime +30 -exec rm -rf {} +
```

Use `sqlite3 .backup` (not `cp`) so the snapshot is consistent with
the running server.

If you lose `hub_identity.json` and have no backup: the hub gets a
new identity on next start. Federation peers will see it as a
different hub; existing alliance memberships stop working until peers
re-add this hub under its new key.

## Upgrades

```bash
cd /path/to/voxply
git pull
cargo build --release -p voxply-hub
sudo systemctl stop voxply-hub
sudo install -o root -g root -m 755 \
  target/release/voxply-hub /usr/local/bin/voxply-hub
sudo systemctl start voxply-hub
```

Database migrations run automatically on startup. They're additive
(`CREATE TABLE IF NOT EXISTS` + `ALTER TABLE`); no down-migrations.
For a major change you might want to take a backup first; for minor
versions the migration is a no-op or new column.

If you need to apply migrations explicitly without starting the
server (rare), use the CLI: `voxply-hub migrate`.

## Health check

```bash
curl https://hub.example/health
# Returns plain "OK" with HTTP 200 when healthy.

curl https://hub.example/info
# Returns hub name, description, icon URL, public key. Useful for
# verifying the hub is reachable and identifying its federation key.
```

A monitoring system (Uptime Kuma, Prometheus blackbox, etc.) hitting
`/health` every minute is enough to know the hub is alive.

## Federation considerations

- Alliance peers will reach this hub at the URL you give them. If you
  put the hub behind Cloudflare or similar, peers see Cloudflare's
  fingerprint, not yours.
- The voice UDP port must be reachable from clients (and other hubs
  in alliance voice scenarios — not yet shipped).
- Outbound: the hub initiates connections to peer hubs for federation
  (alliance reads, DM delivery). Egress firewall rules need to allow
  HTTPS to peer URLs.

## What this guide does NOT cover

- Multi-tenant hub hosting on one server (the future "farm model" —
  see [farm-model.md](farm-model.md)).
- Auto-scaling / clustering. The hub is single-process by design.
- E2E encryption setup for DMs (not implemented yet — see [threat-model.md](threat-model.md)).
- Bot management (not implemented yet).
