//! Unified benchmark: concurrent HTTP requests
//!
//! Sends concurrent requests through axum router to measure true throughput.
//! Tracks RPS, RSS, CPU, and per-request latency.
//!
//! Usage: cargo run --release --example unified_bench

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use rustracker::server::{router, AppState};
use tokio::sync::Semaphore;
use tower::ServiceExt;

#[cfg(windows)]
mod mem {
    use std::mem;
    #[repr(C)]
    struct PMC {
        cb: u32,
        pfc: u32,
        pws: usize,
        ws: usize,
        qppp: usize,
        qpp: usize,
        qpnp: usize,
        qnp: usize,
        pf: usize,
        ppf: usize,
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessMemoryInfo(p: isize, c: *mut PMC, b: u32) -> i32;
    }
    pub fn rss_bytes() -> usize {
        unsafe {
            let mut i: PMC = mem::zeroed();
            i.cb = mem::size_of::<PMC>() as u32;
            GetProcessMemoryInfo(GetCurrentProcess(), &mut i, i.cb);
            i.ws
        }
    }
}

#[cfg(unix)]
mod mem {
    pub fn rss_bytes() -> usize {
        std::fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("VmRSS:"))
            .and_then(|l| l.split_whitespace().nth(1)?.parse::<usize>().ok())
            .unwrap_or(0)
            * 1024
    }
}

// ─── CPU ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct CpuTimes {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

impl CpuTimes {
    fn read() -> Option<Self> {
        let content = std::fs::read_to_string("/proc/stat").ok()?;
        let line = content.lines().find(|l| l.starts_with("cpu "))?;
        let nums: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|s| s.parse().ok())
            .collect();
        if nums.len() < 8 {
            return None;
        }
        Some(CpuTimes {
            user: nums[0],
            nice: nums[1],
            system: nums[2],
            idle: nums[3],
            iowait: nums[4],
            irq: nums[5],
            softirq: nums[6],
            steal: nums[7],
        })
    }
    fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }
    fn busy(&self) -> u64 {
        self.total() - self.idle - self.iowait
    }
}

fn cpu_usage_pct(prev: &CpuTimes, curr: &CpuTimes) -> f64 {
    let d_total = curr.total().saturating_sub(prev.total());
    let d_busy = curr.busy().saturating_sub(prev.busy());
    if d_total == 0 {
        0.0
    } else {
        d_busy as f64 / d_total as f64 * 100.0
    }
}

// ─── Latency ─────────────────────────────────────────────────────

struct LatencyTracker {
    buf: Vec<u64>,
    pos: usize,
}

impl LatencyTracker {
    fn new(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            pos: 0,
        }
    }
    fn record(&mut self, us: u64) {
        if self.buf.len() < self.buf.capacity() {
            self.buf.push(us);
        } else {
            self.buf[self.pos] = us;
        }
        self.pos = (self.pos + 1) % self.buf.capacity();
    }
    fn stats(&self) -> (f64, u64, u64, u64) {
        if self.buf.is_empty() {
            return (0.0, 0, 0, 0);
        }
        let mut s = self.buf.clone();
        s.sort_unstable();
        let avg = s.iter().sum::<u64>() as f64 / s.len() as f64;
        (
            avg,
            s[s.len() / 2],
            s[(s.len() as f64 * 0.99) as usize],
            *s.last().unwrap(),
        )
    }
    fn clear(&mut self) {
        self.buf.clear();
        self.pos = 0;
    }
}

fn fmt_mb(b: usize) -> String {
    format!("{:.1}", b as f64 / (1024.0 * 1024.0))
}
fn fmt_us(us: u64) -> String {
    if us >= 1000 {
        format!("{:.1}ms", us as f64 / 1000.0)
    } else {
        format!("{}us", us)
    }
}

// ─── URL helpers ──────────────────────────────────────────────────

