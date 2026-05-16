[中文](./README-zh.md) | English

# rustracker

[![Version](https://img.shields.io/github/v/release/LOSRET/rustracker?color=blue&label=version)](https://github.com/LOSRET/rustracker/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021--edition-orange.svg)](https://www.rust-lang.org)

A lightweight, high-performance HTTP BitTorrent tracker with a real-time dashboard, written in Rust.

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
- Bilingual UI (中文 / English) with auto-detection

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

- [Rust](https://www.rust-lang.org/tools/install) 1.74+ (edition 2021)

### Build & Run

```bash
git clone https://github.com/LOSRET/rustracker.git
cd rustracker
cargo run --release -- --listen 127.0.0.1:8080
```

Open `http://127.0.0.1:8080` in your browser to see the dashboard.

### Pre-built Binaries

Download the latest release from [GitHub Releases](https://github.com/LOSRET/rustracker/releases):
- `rustracker.exe` — Windows x86_64
- `rustracker-linux` — Linux x86_64
- `rustracker-linux-vX.Y.Z.tar.gz` — Linux archive with installer

## CLI Reference

| Flag | Env Variable | Default | Description |
|------|-------------|---------|-------------|
| `--listen` | `RUSTRACKER_LISTEN` | `0.0.0.0:8080` | Socket address to bind |
| `--interval-secs` | `RUSTRACKER_INTERVAL_SECS` | `1800` | Announce interval (seconds) |
| `--peer-timeout-secs` | `RUSTRACKER_PEER_TIMEOUT_SECS` | `3000` | Peer expiry timeout (seconds) |
| `--blacklist` | `RUSTRACKER_BLACKLIST` | — | Path to torrent blacklist file |
| `--trends-file` | `RUSTRACKER_TRENDS_FILE` | — | Path to persist trend JSONL data |

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
| `compact` | No | `1` for compact encoding (default), `0` for dictionary |
| `numwant` | No | Number of peers to return (default 100, max 400) |

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

### Stats API

```
GET /api/stats
```

Returns JSON:

```json
{
  "interval": 1800,
  "peer_timeout": 3000,
  "torrents": 42,
  "peers": 128,
  "seeders": 85,
  "leechers": 43,
  "completed": 310,
  "history": [
    {"timestamp": 1715800000, "torrents": 40, "peers": 120, "seeders": 80, "leechers": 40}
  ]
}
```

### Client Distribution

```
GET /api/clients
```

Returns JSON with the top 15 client types and their historical peer counts.

### Top 100 Torrents

```
GET /api/top100?sort=peers
```

| Query | Values | Default |
|-------|--------|---------|
| `sort` | `peers`, `seeders`, `leechers`, `downloaded` | `peers` |

Returns JSON with the top 100 active torrents ranked by the requested metric.

## Web Dashboard

The tracker serves a full-featured dashboard at `/` on the same port:

- **Overview page** — live counts of Peers, Seeders, Leechers, Torrents, and Completed downloads
- **Trend chart** — interactive ECharts graph with 24h / 3d / 7d range selector
- **Client chart** — top 15 BitTorrent clients by peer count over time
- **Top 100 page** — sortable table of the most active torrents
- **Disclaimer** — built-in legal disclaimer for public-facing deployments
- **i18n** — automatic Chinese / English detection with manual toggle

Static assets (`style.css`, `app.js`) are cached for 1 hour by the server.

## Torrent Blacklist

Create a text file with one 40-character hex `info_hash` per line:

```
# blocked torrent
e09b1c0c4b174ef2b25c8de662941777fb3f2d7a
```

Pass the path via `--blacklist blacklist.txt` or `RUSTRACKER_BLACKLIST`.

- Announce requests for blacklisted torrents are rejected with HTTP 403
- Scrape results silently exclude blacklisted torrents
- The file is watched every 5 seconds — edit and save, no restart needed
- Invalid lines are logged as warnings and skipped

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
├── Cargo.toml                     # Rust project manifest and dependencies
├── LICENSE                        # MIT license
├── README.md                      # English documentation
├── README-zh.md                   # Chinese documentation
├── install-linux.sh               # Linux systemd installer (interactive menu)
│
├── src/                           # Rust source code
│   ├── main.rs                    # Entry point: CLI parsing, Tokio runtime, graceful shutdown
│   ├── lib.rs                     # Library root: re-exports all modules
│   ├── server.rs                  # Axum HTTP server, routing, AppState, static asset caching
│   ├── handlers.rs                # HTTP handlers: announce, scrape, healthz, dashboard, APIs
│   ├── tracker.rs                 # TrackerPool — 64-shard concurrent tracker (RwLock<BTreeMap>)
│   ├── swarm.rs                   # Swarm — per-torrent peer set with packed binary storage
│   ├── protocol.rs                # BEP 3 protocol: bencode response construction, compact encoding
│   ├── bencode.rs                 # Lightweight bencode encoder (zero external dependency)
│   ├── types.rs                   # Core types: InfoHash, PeerId, PeerAddr
│   ├── client_id.rs               # 102-client peer ID identification (compile-time lookup table)
│   ├── blacklist.rs               # Torrent blacklist with 5-sec hot-reload file watcher
│   └── trends.rs                  # Trend data collection, 7-day JSONL persistence, history API
│
├── assets/                        # Web dashboard static files
│   ├── index.html                 # Dashboard HTML (production, inlined CSS/JS)
│   ├── index-build.html           # Dashboard HTML (development, external assets)
│   ├── style.css                  # Dashboard styles
│   └── app.js                     # Dashboard logic: ECharts charts, i18n, API calls
│
├── examples/                      # Load testing and benchmarking tools
│   ├── announce_load.rs           # Simple concurrent announce load test
│   ├── load_test.rs               # Advanced load test (Zipf distribution, peer lifecycle)
│   ├── rps_bench.rs               # Requests-per-second benchmark
│   ├── unified_bench.rs           # Unified benchmark suite
│   ├── memory_tracker_bench.rs    # Memory usage benchmark
│   ├── memory_tracker_btree.rs    # BTree memory layout analysis
│   ├── memory_staircase_test.rs   # Memory staircase growth test
│   └── memory_ci_compare.rs       # CI memory regression comparison
│
└── tests/                         # Integration tests
    └── tracker_http.rs            # HTTP endpoint integration tests
```

**Source module overview:**

| Module | Responsibility |
|--------|---------------|
| `main.rs` | CLI argument parsing (`clap`), async runtime setup, signal handling |
| `server.rs` | Axum router, shared `AppState`, 1-hour static asset cache headers |
| `handlers.rs` | Request handlers for `/announce`, `/scrape`, `/healthz`, `/api/*`, `/` |
| `tracker.rs` | 64-shard `TrackerPool` — per-shard `RwLock<BTreeMap<InfoHash, Swarm>>` |
| `swarm.rs` | Per-torrent peer collection — packed binary IPv4 (6B) / IPv6 (18B) per peer |
| `protocol.rs` | BEP 3 compliant bencode response builder, compact peer encoding |
| `bencode.rs` | Minimal bencode serializer — no external crate dependency |
| `types.rs` | `InfoHash` (20 bytes), `PeerId` (20 bytes), `PeerAddr` (IP + port) |
| `client_id.rs` | Compile-time 256×256 lookup table for `-XX####-` Azureus peer ID prefixes |
| `blacklist.rs` | `HashSet<InfoHash>` hot-reload via 5-second file polling |
| `trends.rs` | In-memory ring buffer + optional JSONL persistence, 7-day retention |

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

Release packages include `install-linux.sh`. Place the Linux binary and the script in the same directory, then:

```bash
sudo sh install-linux.sh
```

The installer provides a Chinese interactive menu for:

1. Install or update
2. Uninstall
3. Start / Stop / Restart service
4. View status
5. View configuration
6. Modify configuration (listen address, interval, timeout)

Non-interactive commands:

```bash
sudo sh install-linux.sh install
sudo sh install-linux.sh status
sudo sh install-linux.sh configure
sudo sh install-linux.sh restart
sudo sh install-linux.sh uninstall
```

**Default file layout after installation:**

| Path | Description |
|------|-------------|
| `/opt/rustracker/rustracker` | Binary |
| `/etc/rustracker.env` | Environment config |
| `/etc/rustracker/blacklist.txt` | Torrent blacklist |
| `/etc/rustracker/trends.jsonl` | Trend data |
| `/etc/systemd/system/rustracker.service` | systemd unit |

For the listen address, entering only a port (e.g. `6969`) is accepted and saved as `0.0.0.0:6969`.

## Load Testing

Two built-in examples for benchmarking:

### Simple Load Test

```bash
cargo run --release --example announce_load -- 2000 200 100
#                                       total concurrency torrents
```

Duration mode:

```bash
cargo run --release --example announce_load -- \
  --duration-secs 30 --concurrency 200 --torrents 100
```

### Advanced Load Test

```bash
cargo run --release --example load_test -- \
  --duration 60 \
  --concurrency 500 \
  --torrents 1000 \
  --peers 50000 \
  --scrape-weight 1 \
  --announce-weight 5 \
  --keep-alive \
  --progress-interval 5
```

Features: Zipf distribution for realistic torrent popularity, peer lifecycle events (started/completed/stopped), 40% seeder ratio, latency percentiles (p50/p95/p99).

## Development

### Build

```bash
cargo build --release
```

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
