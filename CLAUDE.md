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
- **Never guess APIs/configs from memory.** Uncertain details (signatures, options, breaking changes, file necessity) must be verified in official docs before writing or asserting — memory-based claims cause silent breakage. Applies to all libraries (Tailwind, Nuxt UI, Vue, Vite, Rust crates, etc.). High-certainty, low-churn usage (stable core syntax, long-unchanged APIs) can be skipped.

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
# NOTE: dashboard feature triggers `npm run build` in frontend/ via build.rs.
# Requires Node 20+ and frontend/node_modules installed (run `npm install` in frontend/ first).
cargo build --release

# Build without dashboard (smaller binary, no frontend bundled, no Node needed)
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

### Frontend (Vue 3 + Vite + Tailwind)

The dashboard SPA lives in `frontend/`. It is built by Vite into `dist/` and embedded into the Rust binary at compile time by `build.rs`.

```bash
# Install dependencies (first time only)
cd frontend && npm install

# Development server with hot reload (proxies /api, /announce, /scrape to 127.0.0.1:8080)
cd frontend && npm run dev

# Production build (outputs to ../dist/)
cd frontend && npm run build

# Type check
cd frontend && npm run typecheck
```

> **Note:** `cargo build --release` with the `dashboard` feature will automatically run `npm run build` in `frontend/`. Ensure `frontend/node_modules/` exists (run `npm install` once after cloning). CI workflows handle this automatically via `setup-node` + `npm ci`.

> **Note:** `.githooks/commit-msg` strips solitary `@` lines from commit messages (PowerShell here-string artifact). Enable with `git config core.hooksPath .githooks` after cloning.

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

# Memory benchmarks (system vs jemalloc vs mimalloc comparison)
cargo run --release --example memory_tracker_bench
cargo run --release --example memory_jemalloc_bench
cargo run --release --example memory_mimalloc_bench

# Shrink/regrow cycle benchmark (env: SHRINK_TORRENTS=30000, SHRINK_BULK=300)
cargo run --release --example shrink_bench
```

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
│  BTreeMap<InfoHash,   │     │  10-min sampling          │
│   Swarm>              │     └─────────────────────────┘
└───────────────────────┘
```

Three-layer design with clear separation of concerns:

- **`core/`** — Pure tracker engine, no I/O. `Tracker` holds a `BTreeMap<InfoHash, Swarm>` where each `Swarm` stores peers in packed binary format (6 bytes/IPv4 peer, 18 bytes/IPv6 peer). `counters.rs` provides O(1) incremental snapshots; `topk.rs` does 4-way top-K ranking.

- **`protocol/`** — BitTorrent protocol encoding, no network dependency. Custom bencode serializer (`bencode.rs`), BEP 3 announce/scrape query parsing (`announce.rs`), and compile-time 256×256 lookup table for Azureus-style peer ID client identification (`client_id.rs`).

- **`server/`** — Axum HTTP layer. `pool.rs` defines `TrackerPool`, which wraps 64 `Tracker` shards with per-shard `RwLock` (shard selected via `DefaultHasher(info_hash) % 64`) to minimize contention. `handlers.rs` routes `/announce`, `/scrape`, `/healthz`, `/api/*`, and dashboard static files. `admin.rs` implements the authenticated `GET`/`POST /api/blacklist` endpoints. `blacklist.rs` hot-reloads a torrent blacklist file every 5 seconds. `trends.rs` manages a 7-day ring buffer with optional JSONL persistence (10-min sampling).

`AppState` (in `server.rs`) is the shared state clone passed to all handlers, containing `Arc<TrackerPool>`, `Arc<RwLock<TrendStore>>`, `Arc<RwLock<Arc<HashSet<InfoHash>>>>` for the blacklist, and `Arc<AtomicU64>` for the real-time RPS counter updated on every announce/scrape request.

## Key Design Decisions

- **Sharding**: 64 shards chosen via `DefaultHasher(info_hash) % 64` to minimize lock contention
- **Peer storage**: Packed binary, no heap allocation per peer — stored inline in `Vec<u8>`
- **Shrink strategy (`shrink_if_idle`)**: After peer expiry sweeps, each `Swarm`'s `Vec<u8>` checks if `cap > floor * entry_size` (skip tiny vecs), then computes `target = next_power_of_two(entries).max(floor) * entry_size`. Shrinks to target only if `target < cap`, yielding 50–100% post-shrink utilization. This is tighter than opentracker's approach (waits until <25% utilization, then halves).
- **Background tasks**: Peer expiry sweeps every 1s, trend sampling every 10min, blacklist file watch every 5s
- **Allocator**: `tikv-jemallocator` is installed as the global allocator on Linux only (`#[global_allocator]` in `main.rs`); other platforms use the system allocator. Memory benchmarks (`memory_tracker_bench` vs `memory_jemalloc_bench` vs `memory_mimalloc_bench`) compare these.
- **build.rs**: When the `dashboard` feature is on, runs `npm run build` in `frontend/`, then copies `dist/index.html` to `$OUT_DIR/index.html` and `dist/assets/*` to `$OUT_DIR/assets/`, generating an `assets_manifest.rs` of `include_bytes!` calls. The `personal-contact` feature is handled at Vite build time via the `VITE_PERSONAL_CONTACT=true` env var — `vite.config.ts` inlines contact info (blog URL + email) into the JS bundle via `define: { __CONTACT__: ... }` (an object when enabled, `null` when not); the `Disclaimer.vue` component renders it with `v-if` and real `<a>` tags using i18n labels (`t.blog_label`/`t.contact_label`). This means the contact info distinction is compile-time (public releases have no contact info in the binary at all), controlled by whether CI sets the env var.
- **Features**: `dashboard` (default) enables web UI routes and embeds the Vue SPA at compile time; `personal-contact` injects contact info into the frontend bundle via `VITE_PERSONAL_CONTACT` env var (set by `sync-deploy.yml`, not set by `release.yml`)

