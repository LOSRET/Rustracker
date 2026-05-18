//! HTTP load tester for rustracker with realistic BitTorrent behavior.
//!
//! Features:
//! - Zipf distribution for torrent hotness
//! - Peer reuse with realistic events (started/completed/stopped)
//! - Seeder/leecher mix
//! - Pre-computed info_hash strings to reduce per-request allocation
//! - Semaphore backpressure for stable throughput measurement
//!
//! # Usage
//!
//! ```text
//! cargo run --release --example load_test -- [OPTIONS]
//!
//! Options:
//!   --duration <SECS>       Test duration (default: 30)
//!   --concurrency <N>       Max in-flight requests (default: 500)
//!   --torrents <N>          Distinct info_hashes (default: 1000)
//!   --peers <N>             Simulated peers (default: 50000)
//!   --scrape-weight <N>     Scrape weight (default: 1)
//!   --announce-weight <N>   Announce weight (default: 5)
//!   --keep-alive / --no-keep-alive
//!   --target <ADDR>         External target
//!   --port <PORT>           Embedded server port (default: random)
//!   --shards <N>            Tracker shards (default: 64)
//!   --no-embed              Skip embedded server
//!   --progress-interval <N> Progress line interval in seconds (default: 5)
//! ```

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use rand::prelude::*;
use reqwest::Client;
use rustracker::server::{router, AppState, DEFAULT_TRACKER_SHARDS};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Config {
    duration: Duration,
    concurrency: usize,
    torrents: usize,
    peers: usize,
    scrape_weight: usize,
    announce_weight: usize,
    keep_alive: bool,
    target: Option<SocketAddr>,
    listen_port: u16,
    shards: usize,
    progress_interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(30),
            concurrency: 500,
            torrents: 1000,
            peers: 50_000,
            scrape_weight: 1,
            announce_weight: 5,
            keep_alive: true,
            target: None,
            listen_port: 0,
            shards: DEFAULT_TRACKER_SHARDS,
            progress_interval: 5,
        }
    }
}

fn parse_args() -> anyhow::Result<Config> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = Config::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--duration" => { i += 1; cfg.duration = Duration::from_secs(args.get(i).context("--duration")?.parse()?); }
            "--concurrency" => { i += 1; cfg.concurrency = args.get(i).context("--concurrency")?.parse()?; }
            "--torrents" => { i += 1; cfg.torrents = args.get(i).context("--torrents")?.parse()?; }
            "--peers" => { i += 1; cfg.peers = args.get(i).context("--peers")?.parse()?; }
            "--scrape-weight" => { i += 1; cfg.scrape_weight = args.get(i).context("--scrape-weight")?.parse()?; }
            "--announce-weight" => { i += 1; cfg.announce_weight = args.get(i).context("--announce-weight")?.parse()?; }
            "--keep-alive" => cfg.keep_alive = true,
            "--no-keep-alive" => cfg.keep_alive = false,
            "--target" => { i += 1; cfg.target = Some(args.get(i).context("--target")?.parse()?); }
            "--port" => { i += 1; cfg.listen_port = args.get(i).context("--port")?.parse()?; }
            "--shards" => { i += 1; cfg.shards = args.get(i).context("--shards")?.parse()?; }
            "--no-embed" => {}
            "--progress-interval" => { i += 1; cfg.progress_interval = args.get(i).context("--progress-interval")?.parse()?; }
            other => anyhow::bail!("unknown argument: {other}"),
        }
        i += 1;
    }
    if cfg.target.is_none() && args.contains(&"--no-embed".to_string()) {
        anyhow::bail!("--no-embed requires --target");
    }
    cfg.concurrency = cfg.concurrency.max(1);
    cfg.torrents = cfg.torrents.max(1);
    cfg.peers = cfg.peers.max(1);
    Ok(cfg)
}

// ---------------------------------------------------------------------------
// Latency tracker
// ---------------------------------------------------------------------------

#[derive(Default)]
struct LatencyRecorder {
    samples: std::sync::Mutex<Vec<u64>>,
}

impl LatencyRecorder {
    fn record(&self, latency: Duration) {
        let us = latency.as_micros() as u64;
        if let Ok(mut v) = self.samples.lock() { v.push(us); }
    }

