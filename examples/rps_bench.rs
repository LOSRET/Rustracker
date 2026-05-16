//! RPS benchmark: concurrent load + re-announce
//!
//! Simulates a real tracker where new peers join AND existing peers
//! re-announce simultaneously on the same instance.
//!
//! For each scale step:
//!   - Loader task: continuously adds NEW peers (simulating joins)
//!   - RPS task: re-announces EXISTING peers (simulating re-announce traffic)
//!   - Both run concurrently on the same tracker
//!
//! Usage: cargo run --release --example rps_bench

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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

// ─── Send one HTTP announce ───────────────────────────────────────

async fn http_announce(app: &axum::Router, uri: String) {
    let resp = app.clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let _ = resp.into_body().collect().await;
}

// ─── Loader: adds NEW peers, updates counter ──────────────────────

async fn loader(app: &axum::Router, num_torrents: usize, num_peers: usize, counter: &AtomicUsize) {
    let peers_per = (num_peers / num_torrents).max(1);
    for t in 0..num_torrents {
        let ih = make_info_hash(t);
        for j in 0..peers_per {
            let idx = t * peers_per + j;
            if idx >= num_peers { return; }
            let left = if j % 3 == 0 { 0 } else { 1_000_000_000 };
            let uri = announce_uri(ih, make_peer_id(idx), (6881 + idx % 60000) as u16, left);
            http_announce(app, uri).await;
            counter.store(idx + 1, Ordering::Relaxed);
        }
    }
}

// ─── RPS: re-announces EXISTING peers ─────────────────────────────

async fn rps_loop(
    app: &axum::Router,
    num_torrents: usize,
    counter: &AtomicUsize,
    stop: &AtomicUsize,
    results: &mut Vec<(usize, f64)>,
) {
    let mut next_sample_at: usize = 10_000;
    let mut burst_count: usize = 0;
    let burst_start = Instant::now();

    loop {
        let loaded = counter.load(Ordering::Relaxed);
        if loaded == 0 {
            tokio::task::yield_now().await;
            continue;
        }

        // Re-announce an existing peer (cycling through loaded peers)
        let peer_idx = burst_count % loaded;
        let torrent_idx = peer_idx % num_torrents;
        let ih = make_info_hash(torrent_idx);
        // Re-use the same peer_id as the original load → this is a re-announce
        let uri = announce_uri(ih, make_peer_id(peer_idx), (6881 + peer_idx % 60000) as u16, 0);
        http_announce(app, uri).await;
        burst_count += 1;

        // Record RPS at milestones
        if loaded >= next_sample_at {
            let elapsed = burst_start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                let rps = burst_count as f64 / elapsed;
                results.push((loaded, rps));
                eprintln!(
                    "  peers={:>10}  rps={:>10.0}  re-announces={}",
                    loaded, rps, burst_count,
                );
            }
            next_sample_at += 10_000;
            if next_sample_at > 1_000_000 {
                next_sample_at += 900_000; // sparser samples at scale
            }
        }

        // Check if loader is done
        if stop.load(Ordering::Relaxed) == 1 && loaded >= counter.load(Ordering::Relaxed) {
            // Final measurement
            let elapsed = burst_start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                let rps = burst_count as f64 / elapsed;
                results.push((loaded, rps));
                eprintln!(
                    "  peers={:>10}  rps={:>10.0}  re-announces={} (final)",
                    loaded, rps, burst_count,
                );
            }
            break;
        }
    }
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

    let app = router(AppState::sharded(
        Duration::from_secs(1800),
        Duration::from_secs(2700),
        16,
    ));

    println!("peers,rps");

    let counter = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicUsize::new(0));
    let mut all_results: Vec<(usize, f64)> = Vec::new();
    let mut loaded_peers: usize = 0;

    for &(torrents, target_peers) in &schedule {
        let to_load = target_peers.saturating_sub(loaded_peers);
        if to_load == 0 { continue; }

        eprint!("Loading {} peers ({} torrents) while measuring RPS ... ", to_load, torrents);
        let t0 = Instant::now();
        stop.store(0, Ordering::Relaxed);

        // Spawn loader and RPS measurement concurrently
        let loader_app = app.clone();
        let loader_counter = counter.clone();
        let rps_counter = counter.clone();
        let rps_stop = stop.clone();
        let rps_app = app.clone();

        let loader_handle = tokio::spawn(async move {
            loader(&loader_app, torrents, to_load, &loader_counter).await;
        });
        let rps_handle = tokio::spawn(async move {
            let mut results = Vec::new();
            rps_loop(&rps_app, torrents, &rps_counter, &rps_stop, &mut results).await;
            results
        });

        // Wait for loader to finish, then signal RPS to stop
        loader_handle.await.unwrap();
        stop.store(1, Ordering::Relaxed);
        let step_results = rps_handle.await.unwrap();
        loaded_peers = target_peers;

        eprintln!("  loader done in {:.1}s", t0.elapsed().as_secs_f64());
        all_results.extend(step_results);
    }

    // Output final CSV
    for (peers, rps) in &all_results {
        let rss = mem::rss_bytes();
        println!("{},{:.0},{:.1}", peers, rps, fmt_mb(rss));
    }

    eprintln!("\nDone. Total data points: {}", all_results.len());
}
