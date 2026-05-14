# rustracker

A small HTTP BitTorrent tracker written in Rust.

## Features

- `GET /announce` for peer announcements
- `GET /scrape` for torrent statistics
- Bencoded tracker responses
- Compact IPv4 peer responses by default
- In-memory swarm state with peer expiry
- Flat dashboard on the same HTTP port
- JSON stats API for dashboard data
- Graceful shutdown on Ctrl+C
- Torrent blacklist via file

## Run

```powershell
cargo run -- --listen 127.0.0.1:8080
```

Environment variables can also configure the service:

```powershell
$env:RUSTRACKER_LISTEN = "127.0.0.1:8080"
$env:RUSTRACKER_INTERVAL_SECS = "1800"
$env:RUSTRACKER_PEER_TIMEOUT_SECS = "3000"
cargo run
```

### Torrent Blacklist

Create a text file with one 40-character hex `info_hash` per line (blank lines and `#` comments are ignored), then pass it via `--blacklist` or `RUSTRACKER_BLACKLIST`:

```text
# blocked torrent
e09b1c0c4b174ef2b25c8de662941777fb3f2d7a
```

```powershell
cargo run -- --blacklist blacklist.txt
```

Blacklisted torrents are rejected on `announce` (HTTP 403) and silently excluded from `scrape` results. The file is watched for changes every 5 seconds — edit and save, no restart needed. Invalid lines are logged as warnings and skipped.

### Trend Data Persistence

Tracker trend data (torrents, peers, seeders, leechers, client distribution) is stored in memory by default and lost on restart. To persist trends to disk, use `--trends-file` or `RUSTRACKER_TRENDS_FILE`:

```powershell
cargo run -- --trends-file trends.jsonl
```

Data is sampled every 10 minutes and retained for 7 days. Two JSONL files are created:
- The path you specify (e.g. `trends.jsonl`) — torrents/peers/seeders/leechers per timestamp
- `top_clients.jsonl` in the same directory — client distribution per timestamp

On restart, existing data is loaded from disk. The Linux installer enables this by default at `/var/lib/rustracker/trends.jsonl`.

## Linux Install

Release packages include `install-linux.sh` for Linux hosts. Put the Linux binary and the script in the same directory, then run:

```sh
sudo sh install-linux.sh
```

The installer provides a Chinese menu for install/update, uninstall, service start/stop/restart, status, and configuration display.

Non-interactive commands are also available:

```sh
sudo sh install-linux.sh install
sudo sh install-linux.sh status
sudo sh install-linux.sh configure
sudo sh install-linux.sh restart
```

Use `configure` to change the listening address, announce interval, or peer timeout after installation. For the listening address, entering only a port such as `6969` is accepted and saved as `0.0.0.0:6969`.

## Endpoints

Health check:

```text
GET /healthz
```

Dashboard:

```text
GET /
```

Stats API:

```text
GET /api/stats
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

The first version is intended for local or controlled deployments. Add rate limiting, metrics, and persistent storage before exposing it as a public tracker.

## Test

```powershell
cargo test
```
