//! CI memory comparison: HashMap vs BTreeMap
//!
//! Focused test for GitHub Actions — only runs the container memory comparison
//! at key scales, skipping all other tests.
//!
//! Outputs CSV-formatted results for easy parsing in CI.

use std::collections::BTreeMap;
use std::time::Instant;

// ─── RSS measurement (cross-platform) ────────────────────────────

#[cfg(windows)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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

#[allow(dead_code)]
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
    #[allow(dead_code)]
    peers: Vec<u8>,
}

impl Swarm {
    fn with_peers(n: usize) -> Self {
        Self { peers: vec![0x42u8; n * 12] }
    }
}

// ─── Main ────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let engine = args.get(1).map(|s| s.as_str()).unwrap_or("hashmap");

    // Header
    println!("type,scale,torrents,peers,container_mb,build_sec");

    let scales = [
        100_000, 428_493,
        1_000_000, 2_000_000, 3_000_000, 4_000_000, 5_000_000,
        6_000_000, 7_000_000, 8_000_000, 9_000_000, 10_000_000,
    ];

    for &n in &scales {
        let hashes: Vec<InfoHash> = (0..n).map(|i| InfoHash::from_u64(i as u64)).collect();

        let t0 = Instant::now();
        let (label, container_mb) = match engine {
            "btree" => {
                let mut bt: BTreeMap<InfoHash, Swarm> = BTreeMap::new();
                for i in 0..n { bt.insert(hashes[i], Swarm::with_peers(1)); }
                let mem = bt.len() as f64 * (76.0 + 28.0) / (1024.0 * 1024.0); // K+V inline + node overhead
                drop(bt);
                ("btree", mem)
            }
            _ => {
                let mut hm: std::collections::HashMap<InfoHash, Swarm> = std::collections::HashMap::new();
                for i in 0..n { hm.insert(hashes[i], Swarm::with_peers(1)); }
                let mem = hm.capacity() as f64 * (20.0 + 24.0 + 1.0) / (1024.0 * 1024.0); // slot K+V+ctrl
                drop(hm);
                ("hashmap", mem)
            }
        };

        let sec = t0.elapsed().as_secs_f64();

        println!(
            "{},{},{},{},{:.1},{:.3}",
            label, n, n, n, container_mb, sec,
        );
    }
}
