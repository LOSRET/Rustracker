use std::cmp::Reverse;
use std::collections::{BinaryHeap, BTreeMap};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use super::counters::{ExpireResult, PeerRemoval, PeerUpsert, TrackerCounters};
use super::swarm::{allocate_v4_v6, PackedIpv4Peers, PackedIpv6Peers, Rng};
use super::types::{
    AnnounceEvent, InfoHash, Ipv4PeerKey, Ipv6PeerKey, PeerContact, PeerId, PeerState, TorrentStats,
};

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
    pub peers: Vec<PeerContact>,
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

#[derive(Debug, Default)]
pub(crate) struct Swarm {
    pub(crate) ipv4_peers: PackedIpv4Peers,
    pub(crate) ipv6_peers: PackedIpv6Peers,
    pub(crate) complete: u32,
    pub(crate) downloaded: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PeerEndpoint {
    V4(Ipv4PeerKey),
    V6(Ipv6PeerKey),
}

/// One-pass Top-K for all four rankings (peers, seeders, leechers, downloaded).
/// Four min-heaps share a single iteration over the swarm table.
pub(crate) struct Top100All {
    pub peers: Vec<(InfoHash, usize, usize, u64)>,
    pub seeders: Vec<(InfoHash, usize, usize, u64)>,
    pub leechers: Vec<(InfoHash, usize, usize, u64)>,
    pub downloaded: Vec<(InfoHash, usize, usize, u64)>,
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

        // Detect new torrent before borrowing swarms.
        let is_new_torrent = !self.swarms.contains_key(&info_hash);

