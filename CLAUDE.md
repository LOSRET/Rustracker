# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

rustracker is a lightweight, high-performance HTTP BitTorrent tracker written in Rust. It implements BEP 3 compliant `announce` and `scrape` endpoints, serves a real-time web dashboard on the same port, and identifies 102 BitTorrent clients from peer ID prefixes.

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
```

## Load Testing / Benchmarks

```bash
cargo run --release --example announce_load -- 2000 200 100
cargo run --release --example load_test -- --duration 60 --concurrency 500 --torrents 1000 --peers 50000
cargo run --release --example rps_bench
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

## Testing Pattern

Integration tests in `tests/tracker_http.rs` use `axum::Router::oneshot()` with `tower::ServiceExt` — no real TCP server needed. Tests create `AppState::new()` or `AppState::sharded()` directly.

## CLI Configuration

All flags support env var fallback (prefixed `RUSTRACKER_`). CLI takes precedence over env vars.
