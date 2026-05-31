//! shrink/regrow cycle memory benchmark
//!
//! Measures RSS overhead after repeated grow→shrink→regrow cycles
//! through the real Tracker API (exercises PackedIpv4Peers::shrink_if_idle).
//!
//! design:
//!   Each torrent has 1 anchor peer (re-announced every cycle, survives expire)
//!   and N bulk peers (added fresh each cycle, expire at next cycle).
//!   A 5-second peer_timeout means cycles must space out by >5s between
//!   regrow and the next expire.  Uses real wall-clock sleep to ensure
//!   correct expiry regardless of machine speed.
//!
//! Usage: cargo run --release --example shrink_bench
//!
//! Config via env vars:
//!   SHRINK_TORRENTS=30000  (default)
//!   SHRINK_BULK=300        (default)
//!   SHRINK_CYCLES=10       (default)

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

    // Short peer_timeout so we don't wait too long between cycles.
    // After regrow we sleep(6s), then expire: regrow peers have aged 6s,
    // peer_timeout=5 → all expire cleanly.
    let peer_timeout = Duration::from_secs(5);
    let mut tracker = Tracker::new(
        Duration::from_secs(1800),
        peer_timeout,
    );

    eprintln!(
        "shrink_bench: torrents={} bulk/torrent={} cycles={} peer_timeout={}s",
        n_torrents,
        bulk_per_torrent,
        n_cycles,
        peer_timeout.as_secs(),
    );
    eprintln!();

    // ── Phase 1: Build ─────────────────────────────────────────
    // Use a shared Instant for all build announces so they have
    // the same last_seen_secs and expire together.
    eprint!("  building...");
    let build_time = Instant::now();
    let anchor_port: u16 = 6881;

    for t in 0..n_torrents {
        // Anchor peer (1 per torrent, survives expiry)
        announce(
            &mut tracker,
            t as u64,
            t as u64,
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            anchor_port,
            build_time,
        );

        // Bulk peers (300 per torrent)
        for j in 0..bulk_per_torrent {
            let seed = (t * bulk_per_torrent + j) as u64;
            let ip_octet = 2 + (seed % 253) as u8;
            let port = 6881u16.wrapping_add((seed % 60000) as u16);
            announce(
                &mut tracker,
                t as u64,
                seed + 1_000_000_000,
                IpAddr::V4(Ipv4Addr::new(10, 0, ip_octet, (seed >> 8) as u8)),
                port,
                build_time,
            );
        }
    }
    eprintln!(" done");
    let rss_build = rss_mb();
    eprintln!("  build RSS = {:.1} MB", rss_build);
    eprintln!();

    // Wait for build peers to age past peer_timeout.
    // build_time + 8s > build_time + peer_timeout(5s) + margin → all expired.
    eprint!("  ageing build peers...");
    std::thread::sleep(Duration::from_secs(8));
    eprintln!(" done");

    // ── CSV header ─────────────────────────────────────────────
    println!("cycle,torrents,rss_build_mb,rss_shrink_mb,rss_regrow_mb,overhead_mb");

    let mut prev_regrow_rss = rss_build;

    // ── Cycles ─────────────────────────────────────────────────
    for cycle in 0..n_cycles {
        eprint!("  cycle {:2}/{}: re-anchor...", cycle + 1, n_cycles);

        // ── Re-anchor ──────────────────────────────────────────
        // Update anchor's last_seen_secs so it survives expire.
        let anchor_time = Instant::now();
        for t in 0..n_torrents {
            announce(
                &mut tracker,
                t as u64,
                t as u64,
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                anchor_port,
                anchor_time,
            );
        }

        // Wait 3s: anchor has only aged 3s ≤ peer_timeout(5s), so survives.
        // But ALL previous bulk peers have aged 8s+3s = 11s > 5s → expired.
        std::thread::sleep(Duration::from_secs(3));

        eprint!("expire...");

        // ── Expire ─────────────────────────────────────────────
        tracker.expire_due(Instant::now());
        // shrink_if_idle was called on every non-empty swarm.
        // The anchor survived, so each swarm has 1 peer left.
        // Vec shrinks from 300+ capacity → 1 entry.
        let rss_shrink = rss_mb();

        eprint!("regrow...");

        // ── Regrow ─────────────────────────────────────────────
        let regrow_time = Instant::now();
        for t in 0..n_torrents {
            for j in 0..bulk_per_torrent {
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
                    regrow_time,
                );
            }
        }
        let rss_regrow = rss_mb();

        let overhead = rss_regrow - prev_regrow_rss;
        prev_regrow_rss = rss_regrow;

        println!(
            "{},{},{:.1},{:.1},{:.1},{:.1}",
            cycle + 1, n_torrents, rss_build, rss_shrink, rss_regrow, overhead,
        );

        eprintln!(
            " shrink={:.1} regrow={:.1} overhead={:.1} MB",
            rss_shrink, rss_regrow, overhead,
        );

        // Wait for regrow peers to age past peer_timeout for next cycle.
        // Next cycle's re-anchor + 3s wait = next expire at regrow_time + 6s.
        // So regrow peers have aged 6s > peer_timeout(5s) → expired.
        // Anchor will be re-announced in next cycle, only 3s before expire → stays.
        std::thread::sleep(Duration::from_secs(6));
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
