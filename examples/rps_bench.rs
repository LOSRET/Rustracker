//! RPS benchmark through the full HTTP stack
//!
//! Sends real HTTP requests through axum router → query parsing →
//! tracker announce → bencode response → HTTP response.
//! Gradually ramps up peer scale to show RPS degradation.
//!
//! Usage: cargo run --release --example rps_bench

use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use rustracker::server::{router, AppState};
use tower::ServiceExt;

// ─── RSS measurement ──────────────────────────────────────────────

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

// ─── URL encoding helpers ─────────────────────────────────────────

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

// ─── Pre-load via HTTP ────────────────────────────────────────────

async fn load_peers(app: &axum::Router, num_torrents: usize, num_peers: usize) {
    let peers_per = (num_peers / num_torrents).max(1);
    for t in 0..num_torrents {
        let ih = make_info_hash(t);
        for j in 0..peers_per {
            let idx = t * peers_per + j;
            if idx >= num_peers { return; }
            let left = if j % 3 == 0 { 0 } else { 1_000_000_000 };
            let uri = announce_uri(ih, make_peer_id(idx), (6881 + idx % 60000) as u16, left);
            let resp = app.clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let _ = resp.into_body().collect().await;
        }
    }
}

// ─── RPS measurement burst ────────────────────────────────────────

async fn measure_rps(app: &axum::Router, num_torrents: usize, burst_size: usize) -> (f64, f64) {
    let t0 = Instant::now();
    for k in 0..burst_size {
        let torrent_idx = k % num_torrents;
        let ih = make_info_hash(torrent_idx);
        // peer_id uses 1B offset to avoid colliding with pre-loaded peers
        let pid = make_peer_id(1_000_000_000 + k);
        let uri = announce_uri(ih, pid, (40000 + k % 60000) as u16, 500_000_000);
        let resp = app.clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let _ = resp.into_body().collect().await;
    }
    let elapsed = t0.elapsed();
    let rps = burst_size as f64 / elapsed.as_secs_f64();
    (rps, elapsed.as_secs_f64() * 1000.0)
}

// ─── Main ────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let schedule: Vec<(usize, usize)> = vec![
        (1_000,        10_000),
        (5_000,        50_000),
        (10_000,      200_000),
        (50_000,      500_000),
        (100_000,   1_000_000),
        (200_000,   2_000_000),
        (500_000,   5_000_000),
        (1_000_000,10_000_000),
    ];

    let burst_size: usize = 50_000; // HTTP overhead is larger, use smaller burst
    let app = router(AppState::sharded(
        Duration::from_secs(1800),
        Duration::from_secs(2700),
        16,
    ));

    println!("peers,rps,elapsed_ms,rss_mb");

    let mut loaded_peers: usize = 0;

    for &(torrents, target_peers) in &schedule {
        let to_load = target_peers.saturating_sub(loaded_peers);
        if to_load > 0 {
            eprint!("Loading {} peers ({} torrents) via HTTP ... ", to_load, torrents);
            let t0 = Instant::now();
            load_peers(&app, torrents, to_load).await;
            loaded_peers = target_peers;
            eprintln!("done in {:.1}s", t0.elapsed().as_secs_f64());
        }

        let (rps, elapsed_ms) = measure_rps(&app, torrents, burst_size).await;
        let rss = mem::rss_bytes();
        println!("{},{:.0},{:.1},{:.1}", target_peers, rps, elapsed_ms, fmt_mb(rss));

        eprintln!(
            "  peers={:>10}  rps={:>10.0}  elapsed={:>7.1}ms  rss={}",
            target_peers, rps, elapsed_ms, fmt_mb(rss),
        );
    }

    eprintln!("\nDone.");
}
