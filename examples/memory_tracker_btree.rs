//! Real-tracker memory benchmark using BTreeMap (for CI comparison)
//!
//! Mirrors memory_tracker_bench but uses BTreeMap instead of DashMap.
//! Tracks the same production-pattern announce traffic, samples RSS.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use rustracker::tracker::{AnnounceInput, Tracker};
use rustracker::types::{AnnounceEvent, InfoHash, PeerId};

// ─── RSS ─────────────────────────────────────────────────────────

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
#[cfg(windows)]
mod mem {
    use std::mem;
    #[repr(C)] struct PMC { cb:u32,pfc:u32,pws:usize,ws:usize,qppp:usize,qpp:usize,qpnp:usize,qnp:usize,pf:usize,ppf:usize }
    #[link(name="psapi")] extern "system" { fn GetCurrentProcess()->isize; fn GetProcessMemoryInfo(p:isize,c:*mut PMC,b:u32)->i32; }
    pub fn rss_bytes()->usize { unsafe { let mut i:PMC=mem::zeroed(); i.cb=mem::size_of::<PMC>() as u32; GetProcessMemoryInfo(GetCurrentProcess(),&mut i,i.cb); i.ws } }
}

fn fmt_mb(b: usize) -> String { format!("{:.1}",b as f64/(1024.0*1024.0)) }

// ─── Distribution ────────────────────────────────────────────────

fn gen_distribution(torrents: usize, total_peers: usize, max_peers: usize) -> (Vec<InfoHash>, Vec<usize>) {
    let c = (max_peers - 1) as f64;
    let mut lo = 0.01f64; let mut hi = 2.0f64;
    let mut counts: Vec<usize> = Vec::new();
    for _ in 0..50 {
        let alpha = (lo + hi) / 2.0; let mut sum: usize = 0; counts.clear();
        for rank in 1..=torrents {
            let extra = (c / (rank as f64).powf(alpha)) as usize;
            let n = 1 + extra.min(max_peers - 1); counts.push(n); sum += n;
            if sum > total_peers + 5000 { break; }
        }
        if sum < total_peers { hi = alpha; } else if sum > total_peers + 100 { lo = alpha; } else { break; }
    }
    while counts.len() < torrents { counts.push(1); } counts.truncate(torrents);
    if counts.iter().sum::<usize>() > total_peers {
        let mut excess = counts.iter().sum::<usize>() - total_peers;
        for i in (0..counts.len()).rev() { if excess==0{break} if counts[i]>1 { let r=excess.min(counts[i]-1); counts[i]-=r; excess-=r; } }
    }
    let hashes: Vec<InfoHash> = (0..torrents).map(|i|{ let mut h=[0u8;20]; h[0..8].copy_from_slice(&(i as u64).to_le_bytes()); InfoHash::new(h) }).collect();
    (hashes, counts)
}

// ─── BTreeMap-based tracker (lightweight, mimics announce path) ──

struct BTreeTracker {
    swarms: BTreeMap<InfoHash, SwarmState>,
}

#[derive(Default)]
struct SwarmState {
    seeders: usize,
    leechers: usize,
    peers: Vec<u8>, // flat peer storage like PackedIpv4Peers
}

impl BTreeTracker {
    fn new() -> Self { Self { swarms: BTreeMap::new() } }

    fn announce(&mut self, input: AnnounceInput) {
        use std::collections::btree_map::Entry;
        let swarm = self.swarms.entry(input.info_hash).or_default();
        let is_seeder = input.left == 0;
        // Push 12-byte peer entry (matching PackedIpv4Peers format)
        match input.ip {
            IpAddr::V4(ip) => {
                swarm.peers.extend_from_slice(&ip.octets());
                swarm.peers.extend_from_slice(&input.port.to_be_bytes());
                swarm.peers.push(if is_seeder { 0x80 } else { 0 });
                swarm.peers.push(input.client_tag);
                swarm.peers.extend_from_slice(&0u32.to_be_bytes()); // last_seen placeholder
            }
            IpAddr::V6(_) => {} // skip v6 for brevity
        }
        if is_seeder { swarm.seeders += 1; } else { swarm.leechers += 1; }
    }
}

// ─── Main ────────────────────────────────────────────────────────

fn main() {
    let (hashes, counts) = gen_distribution(10_000_000, 25_000_000, 500);
    let n = hashes.len();
    let total_peers: usize = counts.iter().sum();
    eprintln!("btree torrents={} peers={}", n, total_peers);

    let now = Instant::now();
    let mut tracker = BTreeTracker::new();

    let base_ip = Ipv4Addr::new(10, 0, 0, 1);
    let base_port: u16 = 6881;
    let sample_every = (n / 40).max(1);

    println!("label,torrents_done,peers_added,rss_mb,delta_mb");

    let baseline = mem::rss_bytes();
    let mut last_rss = baseline;
    let mut peers_done = 0usize;

    for i in 0..n {
        let count = counts[i];
        for j in 0..count {
            let mut ip = base_ip.octets();
            ip[3] = ((peers_done + j) % 254 + 1) as u8;
            let mut pid = [0u8; 20];
            pid[0..8].copy_from_slice(&((peers_done + j) as u64).to_le_bytes());
            tracker.announce(AnnounceInput {
                info_hash: hashes[i],
                peer_id: PeerId::new(pid),
                ip: IpAddr::V4(Ipv4Addr::from(ip)),
                port: (base_port as usize + (peers_done + j) % 60000) as u16,
                uploaded: 0, downloaded: 0,
                left: if j % 3 == 0 { 0 } else { 1_000_000_000 },
                event: AnnounceEvent::Started,
                numwant: 50, client_tag: 1,
            });
        }
        peers_done += count;

        if (i + 1) % sample_every == 0 {
            let rss = mem::rss_bytes();
            let delta = rss.saturating_sub(last_rss);
            println!("btree,{},{},{},{}", i + 1, peers_done, fmt_mb(rss.saturating_sub(baseline)), fmt_mb(delta));
            last_rss = rss;
        }
    }

    let final_rss = mem::rss_bytes().saturating_sub(baseline);
    eprintln!("[btree] done rss={}", fmt_mb(final_rss));
}
