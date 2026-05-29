use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use rustracker::protocol::announce_response;
use rustracker::tracker::{AnnounceInput, Tracker};
use rustracker::types::{AnnounceEvent, InfoHash, PeerId};

// ─── RSS measurement ──────────────────────────────────────────────

#[cfg(windows)]
mod mem {
    use std::mem;
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_nonpaged_pool_usage: usize,
        quota_nonpaged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessMemoryInfo(
            process: isize,
            ppsmemcounters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
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
            .unwrap_or(0)
            * 1024
    }
}

fn fmt_mb(b: usize) -> String {
    format!("{:.1}", b as f64 / (1024.0 * 1024.0))
}

// ─── Production distribution ──────────────────────────────────────

fn gen_distribution(
    torrents: usize,
    total_peers: usize,
    max_peers: usize,
) -> (Vec<InfoHash>, Vec<usize>) {
    let c = (max_peers - 1) as f64;

    let mut lo = 0.01f64;
    let mut hi = 2.0f64;
    let mut counts: Vec<usize> = Vec::new();

    for _ in 0..50 {
        let alpha = (lo + hi) / 2.0;
        let mut sum: usize = 0;
        counts.clear();
        for rank in 1..=torrents {
            let extra = (c / (rank as f64).powf(alpha)) as usize;
            let n = 1 + extra.min(max_peers - 1);
            counts.push(n);
            sum += n;
            if sum > total_peers + 5000 {
                break;
            }
        }
        if sum < total_peers {
            hi = alpha;
        } else if sum > total_peers + 100 {
            lo = alpha;
        } else {
            break;
        }
    }
    while counts.len() < torrents {
        counts.push(1);
    }
    counts.truncate(torrents);
    if counts.iter().sum::<usize>() > total_peers {
        let mut excess = counts.iter().sum::<usize>() - total_peers;
        for i in (0..counts.len()).rev() {
            if excess == 0 {
                break;
            }
            if counts[i] > 1 {
                let r = excess.min(counts[i] - 1);
                counts[i] -= r;
                excess -= r;
            }
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

fn run_bench(label: &str, torrents: usize, total_peers: usize, max_peers: usize) {
    let (hashes, counts) = gen_distribution(torrents, total_peers, max_peers);
    let actual_peers: usize = counts.iter().sum();
    eprintln!("[{label}] torrents={torrents} peers={actual_peers} max_peer={max_peers}");

    let interval = Duration::from_secs(1800);
    let timeout = Duration::from_secs(2700);
    let now = Instant::now();
    let mut tracker = Tracker::new(interval, timeout);

    let base_ip = Ipv4Addr::new(10, 0, 0, 1);
    let base_port: u16 = 6881;

    let sample_every = (torrents / 40).max(1);

    let baseline = mem::rss_bytes();
    let mut last_rss = baseline;
    let mut peers_done = 0usize;

    for i in 0..torrents {
        let count = counts[i];
        for j in 0..count {
            let idx = peers_done + j;
            let port = (base_port as usize + idx % 60000) as u16;
            // 30% IPv4, 70% IPv6
            let ip = if idx % 10 < 3 {
                let mut octets = base_ip.octets();
                octets[3] = (idx % 254 + 1) as u8;
                IpAddr::V4(Ipv4Addr::from(octets))
            } else {
                IpAddr::V6(Ipv6Addr::new(
                    0x2001,
                    0xdb8,
                    0,
                    0,
                    (idx >> 48 & 0xffff) as u16,
                    (idx >> 32 & 0xffff) as u16,
                    (idx >> 16 & 0xffff) as u16,
                    (idx & 0xffff) as u16,
                ))
            };

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

            let output = tracker.announce(input, now);
            // Encode bencode response (simulates real request path)
            let _resp = announce_response(output, true);
            drop(_resp);
        }
        peers_done += count;

        if (i + 1) % sample_every == 0 {
            let rss = mem::rss_bytes();
            let delta = rss.saturating_sub(last_rss);
            println!(
                "{},{},{},{},{}",
                label,
                i + 1,
                peers_done,
                fmt_mb(rss.saturating_sub(baseline)),
                fmt_mb(delta),
            );
            last_rss = rss;
        }
    }

    let final_rss = mem::rss_bytes().saturating_sub(baseline);
    let snapshot = tracker.snapshot();
    eprintln!(
        "[{label}] done | rss={} torrents={} peers={} seeders={} leechers={}",
        fmt_mb(final_rss),
        snapshot.totals.torrents,
        snapshot.totals.peers,
        snapshot.totals.seeders,
        snapshot.totals.leechers,
    );
}

pub fn run_large_bench(label: &str) {
    println!("label,torrents_done,peers_added,rss_mb,delta_mb");
    run_bench(label, 10_000_000, 25_000_000, 500);
}