fn percent_encode(bytes: [u8; 20]) -> String {
    let mut s = String::with_capacity(60);
    for &b in &bytes {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            s.push(b as char);
        } else {
            s.push('%');
            s.push(hex_digit(b >> 4));
            s.push(hex_digit(b & 0xf));
        }
    }
    s
}
fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + n - 10) as char,
        _ => '?',
    }
}
fn make_info_hash(t: usize) -> [u8; 20] {
    let mut h = [0u8; 20];
    h[0..8].copy_from_slice(&(t as u64).to_le_bytes());
    h
}
fn make_peer_id(i: usize) -> [u8; 20] {
    let mut p = [0u8; 20];
    p[0..8].copy_from_slice(&(i as u64).to_le_bytes());
    p
}
fn announce_uri(ih: [u8; 20], pid: [u8; 20], port: u16, left: u64) -> String {
    format!("/announce?info_hash={}&peer_id={}&port={port}&uploaded=0&downloaded=0&left={left}&event=started&compact=1",
        percent_encode(ih), percent_encode(pid))
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn next_usize(&mut self, b: usize) -> usize {
        if b == 0 {
            0
        } else {
            (self.next_u64() as usize) % b
        }
    }
}

// ─── Main ────────────────────────────────────────────────────────

const CONCURRENCY: usize = 100;
const MAX_PEERS_PER_TORRENT: usize = 100_000;
const NEW_TORRENT_PROB: usize = 2500;

