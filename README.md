[中文](./README-zh.md) | English

> **AI agents:** for a Linux production install runbook that asks for the required settings, see [agent-install.md](./agent-install.md).

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
- [Node.js](https://nodejs.org/) 20.19+ or 22.12+ and npm (only needed when building with the `dashboard` feature, which is on by default)

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
| `--admin-token` | `RUSTRACKER_ADMIN_TOKEN` | — | Bearer token required for admin API and MCP endpoints |
| `--no-mcp` | `RUSTRACKER_NO_MCP` | `false` | Disable `/mcp` even when an admin token is configured |
| `--trust-proxy-headers` | `RUSTRACKER_TRUST_PROXY_HEADERS` | `false` | Trust `CF-Connecting-IP` / `X-Real-IP` / `X-Forwarded-For` for the announce client IP; enable only behind a trusted proxy |

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

Multiple `info_hash` parameters are supported (up to 200; requests with more return a bencoded `400` failure). Response (bencoded):

```
d5:filesd20:<info_hash>d8:completei5e10:downloadedi10e10:incompletei3eeee
```

### Dashboard API

| Endpoint | Description |
|----------|-------------|
| `GET /api/stats` | Real-time counts: peers, seeders, leechers, torrents, completed, tracker_rps, version, uptime |
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

List entries (sorted, paginated; `limit` defaults to 100 and is capped at 1000):

```bash
curl "http://127.0.0.1:8080/api/blacklist?limit=100&offset=0" \
  -H "Authorization: Bearer $RUSTRACKER_ADMIN_TOKEN"
```

The response is `{"ok":true,"total":1,"offset":0,"limit":100,"entries":["0123..."],"error":null}`.

Remove an entry:

```bash
curl -X DELETE "http://127.0.0.1:8080/api/blacklist?info_hash=0123456789abcdef0123456789abcdef01234567" \
  -H "Authorization: Bearer $RUSTRACKER_ADMIN_TOKEN"
```

Removal rewrites the file atomically. `"removed": true` means an entry existed; `"removed": false` means the call was already a no-op.

Every add/remove attempt is appended to `<blacklist-stem>.audit.jsonl` next to the blacklist file (for example, `blacklist.txt` → `blacklist.audit.jsonl`), one JSON object per line: timestamp, action, info_hash, source (`http` or `mcp`), result, and client IP when available.

The blacklist file is designed for a single tracker process. If multiple instances share it, coordinate writes externally; removal performs a read-modify-write and has no cross-process lock.

### MCP endpoint

When `RUSTRACKER_ADMIN_TOKEN` is set, the tracker also exposes an MCP server at `/mcp` using the Streamable HTTP transport (stateless, JSON responses). It reuses the admin Bearer token and provides four tools:

| Tool | Description |
|------|-------------|
| `blacklist_check` | Check whether an info_hash is blacklisted (read-only) |
| `blacklist_add` | Add an info_hash (idempotent) |
| `blacklist_list` | List info_hashes with `offset`/`limit` |
| `blacklist_remove` | Remove an info_hash (idempotent) |

The endpoint returns server `instructions` during MCP initialization, so a client knows what the four tools do and that `info_hash` is a 40-character hex string without extra prompting.

Successful calls return the tool's `structuredContent` (`blacklisted`, `added`, `removed`, `entries`, …). There is no `ok` field: a failed call is reported the MCP way — `isError: true` plus a text message explaining the failure (malformed `info_hash`, or `blacklist file is not configured` when the tracker was started without `--blacklist`).

Disable it with `--no-mcp` or `RUSTRACKER_NO_MCP=1`. Tokens shorter than 32 characters trigger a startup warning.

### Client setup

`/mcp` speaks MCP Streamable HTTP and authenticates with the same admin token as the REST admin API — send it as a static request header:

```http
Authorization: Bearer <admin-token>
```

The endpoint does not implement OAuth, so configure that header as a static credential rather than signing in.

Requests must look like this:

```http
POST /mcp
Authorization: Bearer <admin-token>
Content-Type: application/json
Accept: application/json, text/event-stream
```

The `Accept` header has to list **both** media types even though the server only ever replies with JSON — that is part of the MCP Streamable HTTP transport, and a client that sends `Accept: application/json` alone is rejected with `406 Not Acceptable`. A non-JSON `Content-Type` is rejected with `415 Unsupported Media Type`. The `MCP-Protocol-Version` header is optional; if you send it, it must be a version the server knows, and on `initialize` it must match the `protocolVersion` in the request body.

Client configuration syntax changes between releases — OpenCode, for example, moved server entries under `mcp.servers` in v2 — so take the snippet from the client's own documentation instead of copying one from here:

- [OpenCode — MCP servers](https://opencode.ai/v2/docs/mcp-servers)
- [Claude Code — Connect to tools via MCP](https://code.claude.com/docs/en/mcp)
- [DeepSeek Harness — MCP client (`@deepseek-ai/dsh-mcp-client`)](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/mcp/mcp-client/README.md)
- [Other clients — Connect to remote MCP servers](https://modelcontextprotocol.io/docs/develop/connect-remote-servers)

MCP is a write-capable admin surface: use HTTPS and a strong token, and keep `/mcp` behind your existing reverse proxy or Cloudflare setup. Host-header validation is intentionally permissive so public deployments work; the bearer token is the access boundary.

## Trend Data Persistence

By default, trend data (torrents, peers, seeders, leechers, client distribution) lives in memory and is lost on restart. To persist:

```bash
cargo run --release -- --trends-file /var/lib/rustracker/trends.jsonl
```

- Data is sampled every 10 minutes and retained for 7 days
- Two JSONL files are created:
  - `<path>` (e.g. `trends.jsonl`) — torrents/peers/seeders/leechers per timestamp
  - `<stem>.clients.jsonl` next to it (e.g. `trends.clients.jsonl`) — client distribution per timestamp. A legacy `top_clients.jsonl` is still read on upgrade, but no longer written.
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
│  │  >               │  │     │  BTreeSet<InfoHash>       │
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
│   └── server/        # HTTP layer: handlers, admin, mcp, pool (64 shards), blacklist, audit, trends
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

Release packages include `install-linux.sh`. Place the Linux binary and the script in the same directory, then run `sudo sh install-linux.sh` for an interactive menu (install/update, start/stop/restart, config, system tuning). Service commands like `start`, `stop`, and `status` are also supported.

For unattended installs, run `sudo sh install-linux.sh install --yes`. Defaults can be overridden with `RUSTRACKER_LISTEN`, `RUSTRACKER_INTERVAL_SECS`, `RUSTRACKER_PEER_TIMEOUT_SECS`, `RUSTRACKER_TRENDS_FILE`, `RUSTRACKER_TRUST_PROXY_HEADERS`, and `RUSTRACKER_NO_MCP`. See [agent-install.md](./agent-install.md) for the full runbook.

The Linux installer enables `RUSTRACKER_TRUST_PROXY_HEADERS` by default. Set it to `false` when the tracker is directly exposed to the internet without a trusted reverse proxy.

The installer asks whether to enable the `/mcp` endpoint. Choosing no writes `RUSTRACKER_NO_MCP=true` to `/etc/rustracker.env`; the default keeps MCP enabled. Both `true`/`false` and `1`/`0` are accepted.

**Default file layout after installation:**

| Path | Description |
|------|-------------|
| `/opt/rustracker/rustracker` | Binary |
| `/etc/rustracker.env` | Environment config, including the generated Admin Token |
| `/etc/rustracker/blacklist.txt` | Torrent blacklist |
| `/etc/rustracker/blacklist.audit.jsonl` | Blacklist mutation audit log (created on first write) |
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
