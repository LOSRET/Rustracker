//! shrink/regrow cycle memory benchmark
//!
//! Measures RSS behavior under repeated grow→shrink→regrow cycles
//! through the real Tracker API (exercises PackedIpv4Peers::shrink_if_idle).
//!
//! Usage: cargo run --release --example shrink_bench
//!
//! Output: CSV with RSS at each phase across N cycles.

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use rustracker::tracker::{AnnounceInput, Tracker};
use rustracker::types::{AnnounceEvent, InfoHash, PeerId};

// ─── RSS measurement ──────────────────────────────────────────────

#[cfg(windows)]
mod mem {
    use std::mem;
    #[repr(C)]
    struct PMC {
        cb: u32, pfc: u32, pws: usize, ws: usize,
        qppp: usize, qpp: usize, qpnp: usize, qnp: usize,
        pf: usize, ppf: usize,
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

fn rss_mb() -> f64 {
    mem::rss_bytes() as f64 / (1024.0 * 1024.0)
}


// ─── Helpers ─────────────────────────────────────────────────────

fn make_peer_id(seed: u64) -> PeerId {
    let mut b = [0u8; 20];
    b[0..8].copy_from_slice(&seed.to_le_bytes());
    PeerId(b)
}

fn make_info_hash(torrent: u64) -> InfoHash {
    let mut b = [0u8; 20];
    b[0..8].copy_from_slice(&torrent.to_le_bytes());
    InfoHash(b)
}

fn announce(
    tracker: &mut Tracker,
    torrent: u64,
    peer_seed: u64,
    ip: IpAddr,
    port: u16,
    now: Instant,
) {
    let _output = tracker.announce(
        AnnounceInput {
            info_hash: make_info_hash(torrent),
            peer_id: make_peer_id(peer_seed),
            ip,
            port,
            uploaded: 0,
            downloaded: 0,
            left: if peer_seed % 3 == 0 { 0 } else { 1_000_000_000 },
            event: AnnounceEvent::Started,
            numwant: 50,
            client_tag: (peer_seed % 250) as u8,
        },
        now,
    );
}

// ─── Main ─────────────────────────────────────────────────────────

fn main() {
    let n_torrents: usize = std::env::var("SHRINK_TORRENTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30_000);

    let bulk_per_torrent: usize = std::env::var("SHRINK_BULK")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    let n_cycles: usize = std::env::var("SHRINK_CYCLES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    // Use a short peer_timeout so each cycle is fast (~5-7s).
    // With peer_timeout=5 and sweep_interval=min(5,30).max(1)=5:
    //   add bulk → wait 2s → re-anchor → wait 5s → expire
    //   = ~7s per cycle.
    let peer_timeout = Duration::from_secs(5);
    let mut tracker = Tracker::new(
        Duration::from_secs(1800), // announce interval (unused for bench)
        peer_timeout,
    );
    let base = Instant::now();

    eprintln!(
        "shrink_bench: torrents={} bulk/torrent={} cycles={} peer_timeout={}s",
        n_torrents,
        bulk_per_torrent,
        n_cycles,
        peer_timeout.as_secs(),
    );
    eprintln!();
    eprintln!("─ Phase 1/2: initial build ─");

    // ── Phase 1: Build ─────────────────────────────────────────
    let anchor_port: u16 = 6881;

    for t in 0..n_torrents {
        // Insert anchor peer (1 per torrent, survives expiry)
        announce(
            &mut tracker,
            t as u64,
            t as u64,             // anchor peer_seed = torrent index
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            anchor_port,
            base,
        );

        // Insert bulk peers (will be expired)
        for j in 0..bulk_per_torrent {
            let seed = (t * bulk_per_torrent + j) as u64;
            let ip_octet = 2 + (seed % 253) as u8;
            let port = 6881u16.wrapping_add((seed % 60000) as u16);
            announce(
                &mut tracker,
                t as u64,
                seed + 1_000_000_000, // different from anchor seed
                IpAddr::V4(Ipv4Addr::new(10, 0, ip_octet, (seed >> 8) as u8)),
                port,
                base,
            );
        }
    }
    let rss_build = rss_mb();
    eprintln!("  build complete: RSS={:.1} MB", rss_build);
    eprintln!();

    // ── CSV header ─────────────────────────────────────────────
    println!("cycle,torrents,rss_build_mb,rss_shrink_mb,rss_regrow_mb,overhead_mb");

    let mut prev_cycle_rss = rss_build;

    // ── Cycles: expire → shrink → regrow → measure ─────────────
    for cycle in 0..n_cycles {
        let cycle_tag = cycle + 1;
        let cycle_base = base
            + Duration::from_secs((cycle as u64) * 7 + 7);

        // Re-anchor: re-announce the anchor (keeps it alive through expire)
        for t in 0..n_torrents {
            announce(
                &mut tracker,
                t as u64,
                t as u64,
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                anchor_port,
                cycle_base,
            );
        }

        // Advance time past peer_timeout and expire.
        // expire_due checks `now >= next_expire_at`, initially = started_at = base,
        // then advances by sweep_interval (5s).  After each expire, next += 5.
        let expire_time = cycle_base + Duration::from_secs(3);
        tracker.expire_due(expire_time);

        // At this point: all bulk peers from the previous add have expired
        // (they were last announced at least 5s ago). Anchor survives.
        // shrink_if_idle runs on every non-empty swarm.
        let rss_shrink = rss_mb();

        // ── Regrow: add new bulk peers ─────────────────────────
        let grow_time = expire_time + Duration::from_secs(1);
        for t in 0..n_torrents {
            for j in 0..bulk_per_torrent {
                // Unique per cycle: seed = cycle * offset + torrent + j
                let seed = (cycle as u64) * 10_000_000_000
                    + (t * bulk_per_torrent + j) as u64;
                let ip_octet = 2 + (seed % 253) as u8;
                let port = 6881u16.wrapping_add((seed % 60000) as u16);
                announce(
                    &mut tracker,
                    t as u64,
                    seed + 2_000_000_000,
                    IpAddr::V4(Ipv4Addr::new(10, 1, ip_octet, (seed >> 8) as u8)),
                    port,
                    grow_time,
                );
            }
        }
        let rss_regrow = rss_mb();

        let overhead = rss_regrow - prev_cycle_rss;
        println!(
            "{},{},{:.1},{:.1},{:.1},{:.1}",
            cycle_tag, n_torrents, rss_build, rss_shrink, rss_regrow, overhead,
        );

        eprintln!(
            "  cycle {:2}/{}: shrink={:.1} MB  regrow={:.1} MB  overhead={:.1} MB",
            cycle_tag, n_cycles, rss_shrink, rss_regrow, overhead,
        );

        prev_cycle_rss = rss_regrow;
    }

    // ── Summary ────────────────────────────────────────────────
    let final_rss = rss_mb();
    let total_growth = final_rss - rss_build;
    eprintln!();
    eprintln!(
        "Done.  build={:.1} MB  final={:.1} MB  total_growth={:.1} MB",
        rss_build, final_rss, total_growth,
    );
}