    fn snapshot_and_drain(&self) -> Option<LatencyStats> {
        let mut v = { let mut g = self.samples.lock().unwrap(); std::mem::take(&mut *g) };
        if v.is_empty() { return None; }
        v.sort_unstable();
        let n = v.len();
        let sum: u64 = v.iter().sum();
        Some(LatencyStats {
            count: n, min_us: v[0], max_us: v[n - 1], avg_us: sum / n as u64,
            p50_us: v[n * 50 / 100], p95_us: v[n * 95 / 100], p99_us: v[n * 99 / 100],
        })
    }
}

struct LatencyStats { count: usize, min_us: u64, max_us: u64, avg_us: u64, p50_us: u64, p95_us: u64, p99_us: u64 }

impl std::fmt::Display for LatencyStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "n={} min={}µs avg={}µs p50={}µs p95={}µs p99={}µs max={}µs",
            self.count, self.min_us, self.avg_us, self.p50_us, self.p95_us, self.p99_us, self.max_us)
    }
}

// ---------------------------------------------------------------------------
// Counters
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Counters {
    announce_ok: AtomicUsize, announce_err: AtomicUsize,
    scrape_ok: AtomicUsize, scrape_err: AtomicUsize,
}

// ---------------------------------------------------------------------------
// Zipf distribution
// ---------------------------------------------------------------------------

struct ZipfSampler { n: usize, exponent: f64 }

impl ZipfSampler {
    fn new(n: usize, exponent: f64) -> Self { Self { n, exponent } }

    fn sample<R: Rng>(&self, rng: &mut R) -> usize {
        let u: f64 = rng.random();
        let raw = (u * self.n as f64).powf(1.0 / (1.0 + self.exponent));
        (raw as usize).min(self.n - 1)
    }
}

// ---------------------------------------------------------------------------
// Pre-computed info_hash strings
// ---------------------------------------------------------------------------

struct InfoHashes { hashes: Vec<String> }

impl InfoHashes {
    fn new(n: usize) -> Self {
        let hashes: Vec<String> = (0..n).map(|i| {
            let mut id = [b'h'; 20];
            let s = format!("{i:018}");
            id[2..].copy_from_slice(s.as_bytes());
            String::from_utf8_lossy(&id).into_owned()
        }).collect();
        Self { hashes }
    }

    #[inline]
    fn get(&self, idx: usize) -> &str { &self.hashes[idx % self.hashes.len()] }
}

// ---------------------------------------------------------------------------
// Peer pool (shared, Mutex-protected)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SimPeer {
    info_hash_idx: usize,
    peer_id: String,
    port: u16,
    is_seeder: bool,
    alive: bool,
}

struct PeerPool {
    peers: Mutex<Vec<SimPeer>>,
    zipf: ZipfSampler,
    total_peers: usize,
    hashes: Arc<InfoHashes>,
}

impl PeerPool {
    fn new(total_peers: usize, total_torrents: usize, hashes: Arc<InfoHashes>) -> Self {
        let mut rng = StdRng::seed_from_u64(42);
        let zipf = ZipfSampler::new(total_torrents, 1.1);
        let peers: Vec<SimPeer> = (0..total_peers).map(|i| {
            let ih = zipf.sample(&mut rng);
            SimPeer {
                info_hash_idx: ih,
                peer_id: format!("p{i:019}"),
                port: 10_000 + (i as u16 % 50_000),
                is_seeder: rng.random_bool(0.4),
                alive: true,
            }
        }).collect();
        Self { peers: Mutex::new(peers), zipf, total_peers, hashes }
    }

    async fn next_announce_url(&self, base: SocketAddr, rng: &mut impl Rng) -> String {
        let mut peers = self.peers.lock().await;
        let idx = rng.random_range(0..self.total_peers);
        let peer = &mut peers[idx];

        let action: f64 = rng.random();
        let (event, left) = if !peer.alive {
            peer.info_hash_idx = self.zipf.sample(rng);
            peer.is_seeder = rng.random_bool(0.4);
            peer.alive = true;
            ("started", if peer.is_seeder { 0u64 } else { rng.random_range(1..10_000_000u64) })
        } else if action < 0.02 {
            peer.alive = false;
            ("stopped", 0u64)
        } else if action < 0.08 && !peer.is_seeder {
            peer.is_seeder = true;
            ("completed", 0u64)
        } else {
            ("", if peer.is_seeder { 0u64 } else { rng.random_range(1..10_000_000u64) })
        };

        let ih = self.hashes.get(peer.info_hash_idx).to_owned();
        let pid = peer.peer_id.clone();
        let port = peer.port;
        drop(peers);

        let mut url = format!("http://{base}/announce?info_hash={ih}&peer_id={pid}&port={port}&left={left}&compact=1");
        if !event.is_empty() { url.push_str("&event="); url.push_str(event); }
        url
    }

