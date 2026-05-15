//! CI memory comparison: DashMap vs BTreeMap
//!
//! Focused test for GitHub Actions — only runs the container memory comparison
//! at key scales, skipping all other tests.
//!
//! Outputs CSV-formatted results for easy parsing in CI.

use std::collections::BTreeMap;
use std::time::Instant;

// ─── RSS measurement (cross-platform) ────────────────────────────

#[cfg(windows)]
mod mem {
    use std::mem;
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32, page_fault_count: u32,
        peak_working_set_size: usize, working_set_size: usize,
        quota_peak_paged_pool_usage: usize, quota_paged_pool_usage: usize,
        quota_peak_nonpaged_pool_usage: usize, quota_nonpaged_pool_usage: usize,
        pagefile_usage: usize, peak_pagefile_usage: usize,
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessMemoryInfo(process: isize, ppsmemcounters: *mut ProcessMemoryCounters, cb: u32) -> i32;
    }
    pub fn rss_bytes() -> usize {
        unsafe {
            let mut info: ProcessMemoryCounters = mem::zeroed();
            info.cb = mem::size_of::<ProcessMemoryCounters>() as u32;
            GetProcessMemoryInfo(GetCurrentProcess(), &mut info, info.cb);
            info.working_set_size
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
            .unwrap_or(0) * 1024
    }
}

fn fmt_mb(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
}

// ─── Shared types ────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
struct InfoHash([u8; 20]);

impl InfoHash {
    fn from_u64(n: u64) -> Self {
        let mut h = [0u8; 20];
        h[0..8].copy_from_slice(&n.to_le_bytes());
        InfoHash(h)
    }
}

#[derive(Default)]
struct Swarm {
    peers: Vec<u8>,
}

impl Swarm {
    fn with_peers(n: usize) -> Self {
        Self { peers: vec![0x42u8; n * 12] }
    }
}

// ─── Zipf production distribution ─────────────────────────────────

/// Generate realistic peer counts matching production: 428,493 torrents,
/// 666,384 total peers, max 366 per torrent.  Uses power-law distribution.
fn prod_peer_counts() -> Vec<usize> {
    let torrents = 428_493;
    let total_peers = 666_384;
    let max_peers = 366;
    let c = (max_peers - 1) as f64;

    // Binary search for Zipf α
    let mut lo = 0.01f64;
    let mut hi = 2.0f64;
    let mut alpha = 0.5;
    let mut counts: Vec<usize> = Vec::new();

    for _ in 0..50 {
        alpha = (lo + hi) / 2.0;
        let mut sum: usize = 0;
        counts.clear();
        for rank in 1..=torrents {
            let extra = (c / (rank as f64).powf(alpha)) as usize;
            let n = 1 + extra.min(max_peers - 1);
            counts.push(n);
            sum += n;
            if sum > total_peers + 5000 { break; }
        }
        if sum < total_peers { hi = alpha; }
        else if sum > total_peers + 100 { lo = alpha; }
        else { break; }
    }

    // Trim/tweak to exact total
    while counts.len() < torrents { counts.push(1); }
    counts.truncate(torrents);
    let mut sum: usize = counts.iter().sum();
    if sum > total_peers {
        let mut excess = sum - total_peers;
        for i in (0..counts.len()).rev() {
            if excess == 0 { break; }
            if counts[i] > 1 {
                let r = excess.min(counts[i] - 1);
                counts[i] -= r;
                excess -= r;
            }
        }
    }
    counts
}

// ─── Main ────────────────────────────────────────────────────────

fn main() {
    // Header
    println!("type,scale,torrents,peers,btreemap_mb,dashmap_mb,diff_mb,diff_pct,btree_sec,dashmap_sec");

    // ── Pass 1: container overhead (1 peer/torrent, many scales) ──
    let scales = [
        100_000, 428_493,
        1_000_000, 2_000_000, 3_000_000, 4_000_000, 5_000_000,
        6_000_000, 7_000_000, 8_000_000, 9_000_000, 10_000_000,
    ];

    for &n in &scales {
        let hashes: Vec<InfoHash> = (0..n).map(|i| InfoHash::from_u64(i as u64)).collect();

        // BTreeMap
        let before = mem::rss_bytes();
        let t0 = Instant::now();
        let mut bt: BTreeMap<InfoHash, Swarm> = BTreeMap::new();
        for i in 0..n { bt.insert(hashes[i], Swarm::with_peers(1)); }
        let bt_sec = t0.elapsed().as_secs_f64();
        let bt_rss = mem::rss_bytes().saturating_sub(before);
        drop(bt);

        // DashMap
        let before = mem::rss_bytes();
        let t0 = Instant::now();
        let dm: dashmap::DashMap<InfoHash, Swarm> = dashmap::DashMap::with_shard_amount(256);
        for i in 0..n { dm.insert(hashes[i], Swarm::with_peers(1)); }
        let dm_sec = t0.elapsed().as_secs_f64();
        let dm_rss = mem::rss_bytes().saturating_sub(before);
        drop(dm);

        let diff = dm_rss as f64 - bt_rss as f64;
        let pct = if bt_rss > 0 { diff / bt_rss as f64 * 100.0 } else { 0.0 };
        println!(
            "container,{},{},{},{},{},{:.1},{:.3},{:.3},{:.3}",
            n, n, n, fmt_mb(bt_rss), fmt_mb(dm_rss),
            diff / (1024.0 * 1024.0), pct, bt_sec, dm_sec,
        );
    }

    // ── Pass 2: production data (Zipf distribution) ──────────────
    let counts = prod_peer_counts();
    let n = counts.len();
    let total_peers: usize = counts.iter().sum();
    let hashes: Vec<InfoHash> = (0..n).map(|i| InfoHash::from_u64(i as u64)).collect();

    // BTreeMap
    let before = mem::rss_bytes();
    let t0 = Instant::now();
    let mut bt: BTreeMap<InfoHash, Swarm> = BTreeMap::new();
    for i in 0..n { bt.insert(hashes[i], Swarm::with_peers(counts[i])); }
    let bt_sec = t0.elapsed().as_secs_f64();
    let bt_rss = mem::rss_bytes().saturating_sub(before);
    drop(bt);

    // DashMap
    let before = mem::rss_bytes();
    let t0 = Instant::now();
    let dm: dashmap::DashMap<InfoHash, Swarm> = dashmap::DashMap::with_shard_amount(256);
    for i in 0..n { dm.insert(hashes[i], Swarm::with_peers(counts[i])); }
    let dm_sec = t0.elapsed().as_secs_f64();
    let dm_rss = mem::rss_bytes().saturating_sub(before);
    drop(dm);

    let diff = dm_rss as f64 - bt_rss as f64;
    let pct = if bt_rss > 0 { diff / bt_rss as f64 * 100.0 } else { 0.0 };
    println!(
        "production,{},{},{},{},{},{:.1},{:.3},{:.3},{:.3}",
        n, n, total_peers, fmt_mb(bt_rss), fmt_mb(dm_rss),
        diff / (1024.0 * 1024.0), pct, bt_sec, dm_sec,
    );
}
