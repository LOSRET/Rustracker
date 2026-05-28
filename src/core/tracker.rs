use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use super::counters::{ExpireResult, TrackerCounters};
use super::swarm::{PeerEndpoint, Swarm};
use super::topk::{self, Top100All};
use super::types::{AnnounceEvent, InfoHash, PeerId, PeerState, TorrentStats};

const INTERVAL_JITTER_PERCENT: u64 = 10;
const EXPIRE_SWEEP_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct AnnounceInput {
    pub info_hash: InfoHash,
    pub peer_id: PeerId,
    pub ip: std::net::IpAddr,
    pub port: u16,
    pub uploaded: u64,
    pub downloaded: u64,
    pub left: u64,
    pub event: AnnounceEvent,
    pub numwant: usize,
    pub client_tag: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnounceOutput {
    pub interval: u64,
    pub complete: usize,
    pub incomplete: usize,
    pub downloaded: u32,
    pub peers: (Vec<u8>, Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackerSnapshot {
    pub interval: u64,
    pub peer_timeout: u64,
    pub totals: TrackerTotals,
    pub clients: Vec<(u8, u64)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrackerTotals {
    pub torrents: usize,
    pub peers: usize,
    pub seeders: usize,
    pub leechers: usize,
    pub downloaded: u64,
}

#[derive(Debug)]
pub struct Tracker {
    interval: Duration,
    peer_timeout: Duration,
    started_at: Instant,
    next_expire_at: Instant,
    swarms: BTreeMap<InfoHash, Swarm>,
    client_counts: Vec<(u8, u64)>,
    counters: TrackerCounters,
}

impl Tracker {
    pub fn new(interval: Duration, peer_timeout: Duration) -> Self {
        Self::with_started_at(interval, peer_timeout, Instant::now())
    }

    fn with_started_at(interval: Duration, peer_timeout: Duration, started_at: Instant) -> Self {
        Self {
            interval,
            peer_timeout,
            started_at,
            next_expire_at: started_at,
            swarms: BTreeMap::new(),
            client_counts: Vec::new(),
            counters: TrackerCounters::default(),
        }
    }

    pub fn announce(&mut self, input: AnnounceInput, now: Instant) -> AnnounceOutput {
        let now_secs = self.elapsed_secs(now);

        let info_hash = input.info_hash;
        let requesting_peer_id = input.peer_id;
        let endpoint = PeerEndpoint::new(input.ip, input.port);
        let numwant = input.numwant;
        let new_tag = input.client_tag;

        // All swarm operations happen in this block; borrow released before client_counts access
        let (output, pending_decr, pending_incr) = {
            let entry = self.swarms.entry(info_hash);
            let is_new_torrent = matches!(&entry, Entry::Vacant(_));
            let swarm = entry.or_default();

            if is_new_torrent {
                self.counters.add_torrent();
            }

            let mut decr: Vec<u8> = Vec::new();
            let mut incr: Option<u8> = None;

            match input.event {
                AnnounceEvent::Stopped => {
                    if let Some(removal) = swarm.remove_peer_tag(endpoint) {
                        decr.push(removal.tag);
                        self.counters.apply_removal(&removal);
                        if swarm.is_empty() {
                            // Swarm stays in BTreeMap; will be removed by expire.
                            // No torrent counter change here.
                        }
                    }
                }
                AnnounceEvent::Completed => {
                    let upsert = swarm.upsert_peer(endpoint, input.into_peer_state(now_secs));
                    if !upsert.was_complete {
                        swarm.downloaded = swarm.downloaded.saturating_add(1);
                        self.counters.add_downloaded();
                    }
                    if let Some(tag) = upsert.old_tag {
                        if tag != new_tag {
                            decr.push(tag);
                        }
                    }
                    if upsert.old_tag != Some(new_tag) {
                        incr = Some(new_tag);
                    }
                    self.counters.apply_upsert(&upsert);
                }
                AnnounceEvent::Started | AnnounceEvent::Empty => {
                    let upsert = swarm.upsert_peer(endpoint, input.into_peer_state(now_secs));
                    if let Some(tag) = upsert.old_tag {
                        if tag != new_tag {
                            decr.push(tag);
                        }
                    }
                    if upsert.old_tag != Some(new_tag) {
                        incr = Some(new_tag);
                    }
                    self.counters.apply_upsert(&upsert);
                }
            }

            let stats = swarm.stats();
            let downloaded = swarm.downloaded;
            let peers = swarm.contacts_excluding(
                endpoint,
                numwant,
                jitter_seed(info_hash, requesting_peer_id, now_secs),
            );

            let out = AnnounceOutput {
                interval: jittered_interval_secs(
                    self.interval,
                    info_hash,
                    requesting_peer_id,
                    now_secs,
                ),
                complete: stats.complete,
                incomplete: stats.incomplete,
                downloaded,
                peers,
            };

            (out, decr, incr)
        };

        // Apply client count changes after swarm borrow is released
        for tag in pending_decr {
            self.decr_client(tag);
        }
        if let Some(tag) = pending_incr {
            self.incr_client(tag);
        }

        #[cfg(debug_assertions)]
        self.verify_counters();

        output
    }

    pub fn scrape(&self, info_hashes: &[InfoHash]) -> HashMap<InfoHash, TorrentStats> {
        info_hashes
            .iter()
            .copied()
            .map(|info_hash| {
                let stats = self
                    .swarms
                    .get(&info_hash)
                    .map(|r| r.stats())
                    .unwrap_or_default();
                (info_hash, stats)
            })
            .collect()
    }

    pub(crate) fn top_torrents_all(&self, limit: usize) -> Top100All {
        topk::top_torrents_all(&self.swarms, limit)
    }

    pub fn snapshot(&self) -> TrackerSnapshot {
        let c = &self.counters;
        TrackerSnapshot {
            interval: self.interval.as_secs(),
            peer_timeout: self.peer_timeout.as_secs(),
            totals: TrackerTotals {
                torrents: c.torrents,
                peers: c.peers,
                seeders: c.seeders,
                leechers: c.peers.saturating_sub(c.seeders),
                downloaded: c.downloaded,
            },
            clients: self.client_distribution().to_vec(),
        }
    }

    /// Verify incremental counters match a full traversal (debug builds only).
    #[cfg(debug_assertions)]
    fn verify_counters(&self) {
        let mut torrents = 0usize;
        let mut peers = 0usize;
        let mut seeders = 0usize;
        let mut downloaded = 0u64;
        for swarm in self.swarms.values() {
            let stats = swarm.stats();
            torrents += 1;
            peers += swarm.len();
            seeders += stats.complete;
            downloaded += stats.downloaded as u64;
        }
        self.counters.verify(torrents, peers, seeders, downloaded);
    }

    pub fn client_distribution(&self) -> &[(u8, u64)] {
        &self.client_counts
    }

    fn incr_client(&mut self, tag: u8) {
        match self.client_counts.iter_mut().find(|(t, _)| *t == tag) {
            Some((_, c)) => *c = c.saturating_add(1),
            None => self.client_counts.push((tag, 1)),
        }
    }

    fn decr_client(&mut self, tag: u8) {
        if let Some(pos) = self.client_counts.iter().position(|(t, _)| *t == tag) {
            let count = &mut self.client_counts[pos].1;
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.client_counts.swap_remove(pos);
            }
        }
    }

    pub fn expire_due(&mut self, now: Instant) {
        if now < self.next_expire_at {
            return;
        }

        self.expire(now);
        self.next_expire_at = now + self.expire_sweep_interval();
    }

    fn expire(&mut self, now: Instant) {
        let now_secs = self.elapsed_secs(now);
        let timeout_secs = saturating_u32_secs(self.peer_timeout);
        let mut all_expired_tags: Vec<u8> = Vec::new();
        let mut total_expired_peers: usize = 0;
        let mut total_expired_complete: usize = 0;
        let mut removed_swarms: usize = 0;
        let mut removed_downloaded: u64 = 0;

        self.swarms.retain(|_, swarm| {
            let result = swarm.expire(now_secs, timeout_secs);
            all_expired_tags.extend(result.tags);
            total_expired_peers += result.removed_peers;
            total_expired_complete += result.removed_complete;
            if swarm.is_empty() {
                removed_swarms += 1;
                removed_downloaded += swarm.downloaded as u64;
                false
            } else {
                true
            }
        });

        self.counters.apply_expire(
            &ExpireResult {
                tags: Vec::new(),
                removed_peers: total_expired_peers,
                removed_complete: total_expired_complete,
            },
            removed_swarms,
            removed_downloaded,
        );

        for tag in all_expired_tags {
            self.decr_client(tag);
        }

        #[cfg(debug_assertions)]
        self.verify_counters();
    }

    fn expire_sweep_interval(&self) -> Duration {
        self.peer_timeout
            .min(EXPIRE_SWEEP_INTERVAL)
            .max(Duration::from_secs(1))
    }

    fn elapsed_secs(&self, now: Instant) -> u32 {
        saturating_u32_secs(now.saturating_duration_since(self.started_at))
    }
}

impl AnnounceInput {
    fn into_peer_state(self, now_secs: u32) -> PeerState {
        PeerState {
            complete: self.left == 0,
            last_seen_secs: now_secs,
            client_tag: self.client_tag,
        }
    }
}

fn saturating_u32_secs(duration: Duration) -> u32 {
    duration.as_secs().min(u32::MAX as u64) as u32
}

fn jittered_interval_secs(
    interval: Duration,
    info_hash: InfoHash,
    peer_id: PeerId,
    now_secs: u32,
) -> u64 {
    let base = interval.as_secs().max(1);
    let jitter = (base.saturating_mul(INTERVAL_JITTER_PERCENT) / 100).max(1);
    let span = jitter.saturating_mul(2).saturating_add(1);
    let offset = (jitter_seed(info_hash, peer_id, now_secs) % span) as i128 - jitter as i128;
    (base as i128 + offset).max(1) as u64
}

fn jitter_seed(info_hash: InfoHash, peer_id: PeerId, now_secs: u32) -> u64 {
    let mut seed = 0xcbf2_9ce4_8422_2325_u64 ^ u64::from(now_secs);

    for byte in info_hash.0.into_iter().chain(peer_id.0) {
        seed ^= u64::from(byte);
        seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
    }

    seed
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::{Duration, Instant};

    use super::super::types::{AnnounceEvent, InfoHash, PeerId};
    use super::{AnnounceInput, Tracker};

    fn hash(byte: u8) -> InfoHash {
        InfoHash([byte; 20])
    }

    fn peer(byte: u8) -> PeerId {
        PeerId([byte; 20])
    }

    fn request_ip(byte: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, byte))
    }

    fn announce(peer_id: PeerId, left: u64, event: AnnounceEvent) -> AnnounceInput {
        AnnounceInput {
            info_hash: hash(1),
            peer_id,
            ip: request_ip(peer_id.0[0]),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left,
            event,
            numwant: 50,
            client_tag: 0,
        }
    }

    fn tracker(now: Instant, peer_timeout: Duration) -> Tracker {
        Tracker::with_started_at(Duration::from_secs(1800), peer_timeout, now)
    }

    #[test]
    fn tracks_complete_and_incomplete_peers() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(3600));

        let first = tracker.announce(announce(peer(1), 10, AnnounceEvent::Started), now);
        assert_eq!(first.complete, 0);
        assert_eq!(first.incomplete, 1);

        let second = tracker.announce(announce(peer(2), 0, AnnounceEvent::Started), now);
        assert_eq!(second.complete, 1);
        assert_eq!(second.incomplete, 1);
    }

    #[test]
    fn removes_stopped_peer() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(3600));

        tracker.announce(announce(peer(1), 10, AnnounceEvent::Started), now);
        let output = tracker.announce(announce(peer(1), 10, AnnounceEvent::Stopped), now);

        assert_eq!(output.complete, 0);
        assert_eq!(output.incomplete, 0);
    }

    #[test]
    fn updates_counters_when_peer_changes_completion_state() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(3600));

        tracker.announce(announce(peer(1), 10, AnnounceEvent::Started), now);
        let output = tracker.announce(announce(peer(1), 0, AnnounceEvent::Completed), now);

        assert_eq!(output.complete, 1);
        assert_eq!(output.incomplete, 0);
    }

    #[test]
    fn expires_old_peers() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(1));

        tracker.announce(announce(peer(1), 10, AnnounceEvent::Started), now);
        tracker.expire_due(now + Duration::from_secs(2));
        let stats = tracker.scrape(&[hash(1)]);

        assert_eq!(stats[&hash(1)].complete, 0);
        assert_eq!(stats[&hash(1)].incomplete, 0);
    }

    #[test]
    fn creates_dashboard_snapshot() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(3600));

        tracker.announce(announce(peer(1), 0, AnnounceEvent::Started), now);
        tracker.announce(announce(peer(2), 10, AnnounceEvent::Started), now);
        let snapshot = tracker.snapshot();

        assert_eq!(snapshot.totals.torrents, 1);
        assert_eq!(snapshot.totals.peers, 2);
        assert_eq!(snapshot.totals.seeders, 1);
        assert_eq!(snapshot.totals.leechers, 1);
    }

    #[test]
    fn jitters_announce_interval_within_bounds() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(3600));

        let output = tracker.announce(announce(peer(1), 10, AnnounceEvent::Started), now);

        assert!((1620..=1980).contains(&output.interval));
    }

    #[test]
    fn jitters_announce_interval_between_peers() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(3600));

        let first = tracker.announce(announce(peer(1), 10, AnnounceEvent::Started), now);
        let second = tracker.announce(announce(peer(2), 10, AnnounceEvent::Started), now);

        assert_ne!(first.interval, second.interval);
    }

    #[test]
    fn handles_large_swarms_after_peer_store_upgrade() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(3600));

        for peer_number in 0..60 {
            tracker.announce(announce(peer(peer_number), 10, AnnounceEvent::Started), now);
        }

        let completed = tracker.announce(announce(peer(59), 0, AnnounceEvent::Completed), now);
        assert_eq!(completed.complete, 1);
        assert_eq!(completed.incomplete, 59);

        let stopped = tracker.announce(announce(peer(59), 0, AnnounceEvent::Stopped), now);
        assert_eq!(stopped.complete, 0);
        assert_eq!(stopped.incomplete, 59);
    }

    #[test]
    fn rotates_selected_peers_between_time_windows() {
        let now = Instant::now();
        let mut tracker = tracker(now, Duration::from_secs(3600));

        for peer_number in 1..=8 {
            tracker.announce(announce(peer(peer_number), 10, AnnounceEvent::Started), now);
        }

        let mut request = announce(peer(9), 10, AnnounceEvent::Started);
        request.numwant = 3;
        tracker.announce(request.clone(), now);

        let first = tracker.announce(request.clone(), now + Duration::from_secs(1));
        let second = tracker.announce(request, now + Duration::from_secs(60));

        let num_v4_peers = first.peers.0.len() / 6 + first.peers.1.len() / 18;
        assert_eq!(num_v4_peers, 3);
        let num_v4_peers = second.peers.0.len() / 6 + second.peers.1.len() / 18;
        assert_eq!(num_v4_peers, 3);
        assert_ne!(first.peers, second.peers);
        let self_bytes = [127, 0, 0, 9, 0x1a, 0xe1];
        assert!(!first.peers.0.windows(6).any(|w| w == self_bytes));
        assert!(!first.peers.1.windows(18).any(|w| w[..6] == self_bytes));
        assert!(!second.peers.0.windows(6).any(|w| w == self_bytes));
        assert!(!second.peers.1.windows(18).any(|w| w[..6] == self_bytes));
    }
}
