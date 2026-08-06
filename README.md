[中文](./README-zh.md) | English

# Rustracker

[![Version](https://img.shields.io/github/v/release/LOSRET/Rustracker?color=blue&label=version)](https://github.com/LOSRET/Rustracker/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021--edition-orange.svg)](https://www.rust-lang.org)

A lightweight, high-performance HTTP BitTorrent tracker with a real-time dashboard, written in Rust, developed using the Vibe Coding approach.

## Highlights

**Core Protocol**
- BEP 3 compliant `announce` and `scrape` endpoints
- Compact IPv4/IPv6 peer encoding
- Bencoded tracker responses
- Configurable announce interval and peer timeout

**Real-time Dashboard**
- Web dashboard served on the same HTTP port — no separate frontend needed
- ECharts trend charts: Torrents, Peers, Seeders, Leechers over 24h / 3d / 7d
- Top 100 Torrents page ranked by Peers / Seeders / Leechers / Downloaded
- Top 15 client distribution chart over time
- Multilingual UI (中文 / English / 日本語 / Русский / Deutsch / Українська) with auto-detection

**Operations**
- 64-shard concurrent tracker pool for high throughput
- 102 BitTorrent client identification (qBittorrent, Transmission, µTorrent, Aria2, 迅雷, etc.)
- Hot-reload torrent blacklist — edit the file, no restart needed
- Optional trend data persistence to JSONL (7-day retention, 10-min sampling)
- Graceful shutdown on Ctrl+C / SIGTERM
- Structured logging via `tracing` with `RUST_LOG` env filter

**Deployment**
- Single binary, zero external dependencies
- Linux `systemd` installer with interactive Chinese menu
- GitHub Actions CI/CD: auto-build & release on version bump

## Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.85+ (edition 2021)
- [Node.js](https://nodejs.org/) 20+ and npm (only needed when building with the `dashboard` feature, which is on by default)

### Build & Run

```bash
git clone https://github.com/LOSRET/Rustracker.git
cd rustracker
npm install --prefix frontend   # first time only — installs dashboard build dependencies
cargo run --release -- --listen 127.0.0.1:8080
```

Open `http://127.0.0.1:8080` in your browser to see the dashboard.

> Building without the dashboard (`cargo build --release --no-default-features`) does not require Node.js/npm.

### Pre-built Binaries

Download the latest release from [GitHub Releases](https://github.com/LOSRET/Rustracker/releases):
- `rustracker.exe` — Windows x86_64
- `rustracker-linux` — Linux x86_64
- `rustracker-linux.tar.gz` — Linux archive with installer

## CLI Reference

| Flag | Env Variable | Default | Description |
|------|-------------|---------|-------------|
| `--listen` | `RUSTRACKER_LISTEN` | `[::]:8080` (Linux); `[::]:8080` + `0.0.0.0:8080` (Windows) | Socket address(es) to bind (repeatable, platform-specific dual-stack default) |
| `--interval-secs` | `RUSTRACKER_INTERVAL_SECS` | `1800` | Announce interval (seconds) |
| `--peer-timeout-secs` | `RUSTRACKER_PEER_TIMEOUT_SECS` | `3000` | Peer expiry timeout (seconds) |
| `--blacklist` | `RUSTRACKER_BLACKLIST` | — | Path to torrent blacklist file |
| `--trends-file` | `RUSTRACKER_TRENDS_FILE` | — | Path to persist trend JSONL data |
| `--admin-token` | `RUSTRACKER_ADMIN_TOKEN` | — | Bearer token required for admin API endpoints |

Every flag can be set via environment variable or command-line argument. Command-line takes precedence.

```bash
# Example: environment variables
export RUSTRACKER_LISTEN=0.0.0.0:6969
export RUSTRACKER_INTERVAL_SECS=900
cargo run --release
```

## API Endpoints

### Health Check

```
GET /healthz
```

Response: `200 OK` with body `ok`.

### Announce

```
GET /announce?info_hash=<20-byte>&peer_id=<20-byte>&port=6881&uploaded=0&downloaded=0&left=0&event=started&compact=1
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `info_hash` | Yes | 20-byte percent-encoded torrent info hash |
| `peer_id` | Yes | 20-byte percent-encoded peer identifier |
| `port` | Yes | Peer's listening port |
| `uploaded` | No | Total bytes uploaded |
| `downloaded` | No | Total bytes downloaded |
| `left` | No | Bytes remaining to complete |
| `event` | No | `started`, `completed`, `stopped`, or empty |
| `compact` | No | Compact encoding. Always compact — the value is accepted and ignored; only compact (binary) peer lists are returned (BEP 3 `compact=0` dictionary form is not supported) |
| `numwant` | No | Number of peers to return (default 100, max 400) |
| `ip` | No | Override peer IP address (for reverse proxy setups) |

Response (bencoded):

```
d8:completei5e10:downloadedi0e10:incompletei3e8:intervali1800e5:peers60:...(compact binary)...e
```

Compact peer format: each IPv4 peer is 6 bytes (4-byte IP + 2-byte port, big-endian). IPv6 peers use 18 bytes each and are returned in `peers6`.

### Scrape

```
GET /scrape?info_hash=<20-byte>[&info_hash=...]
```

Multiple `info_hash` parameters are supported. Response (bencoded):

```
d5:filesd20:<info_hash>d8:completei5e10:downloadedi10e10:incompletei3eeee
```

### Dashboard API

| Endpoint | Description |
|----------|-------------|
| `GET /api/stats` | Real-time counts: peers, seeders, leechers, torrents, completed, rps, version, uptime |
| `GET /api/trends` | Historical trend data (7-day retention, 10-min sampling) |
| `GET /api/clients` | Top 15 client types with time-series history |
| `GET /api/clients/list` | All connected client types sorted by peer count (current snapshot) |
| `GET /api/top100?limit=100` | Top 100 torrents ranked by peers / seeders / leechers / downloaded (`limit` max 500) |

All return JSON.

## Web Dashboard

The tracker serves a full-featured dashboard at `/` on the same port:

- **Overview page** — live counts of Peers, Seeders, Leechers, Torrents, and Completed downloads
- **Trend chart** — interactive ECharts graph with 24h / 3d / 7d range selector
- **Client chart** — top 15 BitTorrent clients by peer count over time
- **Clients page** — sortable table of all connected client types and their current peer counts (powered by `/api/clients/list`)
- **Top 100 page** — sortable table of the most active torrents
- **Disclaimer** — built-in legal disclaimer for public-facing deployments
- **i18n** — automatic detection across 6 languages (中文 / English / 日本語 / Русский / Deutsch / Українська) with manual toggle

Hashed assets under `/assets/*` (JS/CSS/fonts emitted by Vite, content-addressed by filename) are served with `Cache-Control: public, max-age=31536000, immutable`. The `index.html` entry is rebuilt on every release.

## Torrent Blacklist

Create a text file with one 40-character hex `info_hash` per line:

```
# blocked torrent
e09b1c0c4b174ef2b25c8de662941777fb3f2d7a
```

Pass the path via `--blacklist blacklist.txt` or `RUSTRACKER_BLACKLIST`.

- Announce requests for blacklisted torrents return a bencoded failure response
- Scrape results silently exclude blacklisted torrents
- The file is watched every 5 seconds — edit and save, no restart needed
- Invalid lines are logged as warnings and skipped

### Admin API

When `RUSTRACKER_ADMIN_TOKEN` is configured, you can query whether an entry is already blacklisted:

```bash
curl "http://127.0.0.1:8080/api/blacklist?info_hash=0123456789abcdef0123456789abcdef01234567" \
  -H "Authorization: Bearer $RUSTRACKER_ADMIN_TOKEN"
```

The response includes `"blacklisted": true` or `"blacklisted": false` and does not modify the blacklist file.

When both `RUSTRACKER_BLACKLIST` and `RUSTRACKER_ADMIN_TOKEN` are configured, you can add entries through the authenticated admin endpoint:

```bash
curl -X POST http://127.0.0.1:8080/api/blacklist \
  -H "Authorization: Bearer $RUSTRACKER_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"info_hash":"0123456789abcdef0123456789abcdef01234567"}'
```

The endpoint appends the 40-character hex `info_hash` to the blacklist file first, then updates the in-memory blacklist. Duplicate entries return success with `"added": false`.

## Trend Data Persistence

By default, trend data (torrents, peers, seeders, leechers, client distribution) lives in memory and is lost on restart. To persist:

```bash
cargo run --release -- --trends-file /var/lib/rustracker/trends.jsonl
```

- Data is sampled every 10 minutes and retained for 7 days
- Two JSONL files are created:
  - `<path>` (e.g. `trends.jsonl`) — torrents/peers/seeders/leechers per timestamp
  - `top_clients.jsonl` in the same directory — client distribution per timestamp
- On restart, existing data is loaded from disk automatically
- The Linux installer enables this by default at `/var/lib/rustracker/trends.jsonl`

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                    Axum HTTP Server                   │
│  /announce  /scrape  /healthz  /  /api/*             │
└──────────┬───────────────────────────────────────────┘
           │
           ▼
┌──────────────────────┐     ┌─────────────────────────┐
│    TrackerPool        │     │     TrendStore           │
│  (64 sharded RwLock)  │     │  (7-day JSONL history)   │
│                       │     │  10-min sampling          │
│  ┌─────────────────┐  │     └─────────────────────────┘
│  │ Tracker shard 0  │  │
│  │  BTreeMap<       │  │     ┌─────────────────────────┐
│  │   InfoHash,      │  │     │   Blacklist Watcher      │
│  │   Swarm          │  │     │  (5-sec file reload)     │
│  │  >               │  │     │  HashSet<InfoHash>        │
│  └─────────────────┘  │     └─────────────────────────┘
│  ... (×64 shards)     │
└───────────────────────┘
```

- **Sharding**: The tracker pool uses 64 shards with per-shard `RwLock` to minimize contention under high concurrency
- **Peer storage**: Packed binary format for IPv4 (6 bytes/peer) and IPv6 (18 bytes/peer) — no heap allocation per peer
- **Expiry**: Background task sweeps expired peers every 1 second
- **Client ID**: Compile-time 256×256 lookup table for Azureus-style peer ID prefixes, plus prefix matching for non-standard formats

## Project Structure

```
rustracker/
├── src/
│   ├── core/          # Tracker engine (no I/O): types, tracker, swarm, topk, counters
│   ├── protocol/      # BT protocol: bencode, announce, client_id (102 clients)
│   └── server/        # HTTP layer: handlers, admin, pool (64 shards), blacklist, trends
├── frontend/          # Vue 3 + Vite + Tailwind dashboard SPA (embedded at compile time)
├── examples/          # Load testing and benchmarking tools
├── tests/             # Integration tests
├── build.rs           # Builds frontend, embeds dist/ into the binary
└── install-linux.sh   # Linux systemd installer
```

Three-layer design: `core` (pure engine) → `protocol` (BT encoding) → `server` (Axum HTTP). See `CLAUDE.md` for detailed module responsibilities.

## Client Identification

The tracker recognizes **102 BitTorrent clients** from peer ID prefixes, including:

| Category | Clients |
|----------|---------|
| Mainstream | qBittorrent, Transmission, µTorrent, BitTorrent, Deluge, Vuze, BiglyBT |
| Lightweight | Aria2, libtorrent, rTorrent, KTorrent, FrostWire |
| Web-based | WebTorrent, Brave |
| Legacy | FlashGet, GetRight, LimeWire, Shareaza |
| Chinese | 迅雷 (Thunder), QQ旋风, 百度网盘 |
| Other | Tixati, Halite, BitComet, BitSpirit, MLDonkey |

Client tags are exposed in the `/api/clients` endpoint and the dashboard's client distribution chart.

## Linux Installation

Release packages include `install-linux.sh`. Place the Linux binary and the script in the same directory, then run `sudo sh install-linux.sh` for an interactive menu (install/update, start/stop/restart, config, system tuning). Non-interactive commands like `install`, `start`, `stop`, `status` are also supported.

**Default file layout after installation:**

| Path | Description |
|------|-------------|
| `/opt/rustracker/rustracker` | Binary |
| `/etc/rustracker.env` | Environment config, including the generated Admin Token |
| `/etc/rustracker/blacklist.txt` | Torrent blacklist |
| `/var/lib/rustracker/trends.jsonl` | Trend data |
| `/etc/systemd/system/rustracker.service` | systemd unit |

## Load Testing

Built-in benchmarking tools are in `examples/`:

```bash
cargo run --release --example announce_load -- 2000 200 100   # simple load test
cargo run --release --example load_test -- --duration 60 --concurrency 500  # advanced (Zipf)
cargo run --release --example unified_bench    # RPS, RSS, CPU, latency
cargo run --release --example shrink_bench     # memory shrink/regrow cycles
```

See `CLAUDE.md` for the full list of memory and benchmark examples.

## Development

### Build

```bash
cargo build --release
```

### Build without Dashboard

To compile a pure tracker binary without the embedded web UI (smaller binary, no HTML/CSS/JS bundled):

```bash
cargo build --release --no-default-features
```

This disables the `dashboard` feature — the `/` and `/assets/{*name}` routes are excluded at compile time. All tracker protocol endpoints (`/announce`, `/scrape`, `/healthz`, `/api/*`) remain fully functional.

### Test

```bash
cargo test
```

### Cross-compile for Linux (from Windows)

The project includes `.cargo/config.toml` with a zig-cc toolchain for musl static linking:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

### Logging

Control log verbosity with the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug cargo run --release
RUST_LOG=rustracker=trace cargo run --release
```

## License

[MIT](./LICENSE)

## Acknowledgments

- [opentracker](https://erdgeist.org/arts/software/opentracker/) by Dirk Engling — peer selection algorithm design inspired by opentracker's fixed-point even-spacing random selection strategy. Licensed under [Beerware](https://erdgeist.org/beerware.html).
- [PBH-BTN/quick-references](https://github.com/PBH-BTN/quick-references) — BitTorrent client identification peer_id reference table.