#[tokio::main]
async fn main() {
    let max_peers: usize = 10_000_000;
    let total_requests: usize = 5_000_000;

    let app = router(AppState::sharded(
        Duration::from_secs(1800),
        Duration::from_secs(2700),
        16,
    ));
    let sem = Arc::new(Semaphore::new(CONCURRENCY));

    let mut rng = Rng::new(0xdead_beef_cafe);
    let mut next_peer_idx: usize = 0;
    let mut active_peers: Vec<(usize, u16)> = Vec::new();
    let mut torrent_peer_counts: Vec<usize> = Vec::new();
    let mut num_torrents: usize = 0;
    let mut request_count: usize = 0;
    let mut new_join_count: usize = 0;
    let mut reannounce_count: usize = 0;

    let sample_interval = total_requests / 100;
    let mut next_sample = sample_interval;

    println!("request,torrents,peers,new_joins,reannounces,rps,window_rps,rss_mb,cpu_pct,avg_us,p50_us,p99_us,max_us");
    let bench_start = Instant::now();
    let baseline_rss = mem::rss_bytes();
    let mut prev_cpu = CpuTimes::read();
    let mut latency = LatencyTracker::new(sample_interval);
    let mut last_sample_elapsed = 0.0;
    let mut last_sample_count = 0usize;

    let mut join_set = tokio::task::JoinSet::<(bool, usize, u16, u64)>::new();
    // (is_new, torrent_idx, port, latency_us)

    for _ in 0..total_requests {
        // Acquire permit (blocks if CONCURRENCY tasks already in-flight)
        let permit = sem.clone().acquire_owned().await.unwrap();

        let current_peers = active_peers.len();
        let is_new = if current_peers == 0 {
            true
        } else if current_peers >= max_peers {
            false
        } else {
            let p_new = if current_peers < 1000 {
                0.95
            } else {
                let ratio = current_peers as f64 / max_peers as f64;
                (1.0 - ratio).powf(2.0).max(0.01)
            };
            rng.next_usize(10000) < (p_new * 10000.0) as usize
        };

        let (uri, torrent_idx, port) = if is_new {
            let ti = loop {
                if active_peers.is_empty() || rng.next_usize(10000) < NEW_TORRENT_PROB {
                    let ti = num_torrents;
                    num_torrents += 1;
                    torrent_peer_counts.push(0);
                    break ti;
                }
                let idx = rng.next_usize(active_peers.len());
                let (candidate_ti, _) = active_peers[idx];
                if torrent_peer_counts[candidate_ti] < MAX_PEERS_PER_TORRENT {
                    break candidate_ti;
                }
            };
            torrent_peer_counts[ti] += 1;
            let p = (6881 + next_peer_idx % 60000) as u16;
            let left = if rng.next_usize(3) == 0 {
                0
            } else {
                1_000_000_000
            };
            (
                announce_uri(make_info_hash(ti), make_peer_id(next_peer_idx), p, left),
                ti,
                p,
            )
        } else {
            let idx = rng.next_usize(current_peers);
            let (ti, p) = active_peers[idx];
            let left = rng.next_usize(2_000_000_000) as u64;
            (
                announce_uri(make_info_hash(ti), make_peer_id(idx), p, left),
                ti,
                p,
            )
        };

        if is_new {
            next_peer_idx += 1;
            new_join_count += 1;
        } else {
            reannounce_count += 1;
        }
        request_count += 1;

        let app = app.clone();
        join_set.spawn(async move {
            let t = Instant::now();
            let resp = app
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let _ = resp.into_body().collect().await;
            let lat = t.elapsed().as_micros() as u64;
            drop(permit);
            (is_new, torrent_idx, port, lat)
        });

        // Drain all readily-completed tasks (non-blocking), then block if at capacity
        while let Some(Ok((is_new, ti, p, lat))) = join_set.try_join_next() {
            latency.record(lat);
            if is_new {
                active_peers.push((ti, p));
            }
        }
        if join_set.len() >= CONCURRENCY {
            if let Some(Ok((is_new, ti, p, lat))) = join_set.join_next().await {
                latency.record(lat);
                if is_new {
                    active_peers.push((ti, p));
                }
            }
        }

        // Periodic sampling
        if request_count >= next_sample {
            // Drain any remaining completed tasks
            while let Some(Ok((is_new, ti, p, lat))) = join_set.try_join_next() {
                latency.record(lat);
                if is_new {
                    active_peers.push((ti, p));
                }
            }

            let elapsed = bench_start.elapsed().as_secs_f64();
            let cum_rps = request_count as f64 / elapsed;
            let window_rps = if last_sample_elapsed > 0.0 {
                (request_count - last_sample_count) as f64 / (elapsed - last_sample_elapsed)
            } else {
                cum_rps
            };
            last_sample_elapsed = elapsed;
            last_sample_count = request_count;
            let rss = mem::rss_bytes().saturating_sub(baseline_rss);
            let cpu_pct = if let (Some(prev), Some(curr)) = (prev_cpu, CpuTimes::read()) {
                let pct = cpu_usage_pct(&prev, &curr);
                prev_cpu = Some(curr);
                pct
            } else {
                prev_cpu = CpuTimes::read();
                0.0
            };
            let (avg, p50, p99, max_lat) = latency.stats();
            latency.clear();

            println!(
                "{},{},{},{},{},{:.0},{:.0},{},{:.1},{},{},{},{}",
                request_count,
                num_torrents,
                active_peers.len(),
                new_join_count,
                reannounce_count,
                cum_rps,
                window_rps,
                fmt_mb(rss),
                cpu_pct,
                avg as u64,
                p50,
                p99,
                max_lat
            );
            eprintln!("  req={:>8}  torrents={:>8}  peers={:>10}  rps={:>10.0}  win={:>10.0}  new={:>8}  re={:>8}  rss={:>8}  cpu={:.1}%  lat: avg={} p50={} p99={} max={}",
                request_count, num_torrents, active_peers.len(), cum_rps, window_rps, new_join_count, reannounce_count,
                fmt_mb(rss), cpu_pct, fmt_us(avg as u64), fmt_us(p50), fmt_us(p99), fmt_us(max_lat));
            next_sample += sample_interval;
        }
    }

    // Drain remaining tasks
    while let Some(Ok((is_new, ti, p, lat))) = join_set.join_next().await {
        latency.record(lat);
        if is_new {
            active_peers.push((ti, p));
        }
    }

    let total_time = bench_start.elapsed().as_secs_f64();
    eprintln!("\nDone. {} req in {:.1}s = {:.0} rps | torrents={} peers={} (new={}, re={}) | rss={} | concurrency={}",
        request_count, total_time, request_count as f64 / total_time,
        num_torrents, active_peers.len(), new_join_count, reannounce_count,
        fmt_mb(mem::rss_bytes().saturating_sub(baseline_rss)), CONCURRENCY);
}
