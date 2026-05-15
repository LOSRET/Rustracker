//! Real-tracker memory benchmark for CI
//!
//! Starts an embedded tracker, feeds production-pattern announce traffic,
//! samples RSS periodically.  Outputs CSV for trend analysis.
//!
//! Usage: cargo run --release --example memory_tracker_bench

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use rustracker::tracker::{AnnounceInput, Tracker};
use rustracker::types::{AnnounceEvent, InfoHash, PeerId};

// ─── RSS measurement ──────────────────────────────────────────────

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
    extern "system" { fn GetCurrentProcess() -> isize;
        fn GetProcessMemoryInfo(process: isize, ppsmemcounters: *mut ProcessMemoryCounters, cb: u32) -> i32; }
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
            .unwrap_or_default().lines()
            .find(|l| l.starts_with("VmRSS:"))
            .and_then(|l| l.split_whitespace().nth(1)?.parse::<usize>().ok())
            .unwrap_or(0) * 1024
    }
}

fn fmt_mb(b: usize) -> String { format!("{:.1}", b as f64 / (1024.0 * 1024.0)) }

// ─── Production distribution ──────────────────────────────────────

fn prod_infohashes_and_peers() -> (Vec<InfoHash>, Vec<usize>) {
    let torrents = 428_493;
    let total_peers = 666_384;
    let max_peers = 366;
    let c = (max_peers - 1) as f64;

    let mut lo = 0.01f64; let mut hi = 2.0f64; let mut alpha = 0.5;
    let mut counts: Vec<usize> = Vec::new();

    for _ in 0..50 {
        alpha = (lo + hi) / 2.0;
        let mut sum: usize = 0; counts.clear();
        for rank in 1..=torrents {
            let extra = (c / (rank as f64).powf(alpha)) as usize;
            let n = 1 + extra.min(max_peers - 1);
            counts.push(n); sum += n;
            if sum > total_peers + 5000 { break; }
        }
        if sum < total_peers { hi = alpha; }
        else if sum > total_peers + 100 { lo = alpha; }
        else { break; }
    }
    while counts.len() < torrents { counts.push(1); }
    counts.truncate(torrents);
    let mut sum: usize = counts.iter().sum();
    if sum > total_peers {
        let mut excess = sum - total_peers;
        for i in (0..counts.len()).rev() {
            if excess == 0 { break; }
            if counts[i] > 1 { let r = excess.min(counts[i] - 1); counts[i] -= r; excess -= r; }
        }
    }

    let hashes: Vec<InfoHash> = (0..torrents)
        .map(|i| {
            let mut h = [0u8; 20];
            h[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            InfoHash::new(h)
        })
        .collect();

    (hashes, counts)
}

// ─── Main ────────────────────────────────────────────────────────

fn main() {
    let (hashes, counts) = prod_infohashes_and_peers();
    let n = hashes.len();
    let total_peers: usize = counts.iter().sum();
    let max_peer = counts.iter().max().copied().unwrap_or(0);
    eprintln!("torrents={} peers={} max_peer={}", n, total_peers, max_peer);

    let interval = Duration::from_secs(1800);
    let timeout = Duration::from_secs(2700);
    let now = Instant::now();
    let mut tracker = Tracker::new(interval, timeout);

    let base_ip = Ipv4Addr::new(10, 0, 0, 1);
    let base_port: u16 = 6881;

    let sample_every = n / 40; // ~10K per sample

    // Header
    println!("torrents_done,peers_added,rss_mb,delta_mb");

    let baseline = mem::rss_bytes();
    let mut last_rss = baseline;
    let mut peers_done = 0usize;

    for i in 0..n {
        let count = counts[i];
        for j in 0..count {
            let ip_octets = base_ip.octets();
            let mut ip = ip_octets;
            ip[3] = ((peers_done + j) % 254 + 1) as u8;
            let ip = IpAddr::V4(Ipv4Addr::from(ip));
            let port = (base_port as usize + (peers_done + j) % 60000) as u16;

            let mut pid = [0u8; 20];
            pid[0..8].copy_from_slice(&((peers_done + j) as u64).to_le_bytes());

            let input = AnnounceInput {
                info_hash: hashes[i],
                peer_id: PeerId::new(pid),
                ip,
                port,
                uploaded: 0,
                downloaded: 0,
                left: if j % 3 == 0 { 0 } else { 1_000_000_000 },
                event: AnnounceEvent::Started,
                numwant: 50,
                client_tag: 1,
            };

            tracker.announce(input, now);
        }
        peers_done += count;

        if (i + 1) % sample_every == 0 {
            let rss = mem::rss_bytes();
            let delta = rss.saturating_sub(last_rss);
            println!(
                "{},{},{},{}",
                i + 1,
                peers_done,
                fmt_mb(rss.saturating_sub(baseline)),
                fmt_mb(delta),
            );
            last_rss = rss;
        }
    }

    // Final snapshot
    let final_rss = mem::rss_bytes().saturating_sub(baseline);
    let snapshot = tracker.snapshot();
    eprintln!(
        "done | rss={} torrents={} peers={} seeders={} leechers={}",
        fmt_mb(final_rss),
        snapshot.totals.torrents,
        snapshot.totals.peers,
        snapshot.totals.seeders,
        snapshot.totals.leechers,
    );
}
