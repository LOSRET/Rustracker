//! RPS (Requests Per Second) benchmark with gradual load ramp-up
//!
//! Pre-loads the tracker at increasing peer scales, then measures
//! announce throughput at each level.  Outputs CSV for CI analysis.
//!
//! Usage: cargo run --release --example rps_bench

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

// ─── Helpers ──────────────────────────────────────────────────────

fn make_info_hash(i: usize) -> InfoHash {
    let mut h = [0u8; 20];
    h[0..8].copy_from_slice(&(i as u64).to_le_bytes());
    InfoHash::new(h)
}

fn make_peer_id(i: usize) -> PeerId {
    let mut pid = [0u8; 20];
    pid[0..8].copy_from_slice(&(i as u64).to_le_bytes());
    PeerId::new(pid)
}

fn make_ip(idx: usize) -> IpAddr {
    if idx % 10 < 3 {
        let mut o = [10, 0, 0, 1];
        o[3] = (idx % 254 + 1) as u8;
        IpAddr::V4(Ipv4Addr::from(o))
    } else {
        IpAddr::V6(Ipv6Addr::new(
            0x2001, 0xdb8, 0, 0,
            (idx >> 48 & 0xffff) as u16,
            (idx >> 32 & 0xffff) as u16,
            (idx >> 16 & 0xffff) as u16,
            (idx & 0xffff) as u16,
        ))
    }
}

/// Load `num_peers` announces into the tracker (30% v4, 70% v6).
fn load_peers(tracker: &mut Tracker, num_torrents: usize, num_peers: usize, now: Instant) {
    let peers_per_torrent = (num_peers / num_torrents).max(1);
    for t in 0..num_torrents {
        let ih = make_info_hash(t);
        for j in 0..peers_per_torrent {
            let idx = t * peers_per_torrent + j;
            if idx >= num_peers { return; }
            let input = AnnounceInput {
                info_hash: ih,
                peer_id: make_peer_id(idx),
                ip: make_ip(idx),
                port: (6881 + idx % 60000) as u16,
                uploaded: 0, downloaded: 0,
                left: if j % 3 == 0 { 0 } else { 1_000_000_000 },
                event: AnnounceEvent::Started,
                numwant: 50,
                client_tag: 1,
            };
            let out = tracker.announce(input, now);
            drop(announce_response(out, true));
        }
    }
}

/// Measure RPS: run `burst_size` announce requests and return (rps, elapsed_ms).
fn measure_rps(tracker: &mut Tracker, num_torrents: usize, burst_size: usize, now: Instant) -> (f64, f64) {
    let t0 = Instant::now();
    for k in 0..burst_size {
        let torrent_idx = k % num_torrents;
        let input = AnnounceInput {
            info_hash: make_info_hash(torrent_idx),
            peer_id: make_peer_id(1_000_000_000 + k), // unique IDs, won't collide with pre-loaded
            ip: make_ip(k),
            port: (40000 + k % 60000) as u16,
            uploaded: 0, downloaded: 0,
            left: 500_000_000,
            event: AnnounceEvent::Started,
            numwant: 50,
            client_tag: 2,
        };
        let out = tracker.announce(input, now);
        drop(announce_response(out, true));
    }
    let elapsed = t0.elapsed();
    let rps = burst_size as f64 / elapsed.as_secs_f64();
    (rps, elapsed.as_secs_f64() * 1000.0)
}

// ─── Main ────────────────────────────────────────────────────────

fn main() {
    // Ramp-up schedule: (torrents, total_peers)
    // Starts small, gradually increases to production scale
    let schedule: Vec<(usize, usize)> = vec![
        (1_000,       10_000),
        (5_000,       50_000),
        (10_000,     200_000),
        (50_000,     500_000),
        (100_000,  1_000_000),
        (200_000,  2_000_000),
        (500_000,  5_000_000),
        (1_000_000,10_000_000),
    ];

    let burst_size: usize = 200_000; // requests per measurement burst
    let interval = Duration::from_secs(1800);
    let timeout = Duration::from_secs(2700);

    let mut tracker = Tracker::new(interval, timeout);
    let now = Instant::now();

    // CSV header
    println!("peers,rps,elapsed_ms,rss_mb");

    let mut loaded_peers: usize = 0;

    for &(torrents, target_peers) in &schedule {
        // Load additional peers to reach target
        let to_load = target_peers.saturating_sub(loaded_peers);
        if to_load > 0 {
            eprint!("Loading {} peers ({} torrents) ... ", to_load, torrents);
            let t0 = Instant::now();
            load_peers(&mut tracker, torrents, to_load, now);
            loaded_peers = target_peers;
            eprintln!("done in {:.1}s", t0.elapsed().as_secs_f64());
        }

        // Measure RPS at this scale
        let (rps, elapsed_ms) = measure_rps(&mut tracker, torrents, burst_size, now);
        let rss = mem::rss_bytes();
        println!("{},{:.0},{:.1},{:.1}", target_peers, rps, elapsed_ms, fmt_mb(rss));

        eprintln!(
            "  peers={:>10}  rps={:>10.0}  elapsed={:>7.1}ms  rss={}",
            target_peers, rps, elapsed_ms, fmt_mb(rss),
        );
    }

    let snapshot = tracker.snapshot();
    eprintln!(
        "\nDone. torrents={} peers={} seeders={} leechers={}",
        snapshot.totals.torrents, snapshot.totals.peers,
        snapshot.totals.seeders, snapshot.totals.leechers,
    );
}
