//! RPS benchmark: single-task mixed traffic
//!
//! Simulates a real tracker lifecycle in one loop:
//!   - Early: mostly new peer joins (tracker warming up)
//!   - Later: mostly re-announces (steady state)
//!   - Ratio shifts naturally as the tracker grows
//!
//! Usage: cargo run --release --example rps_bench

use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use rustracker::server::{router, AppState};
use tower::ServiceExt;

#[cfg(windows)]
mod mem {
    use std::mem;
    #[repr(C)]
    struct PMC { cb:u32,pfc:u32,pws:usize,ws:usize,qppp:usize,qpp:usize,qpnp:usize,qnp:usize,pf:usize,ppf:usize }
    #[link(name="psapi")] extern "system" { fn GetCurrentProcess()->isize; fn GetProcessMemoryInfo(p:isize,c:*mut PMC,b:u32)->i32; }
    pub fn rss_bytes()->usize { unsafe { let mut i:PMC=mem::zeroed(); i.cb=mem::size_of::<PMC>() as u32; GetProcessMemoryInfo(GetCurrentProcess(),&mut i,i.cb); i.ws } }
}

#[cfg(unix)]
mod mem {
    pub fn rss_bytes() -> usize {
        std::fs::read_to_string("/proc/self/status")
            .unwrap_or_default().lines()
            .find(|l| l.starts_with("VmRSS:"))
            .and_then(|l| l.split_whitespace().nth(1)?.parse::<usize>().ok())
            .unwrap_or(0) * 1024
    }
}

fn fmt_mb(b: usize) -> String { format!("{:.1}", b as f64 / (1024.0 * 1024.0)) }

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
    match n { 0..=9 => (b'0'+n) as char, 10..=15 => (b'a'+n-10) as char, _ => '?' }
}

fn make_info_hash(torrent: usize) -> [u8; 20] {
    let mut h = [0u8; 20];
    h[0..8].copy_from_slice(&(torrent as u64).to_le_bytes());
    h
}

fn make_peer_id(idx: usize) -> [u8; 20] {
    let mut pid = [0u8; 20];
    pid[0..8].copy_from_slice(&(idx as u64).to_le_bytes());
    pid
}

fn announce_uri(info_hash: [u8; 20], peer_id: [u8; 20], port: u16, left: u64) -> String {
    format!(
        "/announce?info_hash={}&peer_id={}&port={port}&uploaded=0&downloaded=0&left={left}&event=started&compact=1",
        percent_encode(info_hash), percent_encode(peer_id),
    )
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self { Self(seed | 1) }
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn next_usize(&mut self, bound: usize) -> usize {
        if bound == 0 { 0 } else { (self.next_u64() as usize) % bound }
    }
}

#[tokio::main]
async fn main() {
    let max_torrents: usize = 1_000_000;
    let max_peers: usize = 10_000_000;
    let total_requests: usize = 5_000_000;

    let app = router(AppState::sharded(
        Duration::from_secs(1800),
        Duration::from_secs(2700),
        16,
    ));

    let mut rng = Rng::new(0xdead_beef_cafe);
    let mut next_peer_idx: usize = 0;
    let mut active_peers: Vec<(usize, u16)> = Vec::new();
    let mut request_count: usize = 0;
    let mut new_join_count: usize = 0;
    let mut reannounce_count: usize = 0;

    let sample_interval = total_requests / 50;
    let mut next_sample = sample_interval;

    println!("request,peers,rps,new_joins,reannounces,rss_mb");
    let bench_start = Instant::now();

    for _ in 0..total_requests {
        let current_peers = active_peers.len();

        let is_new = if current_peers >= max_peers {
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

        if is_new {
            let torrent_idx = next_peer_idx % max_torrents;
            let port = (6881 + next_peer_idx % 60000) as u16;
            let left = if rng.next_usize(3) == 0 { 0 } else { 1_000_000_000 };
            let uri = announce_uri(
                make_info_hash(torrent_idx),
                make_peer_id(next_peer_idx),
                port, left,
            );
            let resp = app.clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let _ = resp.into_body().collect().await;

            active_peers.push((torrent_idx, port));
            next_peer_idx += 1;
            new_join_count += 1;
        } else {
            let idx = rng.next_usize(current_peers);
            let (torrent_idx, port) = active_peers[idx];
            let uri = announce_uri(
                make_info_hash(torrent_idx),
                make_peer_id(idx),
                port,
                rng.next_usize(2_000_000_000) as u64,
            );
            let resp = app.clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let _ = resp.into_body().collect().await;

            reannounce_count += 1;
        }

        request_count += 1;

        if request_count >= next_sample {
            let elapsed = bench_start.elapsed().as_secs_f64();
            let rps = request_count as f64 / elapsed;
            let rss = mem::rss_bytes();
            println!(
                "{},{},{:.0},{},{},{}",
                request_count, active_peers.len(), rps,
                new_join_count, reannounce_count, fmt_mb(rss),
            );
            eprintln!(
                "  req={:>8}  peers={:>10}  rps={:>10.0}  new={:>8}  re={:>8}  rss={}",
                request_count, active_peers.len(), rps,
                new_join_count, reannounce_count, fmt_mb(rss),
            );
            next_sample += sample_interval;
        }
    }

    let total_time = bench_start.elapsed().as_secs_f64();
    eprintln!(
        "\nDone. {} requests in {:.1}s = {:.0} rps | peers={} (new={}, re={})",
        request_count, total_time, request_count as f64 / total_time,
        active_peers.len(), new_join_count, reannounce_count,
    );
}
