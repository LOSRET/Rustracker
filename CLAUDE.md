# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Behavioral Guidelines

> Derived from [Andrej Karpathy's observations](https://x.com/karpathy/status/2015883857489522876) on LLM coding pitfalls. These bias toward caution over speed; for trivial tasks, use judgment.

### 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them — don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

### 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it — don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

### 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

## Project Overview

Rustracker is a lightweight, high-performance HTTP BitTorrent tracker written in Rust. It implements BEP 3 compliant `announce` and `scrape` endpoints, serves a real-time web dashboard on the same port, and identifies 102 BitTorrent clients from peer ID prefixes.

**Requirements:** Rust 1.85+ (edition 2021)

## Build & Development Commands

```bash
# Build (release, includes embedded dashboard)
cargo build --release

# Build without dashboard (smaller binary, no HTML/CSS/JS bundled)
cargo build --release --no-default-features

# Run
cargo run --release -- --listen 127.0.0.1:8080

# Run tests
cargo test

# Run a single test
cargo test <test_name>

# Cross-compile for Linux from Windows (uses zig-cc musl toolchain configured in .cargo/config.toml)
cargo build --release --target x86_64-unknown-linux-musl

# Logging
RUST_LOG=debug cargo run --release
RUST_LOG=rustracker=trace cargo run --release

# Validation (run before committing)
cargo fmt --check                          # style check
cargo clippy --all-targets --all-features  # lints
```

## Load Testing / Benchmarks

```bash
# Simple load test (total, concurrency, torrents)
cargo run --release --example announce_load -- 2000 200 100

# Advanced load test with Zipf distribution
cargo run --release --example load_test -- --duration 60 --concurrency 500 --torrents 1000 --peers 50000

# RPS benchmark (single-task mixed traffic)
cargo run --release --example rps_bench

# Unified benchmark (RPS, RSS, CPU, latency)
cargo run --release --example unified_bench

# Memory benchmarks
cargo run --release --example memory_tracker_bench
cargo run --release --example memory_ci_compare
```

## Architecture

Three-layer design with clear separation of concerns:

- **`core/`** — Pure tracker engine, no I/O. `Tracker` holds a `BTreeMap<InfoHash, Swarm>` where each `Swarm` stores peers in packed binary format (6 bytes/IPv4 peer, 18 bytes/IPv6 peer). `TrackerPool` wraps 64 shards with per-shard `RwLock` for concurrency. `counters.rs` provides O(1) incremental snapshots; `topk.rs` does 4-way top-K ranking.

- **`protocol/`** — BitTorrent protocol encoding, no network dependency. Custom bencode serializer (`bencode.rs`), BEP 3 announce/scrape query parsing (`announce.rs`), and compile-time 256×256 lookup table for Azureus-style peer ID client identification (`client_id.rs`).

- **`server/`** — Axum HTTP layer. `handlers.rs` routes `/announce`, `/scrape`, `/healthz`, `/api/*`, and dashboard static files. `blacklist.rs` hot-reloads a torrent blacklist file every 5 seconds. `trends.rs` manages a 7-day ring buffer with optional JSONL persistence (10-min sampling).

`AppState` (in `server.rs`) is the shared state clone passed to all handlers, containing `Arc<TrackerPool>`, `Arc<RwLock<TrendStore>>`, and `Arc<RwLock<Arc<HashSet<InfoHash>>>>` for the blacklist.

## Key Design Decisions

- **Sharding**: 64 shards chosen via `DefaultHasher(info_hash) % 64` to minimize lock contention
- **Peer storage**: Packed binary, no heap allocation per peer — stored inline in `Vec<u8>`
- **Background tasks**: Peer expiry sweeps every 1s, trend sampling every 10min, blacklist file watch every 5s
- **build.rs**: Embeds `assets/index.html` into the binary at compile time; supports `personal-contact` feature to inject contact HTML
- **Features**: `dashboard` (default) enables web UI routes; `personal-contact` injects contact info into the HTML

## Project Structure

```
src/
├── main.rs              # Entry point: CLI parsing, Tokio runtime, graceful shutdown
├── lib.rs               # Library root: re-exports core, protocol, server modules
├── core/                # Pure tracker engine (no I/O)
│   ├── types.rs         # Core types: InfoHash, PeerId, PeerState, TorrentStats
│   ├── tracker.rs       # 64-shard TrackerPool with per-shard RwLock
│   ├── swarm.rs         # Per-torrent peer set, packed binary storage
│   ├── topk.rs          # 4-way Top-K ranking
│   └── counters.rs      # Incremental counters for O(1) snapshots
├── protocol/            # BitTorrent protocol encoding (no network dependency)
│   ├── bencode.rs       # Lightweight bencode encoder
│   ├── announce.rs      # BEP 3 announce/scrape query parsing
│   └── client_id.rs     # 102-client peer ID identification
└── server/              # HTTP server layer (axum + tokio)
    ├── handlers.rs      # HTTP handlers for all endpoints
    ├── blacklist.rs     # Torrent blacklist with 5-sec hot-reload
    └── trends.rs        # Trend data collection and JSONL persistence
```

## Testing Pattern

Integration tests in `tests/tracker_http.rs` use `axum::Router::oneshot()` with `tower::ServiceExt` — no real TCP server needed. Tests create `AppState::new()` or `AppState::sharded()` directly.

Helper functions: `app()` creates a single-tracker router, `sharded_app()` creates a 16-shard router, `request_with_connect_info()` attaches a mock `SocketAddr` (required because handlers extract client IP from connect info).

**Running tests:**
```bash
cargo test                    # all tests
cargo test <test_name>        # single test
cargo test --doc              # doctests only
```

## CI/CD

GitHub Actions release workflow (`.github/workflows/release.yml`) triggers on pushes to `main` that modify `Cargo.toml`. It compares the version field — if changed, builds Linux/Windows binaries and creates a GitHub Release with the new version tag. Bump `version` in `Cargo.toml` to trigger a release.

## CLI Configuration

All flags support env var fallback (prefixed `RUSTRACKER_`). CLI takes precedence over env vars.

## Key API Endpoints

- `GET /announce` — BEP 3 announce endpoint (peer registration)
- `GET /scrape` — BEP 3 scrape endpoint (torrent statistics)
- `GET /healthz` — Health check (returns `200 OK` with body `ok`)
- `GET /api/stats` — JSON statistics (peers, seeders, leechers, torrents, completed)
- `GET /api/trends` — Historical trend data (7-day retention, 10-min sampling)
- `GET /api/clients` — Client distribution (top 15 clients by peer count)
- `GET /api/top100` — Top 100 torrents by peers/seeders/leechers/downloaded
- `GET /` — Web dashboard (requires `dashboard` feature)
