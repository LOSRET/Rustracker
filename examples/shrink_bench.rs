//! shrink/regrow cycle memory benchmark
//!
//! Measures RSS overhead after repeated grow→shrink→regrow cycles.
//!
//! design:
//!   peer_timeout=1s so timing constraints are trivial:
//!   re-anchor and expire happen at the same Instant (anchor aged 0s, stays alive).
//!   Previous cycle's bulk peers are always >1s old → always expired.
//!   No timing windows, no race conditions.
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

// Example binaries don't inherit src/main.rs's global allocator.
#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// ─── RSS measurement ──────────────────────────────────────────────

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

    // peer_timeout=1s means:
    //   - anchor re-announced at same Instant as expire → survives
    //   - any bulk from >1s ago → expired
    //   - expire can fire again in just 1s (sweep_interval = 1s)
    let peer_timeout = Duration::from_secs(1);
    let mut tracker = Tracker::new(Duration::from_secs(1800), peer_timeout);

    eprintln!(
        "shrink_bench: torrents={} bulk/torrent={} cycles={} peer_timeout={}s",
        n_torrents,
        bulk_per_torrent,
        n_cycles,
        peer_timeout.as_secs(),
    );
    eprintln!();

    // ── Phase 1: Build ─────────────────────────────────────────
    eprint!("  building...");
    let build_time = Instant::now();
    let anchor_port: u16 = 6881;

    for t in 0..n_torrents {
        // Insert anchor peer (1 per torrent, survives expiry via re-announce)
        announce(
            &mut tracker,
            t as u64,
            t as u64,
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            anchor_port,
            build_time,
        );
        // Insert bulk peers (will be expired each cycle)
        for j in 0..bulk_per_torrent {
            let seed = (t * bulk_per_torrent + j) as u64;
            let ip_octet = 2 + (seed % 253) as u8;
            announce(
                &mut tracker,
                t as u64,
                seed + 1_000_000_000,
                IpAddr::V4(Ipv4Addr::new(10, 0, ip_octet, (seed >> 8) as u8)),
                6881u16.wrapping_add((seed % 60000) as u16),
                build_time,
            );
        }
    }
    eprintln!(" done");
    // Build peers need to age past peer_timeout=1s before first expire.
    // Build itself took time; add a small safety margin.
    std::thread::sleep(Duration::from_secs(2));

    let rss_build = rss_mb();
    eprintln!("  build RSS = {:.1} MB", rss_build);
    eprintln!();

    // ── CSV header ─────────────────────────────────────────────
    println!(
        "cycle,torrents,rss_build_mb,peers_after_expire,rss_shrink_mb,rss_regrow_mb,overhead_mb"
    );

    let mut prev_regrow_rss = rss_build;

    // ── Cycles ─────────────────────────────────────────────────
    for cycle in 0..n_cycles {
        eprint!("  cycle {:2}/{}", cycle + 1, n_cycles);

        // ── Re-anchor + expire (same Instant) ──────────────────
        // Anchor: aged 0s → definitely survives (peer_timeout=1s).
        // Previous bulk peers (from build or last regrow): aged
        // at least 2s (build→expire) or 3s (regrow→expire) → expired.
        let now = Instant::now();
        for t in 0..n_torrents {
            announce(
                &mut tracker,
                t as u64,
                t as u64,
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                anchor_port,
                now,
            );
        }
        tracker.expire_due(now);
        // shrink_if_idle runs on every non-empty swarm:
        // Vec shrinks from ~300 entries to 1 entry, capacity drops.

        // Sanity check: after expire, only anchors should remain.
        let snap = tracker.snapshot();
        let expected = n_torrents;
        if snap.totals.peers != expected {
            eprintln!();
            eprintln!(
                "  ⚠️  expire left {} peers (expected {}), diff={}",
                snap.totals.peers,
                expected,
                snap.totals.peers.saturating_sub(expected)
            );
        }

        // Let jemalloc / glibc decay freed pages
        std::thread::sleep(Duration::from_secs(2));
        eprint!("  shrink");

        let rss_shrink = rss_mb();

        // ── Regrow ─────────────────────────────────────────────
        let regrow_time = Instant::now();
        for t in 0..n_torrents {
            for j in 0..bulk_per_torrent {
                let seed = (cycle as u64) * 10_000_000_000 + (t * bulk_per_torrent + j) as u64;
                let ip_octet = 2 + (seed % 253) as u8;
                announce(
                    &mut tracker,
                    t as u64,
                    seed + 2_000_000_000,
                    IpAddr::V4(Ipv4Addr::new(10, 1, ip_octet, (seed >> 8) as u8)),
                    6881u16.wrapping_add((seed % 60000) as u16),
                    regrow_time,
                );
            }
        }
        eprint!("  regrow");

        let rss_regrow = rss_mb();

        let overhead = rss_regrow - prev_regrow_rss;
        prev_regrow_rss = rss_regrow;

        println!(
            ",{},{:.1},{},{:.1},{:.1},{:.1}",
            n_torrents, rss_build, snap.totals.peers, rss_shrink, rss_regrow, overhead,
        );
        eprintln!(
            "  {:.1}→{:.1}→{:.1} overhead={:.1}",
            rss_build, rss_shrink, rss_regrow, overhead
        );

        // Wait for regrow peers to age past peer_timeout=1s.
        // This + next cycle's re-anchor overhead gives >1s gap.
        std::thread::sleep(Duration::from_secs(2));
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
