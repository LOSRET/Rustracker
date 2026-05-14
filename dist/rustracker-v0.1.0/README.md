# rustracker

A small HTTP BitTorrent tracker written in Rust.

## Features

- `GET /announce` for peer announcements
- `GET /scrape` for torrent statistics
- Bencoded tracker responses
- Compact IPv4 peer responses by default
- In-memory swarm state with peer expiry
- Graceful shutdown on Ctrl+C

## Run

```powershell
cargo run -- --listen 127.0.0.1:8080
```

Environment variables can also configure the service:

```powershell
$env:RUSTRACKER_LISTEN = "127.0.0.1:8080"
$env:RUSTRACKER_INTERVAL_SECS = "1800"
$env:RUSTRACKER_PEER_TIMEOUT_SECS = "5400"
cargo run
```

## Endpoints

Health check:

```text
GET /healthz
```

Announce:

```text
GET /announce?info_hash=<20 byte percent-encoded value>&peer_id=<20 byte value>&port=6881&uploaded=0&downloaded=0&left=0&event=started&compact=1
```

Scrape:

```text
GET /scrape?info_hash=<20 byte percent-encoded value>
```

## Protocol Scope

This is an HTTP tracker. It does not implement the UDP tracker protocol. State is kept in memory, so peer lists and counters are lost when the process restarts.

The first version is intended for local or controlled deployments. Add authentication, rate limiting, metrics, and persistent storage before exposing it as a public tracker.

## Test

```powershell
cargo test
```