    fn next_scrape_url(&self, base: SocketAddr, rng: &mut impl Rng) -> String {
        let mut url = format!("http://{base}/scrape?");
        for i in 0..5 {
            if i > 0 { url.push('&'); }
            let idx = self.zipf.sample(rng);
            url.push_str("info_hash=");
            url.push_str(self.hashes.get(idx));
        }
        url
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = parse_args()?;

    let (addr, _server_handle) = if let Some(target) = cfg.target {
        (target, None)
    } else {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], cfg.listen_port)))
            .await.context("bind")?;
        let addr = listener.local_addr()?;
        let app = router(AppState::sharded(Duration::from_secs(1800), Duration::from_secs(3000), cfg.shards));
        let h = tokio::spawn(async move {
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await
        });
        wait_until_ready(addr).await?;
        (addr, Some(h))
    };

    println!("=== rustracker load test ===");
    println!("target:        {addr}");
    println!("duration:      {}s", cfg.duration.as_secs());
    println!("concurrency:   {}", cfg.concurrency);
    println!("torrents:      {} (Zipf s=1.1)", cfg.torrents);
    println!("peers:         {}", cfg.peers);
    println!("announce:scrape = {}:{}", cfg.announce_weight, cfg.scrape_weight);
    println!("keep-alive:    {}", cfg.keep_alive);
    println!("shards:        {}", cfg.shards);
    println!();

    let client = Client::builder()
        .pool_max_idle_per_host(if cfg.keep_alive { cfg.concurrency } else { 0 })
        .pool_idle_timeout(Duration::from_secs(90))
        .timeout(Duration::from_secs(10))
        .tcp_keepalive(if cfg.keep_alive { Some(Duration::from_secs(30)) } else { None })
        .build()?;

    let counters = Arc::new(Counters::default());
    let announce_latency = Arc::new(LatencyRecorder::default());
    let scrape_latency = Arc::new(LatencyRecorder::default());
    let semaphore = Arc::new(Semaphore::new(cfg.concurrency));
    let stop = Arc::new(AtomicBool::new(false));
    let hashes = Arc::new(InfoHashes::new(cfg.torrents));
    let peer_pool = Arc::new(PeerPool::new(cfg.peers, cfg.torrents, hashes));
    let total_weight = cfg.announce_weight + cfg.scrape_weight;

    let started = Instant::now();

    let scrape_weight = cfg.scrape_weight;

    // -- Producer --
    let producer = {
        let stop = stop.clone();
        let counters = counters.clone();
        let announce_latency = announce_latency.clone();
        let scrape_latency = scrape_latency.clone();
        let semaphore = semaphore.clone();
        let client = client.clone();
        let peer_pool = peer_pool.clone();

        tokio::spawn(async move {
            let mut rng = SmallRng::from_rng(&mut rand::rng());
            while !stop.load(Ordering::Relaxed) {
                let permit = match semaphore.clone().acquire_owned().await {
                    Ok(p) => p, Err(_) => break,
                };
                if stop.load(Ordering::Relaxed) { drop(permit); break; }

                let is_scrape = rng.random_range(0..total_weight) < scrape_weight;
                let client = client.clone();
                let counters = counters.clone();
                let announce_latency = announce_latency.clone();
                let scrape_latency = scrape_latency.clone();
                let peer_pool = peer_pool.clone();

                tokio::spawn(async move {
                    let _permit = permit;
                    if is_scrape {
                        let url = { let mut r = SmallRng::from_rng(&mut rand::rng()); peer_pool.next_scrape_url(addr, &mut r) };
                        send_request(&client, url, &scrape_latency, &counters.scrape_ok, &counters.scrape_err).await;
                    } else {
                        let url = { let mut r = SmallRng::from_rng(&mut rand::rng()); peer_pool.next_announce_url(addr, &mut r).await };
                        send_request(&client, url, &announce_latency, &counters.announce_ok, &counters.announce_err).await;
                    }
                });
            }
        })
    };

    // -- Progress --
    let progress_handle = {
        let stop = stop.clone();
        let counters = counters.clone();
        let announce_latency = announce_latency.clone();
        let scrape_latency = scrape_latency.clone();
        let interval = cfg.progress_interval;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval));
            ticker.tick().await;
            let mut last_total = 0usize;
            let t0 = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                ticker.tick().await;
                let a_ok = counters.announce_ok.load(Ordering::Relaxed);
                let a_err = counters.announce_err.load(Ordering::Relaxed);
                let s_ok = counters.scrape_ok.load(Ordering::Relaxed);
                let s_err = counters.scrape_err.load(Ordering::Relaxed);
                let total = a_ok + a_err + s_ok + s_err;
                let elapsed = t0.elapsed().as_secs_f64();
                let rps = total as f64 / elapsed.max(0.001);
                let interval_rps = (total - last_total) as f64 / interval as f64;
                last_total = total;
                let a_lat = announce_latency.snapshot_and_drain().map(|s| format!("{s}")).unwrap_or_else(|| "no data".into());
                let s_lat = scrape_latency.snapshot_and_drain().map(|s| format!("{s}")).unwrap_or_else(|| "no data".into());
                println!("[{:>4}s] total={:<8} rps(avg)={:<10.0} rps(now)={:<8.0} | announce: ok={} err={} lat=[{}] | scrape: ok={} err={} lat=[{}]",
                    elapsed as u64, total, rps, interval_rps, a_ok, a_err, a_lat, s_ok, s_err, s_lat);
            }
        })
    };

    // -- Wait --
    tokio::time::sleep(cfg.duration).await;
    stop.store(true, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = producer.await;
    let _ = progress_handle.await;

    // -- Report --
    let a_ok = counters.announce_ok.load(Ordering::Relaxed);
    let a_err = counters.announce_err.load(Ordering::Relaxed);
    let s_ok = counters.scrape_ok.load(Ordering::Relaxed);
    let s_err = counters.scrape_err.load(Ordering::Relaxed);
    let total = a_ok + a_err + s_ok + s_err;
    let seconds = started.elapsed().as_secs_f64().max(0.001);

    println!();
    println!("=== Final Report ===");
    println!("elapsed:       {seconds:.1}s");
    println!("total:         {total}");
    println!("rps:           {:.0}", total as f64 / seconds);
    println!();
    println!("announce ok:   {a_ok}");
    println!("announce err:  {a_err}");
    if let Some(s) = announce_latency.snapshot_and_drain() { println!("announce lat:  {s}"); }
    println!();
    println!("scrape ok:     {s_ok}");
    println!("scrape err:    {s_err}");
    if let Some(s) = scrape_latency.snapshot_and_drain() { println!("scrape lat:    {s}"); }

    let stats: serde_json::Value = client.get(format!("http://{addr}/api/stats")).send().await?.error_for_status()?.json().await?;
    println!();
    println!("tracker torrents: {}", stats["torrents"].as_u64().unwrap_or(0));
    println!("tracker peers:    {}", stats["peers"].as_u64().unwrap_or(0));
    println!("tracker seeders:  {}", stats["seeders"].as_u64().unwrap_or(0));
    println!("tracker leechers: {}", stats["leechers"].as_u64().unwrap_or(0));

    Ok(())
}

async fn send_request(client: &Client, url: String, latency: &LatencyRecorder, ok: &AtomicUsize, err: &AtomicUsize) {
    let t0 = Instant::now();
    match client.get(&url).send().await {
        Ok(r) => { latency.record(t0.elapsed()); if r.status().is_success() { ok.fetch_add(1, Ordering::Relaxed); } else { err.fetch_add(1, Ordering::Relaxed); } }
        Err(_) => { err.fetch_add(1, Ordering::Relaxed); }
    }
}

async fn wait_until_ready(addr: SocketAddr) -> anyhow::Result<()> {
    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(r) = client.get(format!("http://{addr}/healthz")).send().await {
            if r.status().is_success() { return Ok(()); }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    anyhow::bail!("server did not become ready")
}