## Developer Conventions

Recorded by the user in `.trae/rules/project_rules.md` and `.trae/memory/project_rules.md` (Trae IDE project rules):

- **No panicking calls in production code**: `.unwrap()` / `.expect()` are banned in `src/` outside `#[cfg(test)]`. Use `if let`, `match`, `.map()`, `.unwrap_or()` instead. Tests, examples, and build scripts are exempt.
- **Verify before committing**: run `cargo check` (or `cargo build`) after writing code to catch compile errors.
- **Commit granularity**: small changes (single file, simple bug fix, small tweak) commit directly to the current branch; large changes (multi-file, new feature, refactor, version bump) go on a new branch via `gh pr create`.
- **Git sync before editing**: `git fetch` first, check `git status`; if clean `git pull`, if dirty `git stash` (or commit) then `git pull`.
- **Push preference**: the user generally wants code committed and pushed without extra confirmation — a `.git/hooks/post-commit` hook auto-pushes after each commit.

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

GitHub Actions workflows:

- **`ci.yml`** — fmt + clippy + tests on `src`/`tests`/`examples` changes (push to `main` or PR). Runs `cargo fmt --check`, strict clippy (`--lib --tests -- -D warnings`), non-blocking clippy on examples, and `cargo test` with both default and `--no-default-features` (installs Node first since default features build the dashboard).
- **`frontend.yml`** — Frontend validation on `frontend/**` changes: `npm run lint -- --max-warnings=0`, `typecheck`, `format:check`, `build`, then `cargo test index_returns_dashboard_html` to verify the embedded dashboard.
- **`release.yml`** — Triggers on pushes to `main` that modify `Cargo.toml`. Compares the version field between the push commit and its parent — if changed, builds Linux/Windows binaries and creates a GitHub Release with the new version tag. Bump `version` in `Cargo.toml` to trigger a release. Dashboard builds run `setup-node` + `npm ci` before `cargo build`; the no-dashboard matrix entry skips Node.
- **`sync-deploy.yml`** — Personal deployment workflow triggered on version bumps or manual dispatch. Builds Linux (musl) and Windows artifacts without creating a GitHub Release. Sets `VITE_PERSONAL_CONTACT=true` on `cargo build` so contact info is embedded in the frontend bundle.
- **`shrink-bench.yml`** — Runs the `shrink_bench` example (RSS shrink/regrow cycles) on `src/core/swarm.rs` changes or manual dispatch, posting the CSV results to the job summary. Sets `MALLOC_CONF` decay to 2s so freed pages return to the OS before RSS sampling.
- **`opencode.yml`** — Posting `/oc` or `/opencode` in an issue/PR comment runs the opencode agent (deepseek-v4-pro) on the repo.
- **`memory-benchmark.yml`** — Manual dispatch workflow that runs `unified_bench` and system-vs-jemalloc allocator comparisons.

## CLI Configuration

```
--listen                  RUSTRACKER_LISTEN                  default: [::]:8080 (Linux); [::]:8080 + 0.0.0.0:8080 (Windows); comma-separated and repeatable
--interval-secs           RUSTRACKER_INTERVAL_SECS           default: 1800 (announce interval returned to peers)
--peer-timeout-secs       RUSTRACKER_PEER_TIMEOUT_SECS       default: 3000
--blacklist               RUSTRACKER_BLACKLIST               optional: path to blacklist file
--trends-file             RUSTRACKER_TRENDS_FILE             optional: path to trends JSONL
--admin-token             RUSTRACKER_ADMIN_TOKEN             optional: bearer token for admin API
--trust-proxy-headers     RUSTRACKER_TRUST_PROXY_HEADERS     default: false — trust CF-Connecting-IP / X-Real-IP / X-Forwarded-For for the announce client IP; enable only when reachable exclusively through a trusted proxy (any client can spoof these headers otherwise)
```

All flags support env var fallback. CLI takes precedence over env vars. `--interval-secs` is the announce interval advertised to peers; `--peer-timeout-secs` is the peer expiry threshold. (Peer expiry sweeps run on a fixed 1s background interval, not on `--interval-secs`.)

## Key API Endpoints

- `GET /announce` — BEP 3 announce endpoint (peer registration)
- `GET /scrape` — BEP 3 scrape endpoint (torrent statistics)
- `GET /healthz` — Health check (returns `200 OK` with body `ok`)
- `GET /api/stats` — JSON statistics (peers, seeders, leechers, torrents, completed, rps, version, uptime_secs)
- `GET /api/trends` — Historical trend data (7-day retention, 10-min sampling)
- `GET /api/clients` — Client distribution (top 15 clients by peer count, with time-series history)
- `GET /api/clients/list` — All connected client types sorted by current peer count (snapshot)
- `GET /api/top100` — Top 100 torrents by peers/seeders/leechers/downloaded (`limit` query param, max 500)
- `GET /api/blacklist?info_hash=<40-char-hex>` — Authenticated read-only blacklist status endpoint; returns `blacklisted: true/false`
- `POST /api/blacklist` — Authenticated admin endpoint that appends a 40-char hex `info_hash` to the configured blacklist file, then updates the in-memory blacklist; requires `Authorization: Bearer <admin-token>`
- `GET /` — Web dashboard (requires `dashboard` feature)