        // All swarm operations happen in this block; borrow released before client_counts access
        let (output, pending_decr, pending_incr) = {
            let swarm = self.swarms.entry(info_hash).or_insert_with(Swarm::default);

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

    pub fn scrape(&self, info_hashes: &[InfoHash]) -> std::collections::HashMap<InfoHash, TorrentStats> {
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
        if limit == 0 {
            return Top100All {
                peers: Vec::new(),
                seeders: Vec::new(),
                leechers: Vec::new(),
                downloaded: Vec::new(),
            };
        }

        let mut heap_p: BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>> =
            BinaryHeap::with_capacity(limit);
        let mut heap_s: BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>> =
            BinaryHeap::with_capacity(limit);
        let mut heap_l: BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>> =
            BinaryHeap::with_capacity(limit);
        let mut heap_d: BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>> =
            BinaryHeap::with_capacity(limit);
        let mut min_p: u64 = 0;
        let mut min_s: u64 = 0;
        let mut min_l: u64 = 0;
        let mut min_d: u64 = 0;

        for (info_hash, swarm) in &self.swarms {
            let stats = swarm.stats();
            let peers = (stats.complete + stats.incomplete) as u64;
            let seeders = stats.complete as u64;
            let leechers = stats.incomplete as u64;
            let downloaded = stats.downloaded as u64;

            // Fast path: all four heaps are full and this torrent is
            // below every threshold — skip without constructing entries.
            if heap_p.len() >= limit && peers <= min_p
                && heap_s.len() >= limit && seeders <= min_s
                && heap_l.len() >= limit && leechers <= min_l
                && heap_d.len() >= limit && downloaded <= min_d
            {
                continue;
            }

            let dl = stats.downloaded as u64;
            Self::try_heap_insert(&mut heap_p, &mut min_p, limit, peers, *info_hash, stats.complete, stats.incomplete, dl);
            Self::try_heap_insert(&mut heap_s, &mut min_s, limit, seeders, *info_hash, stats.complete, stats.incomplete, dl);
            Self::try_heap_insert(&mut heap_l, &mut min_l, limit, leechers, *info_hash, stats.complete, stats.incomplete, dl);
            Self::try_heap_insert(&mut heap_d, &mut min_d, limit, downloaded, *info_hash, stats.complete, stats.incomplete, dl);
        }

        Top100All {
            peers: Self::drain_heap_by(heap_p, 0),
            seeders: Self::drain_heap_by(heap_s, 1),
            leechers: Self::drain_heap_by(heap_l, 2),
            downloaded: Self::drain_heap_by(heap_d, 3),
        }
    }

    fn try_heap_insert(
        heap: &mut BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>>,
        min_key: &mut u64,
        limit: usize,
        key: u64,
        info_hash: InfoHash,
        complete: usize,
        incomplete: usize,
        downloaded: u64,
    ) {
        if heap.len() >= limit && key <= *min_key {
            return;
        }
        let entry = Reverse((key, info_hash, complete, incomplete, downloaded));
        if heap.len() < limit {
            heap.push(entry);
            if heap.len() == limit {
                if let Some(peek) = heap.peek() {
                    *min_key = peek.0.0;
                }
            }
        } else {
            heap.pop();
            heap.push(entry);
            if let Some(peek) = heap.peek() {
                *min_key = peek.0.0;
            }
        }
    }

    fn drain_heap_by(
        heap: BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>>,
        sort_field: u8, // 0 = peers (1+2), 1 = seeders, 2 = leechers
    ) -> Vec<(InfoHash, usize, usize, u64)> {
        let mut result: Vec<_> = heap
            .into_iter()
            .map(|Reverse((_, info_hash, seeders, leechers, downloaded))| {
                (info_hash, seeders, leechers, downloaded)
            })
            .collect();
        match sort_field {
            1 => result.sort_by(|a, b| b.1.cmp(&a.1)),
            2 => result.sort_by(|a, b| b.2.cmp(&a.2)),
            3 => result.sort_by(|a, b| b.3.cmp(&a.3)),
            _ => result.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2))),
        }
        result
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
        for (_, swarm) in &self.swarms {
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

impl PeerEndpoint {
    fn new(ip: IpAddr, port: u16) -> Self {
        match ip {
            IpAddr::V4(ip) => Self::V4(Ipv4PeerKey::new(ip, port)),
            IpAddr::V6(ip) => Self::V6(Ipv6PeerKey::new(ip, port)),
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

impl Swarm {
    fn upsert_peer(&mut self, endpoint: PeerEndpoint, peer: PeerState) -> PeerUpsert {
        let now_complete = peer.is_complete();
        let old = match endpoint {
            PeerEndpoint::V4(key) => self.ipv4_peers.insert(key, peer),
            PeerEndpoint::V6(key) => self.ipv6_peers.insert(key, peer),
        };

        match old {
            Some(old_peer) => {
                let was_complete = old_peer.is_complete();
                let old_tag = old_peer.client_tag;
                self.remove_from_counters(&old_peer);
                if now_complete {
                    self.complete = self.complete.saturating_add(1);
                }
                PeerUpsert {
                    is_new_peer: false,
                    was_complete,
                    now_complete,
                    old_tag: Some(old_tag),
                }
            }
            None => {
                if now_complete {
                    self.complete = self.complete.saturating_add(1);
                }
                PeerUpsert {
                    is_new_peer: true,
                    was_complete: false,
                    now_complete,
                    old_tag: None,
                }
            }
        }
    }

    fn remove_peer_tag(&mut self, endpoint: PeerEndpoint) -> Option<PeerRemoval> {
        let removed = match endpoint {
            PeerEndpoint::V4(key) => self.ipv4_peers.remove(&key),
            PeerEndpoint::V6(key) => self.ipv6_peers.remove(&key),
        };

        removed.map(|peer| {
            self.remove_from_counters(&peer);
            PeerRemoval {
                tag: peer.client_tag,
                was_complete: peer.is_complete(),
            }
        })
    }

    fn expire(&mut self, now_secs: u32, timeout_secs: u32) -> ExpireResult {
        let mut expired_complete: usize = 0;
        let mut expired_count: usize = 0;
        let mut expired_tags: Vec<u8> = Vec::new();

        self.ipv4_peers.retain(|_, peer| {
            let keep = now_secs.saturating_sub(peer.last_seen_secs) <= timeout_secs;
            if !keep {
                expired_count += 1;
                if peer.is_complete() {
                    expired_complete += 1;
                }
                expired_tags.push(peer.client_tag);
            }
            keep
        });

        self.ipv6_peers.retain(|_, peer| {
            let keep = now_secs.saturating_sub(peer.last_seen_secs) <= timeout_secs;
            if !keep {
                expired_count += 1;
                if peer.is_complete() {
                    expired_complete += 1;
                }
                expired_tags.push(peer.client_tag);
            }
            keep
        });

        self.complete = self.complete.saturating_sub(expired_complete as u32);

        self.ipv4_peers.shrink_if_idle();
        self.ipv6_peers.shrink_if_idle();

        ExpireResult {
            tags: expired_tags,
            removed_peers: expired_count,
            removed_complete: expired_complete,
        }
    }

    fn remove_from_counters(&mut self, peer: &PeerState) {
        if peer.is_complete() {
            self.complete = self.complete.saturating_sub(1);
        }
    }

    fn stats(&self) -> TorrentStats {
        let complete = self.complete as usize;
        TorrentStats {
            complete,
            downloaded: self.downloaded,
            incomplete: self.len().saturating_sub(complete),
        }
    }

    fn len(&self) -> usize {
        self.ipv4_peers.len() + self.ipv6_peers.len()
    }

    fn is_empty(&self) -> bool {
        self.ipv4_peers.is_empty() && self.ipv6_peers.is_empty()
    }

    fn contacts_excluding(
        &self,
        requesting_endpoint: PeerEndpoint,
        limit: usize,
        rng_seed: u64,
    ) -> Vec<PeerContact> {
        if limit == 0 {
            return Vec::new();
        }

        let total = self.len();
        if total == 0 {
            return Vec::new();
        }

        let v4_exclude = match requesting_endpoint {
            PeerEndpoint::V4(key) => Some(key),
            _ => None,
        };
        let v6_exclude = match requesting_endpoint {
            PeerEndpoint::V6(key) => Some(key),
            _ => None,
        };

        // Short circuit: return all peers except self when swarm is small
        let available = total - 1;
        if available <= limit {
            let mut contacts = Vec::with_capacity(available);
            self.ipv4_peers
                .append_contacts(v4_exclude.as_ref(), &mut contacts);
            self.ipv6_peers
                .append_contacts(v6_exclude.as_ref(), &mut contacts);
            return contacts;
        }

        // Allocate between v4 and v6 proportionally (at least 1/4 each if available)
        // Request one extra per pool that contains the excluded peer, so the
        // final result still has `limit` entries after exclusion.
        let extra = v4_exclude.is_some() as usize + v6_exclude.is_some() as usize;
        let (v4_amount, v6_amount) =
            allocate_v4_v6(self.ipv4_peers.len(), self.ipv6_peers.len(), limit + extra);

        let mut rng = Rng::new(rng_seed);
        let mut contacts = Vec::with_capacity(limit);

        self.ipv4_peers
            .select_random(v4_amount, &mut rng, v4_exclude.as_ref(), &mut contacts);
        self.ipv6_peers
            .select_random(v6_amount, &mut rng, v6_exclude.as_ref(), &mut contacts);

        contacts.truncate(limit);
        contacts
    }
}



#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::{Duration, Instant};

    use super::{AnnounceInput, Tracker};
    use super::super::types::{AnnounceEvent, InfoHash, PeerId};

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

        assert_eq!(first.peers.len(), 3);
        assert_eq!(second.peers.len(), 3);
        assert_ne!(first.peers, second.peers);
        assert!(!first
            .peers
            .iter()
            .any(|contact| contact.ip == request_ip(9) && contact.port == 6881));
        assert!(!second
            .peers
            .iter()
            .any(|contact| contact.ip == request_ip(9) && contact.port == 6881));
    }
}
