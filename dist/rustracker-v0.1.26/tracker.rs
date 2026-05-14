use std::collections::HashMap;
use std::hash::Hash;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::types::{
    AnnounceEvent, InfoHash, Ipv4PeerKey, Ipv6PeerKey, PeerContact, PeerId, PeerState, TorrentStats,
};

const INTERVAL_JITTER_PERCENT: u64 = 10;
const SMALL_SWARM_LIMIT: usize = 50;
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnounceOutput {
    pub interval: u64,
    pub complete: usize,
    pub incomplete: usize,
    pub peers: Vec<PeerContact>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackerSnapshot {
    pub interval: u64,
    pub peer_timeout: u64,
    pub totals: TrackerTotals,
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
    swarms: HashMap<InfoHash, Swarm>,
}

#[derive(Debug, Default)]
struct Swarm {
    ipv4_peers: PeerStore<Ipv4PeerKey>,
    ipv6_peers: PeerStore<Ipv6PeerKey>,
    complete: usize,
    incomplete: usize,
    downloaded: u64,
}

#[derive(Debug)]
enum PeerStore<K> {
    Small(Vec<(K, PeerState)>),
    Large(HashMap<K, PeerState>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PeerEndpoint {
    V4(Ipv4PeerKey),
    V6(Ipv6PeerKey),
}

impl<K> Default for PeerStore<K> {
    fn default() -> Self {
        Self::Small(Vec::new())
    }
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
            swarms: HashMap::new(),
        }
    }

    pub fn announce(&mut self, input: AnnounceInput, now: Instant) -> AnnounceOutput {
        let now_secs = self.elapsed_secs(now);

        let info_hash = input.info_hash;
        let requesting_peer_id = input.peer_id;
        let endpoint = PeerEndpoint::new(input.ip, input.port);
        let numwant = input.numwant;

        let swarm = self.swarms.entry(info_hash).or_default();
        let was_complete = swarm.peer_state(endpoint).map(PeerState::is_complete);

        match input.event {
            AnnounceEvent::Stopped => {
                swarm.remove_peer(endpoint);
            }
            AnnounceEvent::Completed => {
                if was_complete != Some(true) {
                    swarm.downloaded = swarm.downloaded.saturating_add(1);
                }
                swarm.upsert_peer(endpoint, input.into_peer_state(now_secs));
            }
            AnnounceEvent::Started | AnnounceEvent::Empty => {
                swarm.upsert_peer(endpoint, input.into_peer_state(now_secs));
            }
        }

        let stats = swarm.stats();
        let peers = swarm.contacts_except(
            endpoint,
            numwant,
            peer_selection_seed(info_hash, requesting_peer_id, now_secs),
        );

        AnnounceOutput {
            interval: jittered_interval_secs(
                self.interval,
                info_hash,
                requesting_peer_id,
                now_secs,
            ),
            complete: stats.complete,
            incomplete: stats.incomplete,
            peers,
        }
    }

    pub fn scrape(&self, info_hashes: &[InfoHash]) -> HashMap<InfoHash, TorrentStats> {
        info_hashes
            .iter()
            .copied()
            .map(|info_hash| {
                let stats = self
                    .swarms
                    .get(&info_hash)
                    .map(Swarm::stats)
                    .unwrap_or_default();
                (info_hash, stats)
            })
            .collect()
    }

    pub fn snapshot(&self) -> TrackerSnapshot {
        let totals = self
            .swarms
            .values()
            .fold(TrackerTotals::default(), |mut totals, swarm| {
                let stats = swarm.stats();
                totals.torrents += 1;
                totals.seeders += stats.complete;
                totals.leechers += stats.incomplete;
                totals.peers += swarm.len();
                totals.downloaded = totals.downloaded.saturating_add(stats.downloaded);
                totals
            });

        TrackerSnapshot {
            interval: self.interval.as_secs(),
            peer_timeout: self.peer_timeout.as_secs(),
            totals,
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
        self.swarms.retain(|_, swarm| {
            swarm.expire(now_secs, timeout_secs);
            !swarm.is_empty()
        });
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

fn peer_selection_seed(info_hash: InfoHash, peer_id: PeerId, now_secs: u32) -> u64 {
    let window = now_secs / 60;
    jitter_seed(info_hash, peer_id, window)
}

impl Swarm {
    fn peer_state(&self, endpoint: PeerEndpoint) -> Option<&PeerState> {
        match endpoint {
            PeerEndpoint::V4(key) => self.ipv4_peers.get(&key),
            PeerEndpoint::V6(key) => self.ipv6_peers.get(&key),
        }
    }

    fn upsert_peer(&mut self, endpoint: PeerEndpoint, peer: PeerState) {
        let is_complete = peer.is_complete();
        let old = match endpoint {
            PeerEndpoint::V4(key) => self.ipv4_peers.insert(key, peer),
            PeerEndpoint::V6(key) => self.ipv6_peers.insert(key, peer),
        };

        if let Some(old_peer) = old {
            self.remove_from_counters(&old_peer);
        }

        if is_complete {
            self.complete = self.complete.saturating_add(1);
        } else {
            self.incomplete = self.incomplete.saturating_add(1);
        }
    }

    fn remove_peer(&mut self, endpoint: PeerEndpoint) {
        let removed = match endpoint {
            PeerEndpoint::V4(key) => self.ipv4_peers.remove(&key),
            PeerEndpoint::V6(key) => self.ipv6_peers.remove(&key),
        };

        if let Some(peer) = removed {
            self.remove_from_counters(&peer);
        }
    }

    fn expire(&mut self, now_secs: u32, timeout_secs: u32) {
        let mut expired = Vec::new();

        self.ipv4_peers.retain(|_, peer| {
            let keep = now_secs.saturating_sub(peer.last_seen_secs) <= timeout_secs;
            if !keep {
                expired.push(peer.is_complete());
            }
            keep
        });
        self.ipv6_peers.retain(|_, peer| {
            let keep = now_secs.saturating_sub(peer.last_seen_secs) <= timeout_secs;
            if !keep {
                expired.push(peer.is_complete());
            }
            keep
        });

        for complete in expired {
            if complete {
                self.complete = self.complete.saturating_sub(1);
            } else {
                self.incomplete = self.incomplete.saturating_sub(1);
            }
        }
    }

    fn remove_from_counters(&mut self, peer: &PeerState) {
        if peer.is_complete() {
            self.complete = self.complete.saturating_sub(1);
        } else {
            self.incomplete = self.incomplete.saturating_sub(1);
        }
    }

    fn stats(&self) -> TorrentStats {
        TorrentStats {
            complete: self.complete,
            downloaded: self.downloaded,
            incomplete: self.incomplete,
        }
    }

    fn len(&self) -> usize {
        self.ipv4_peers.len() + self.ipv6_peers.len()
    }

    fn is_empty(&self) -> bool {
        self.ipv4_peers.is_empty() && self.ipv6_peers.is_empty()
    }

    fn contacts_except(
        &self,
        requesting_endpoint: PeerEndpoint,
        limit: usize,
        seed: u64,
    ) -> Vec<PeerContact> {
        if limit == 0 {
            return Vec::new();
        }

        let mut contacts = Vec::with_capacity(limit.min(self.len().saturating_sub(1)));
        let ipv4_start = self.ipv4_peers.seeded_start(seed);
        let ipv6_start = self.ipv6_peers.seeded_start(seed.rotate_left(17));

        let prefer_ipv6 = matches!(requesting_endpoint, PeerEndpoint::V6(_));
        if prefer_ipv6 {
            self.ipv6_peers
                .contacts_except(requesting_endpoint, ipv6_start, limit, &mut contacts);
            self.ipv4_peers
                .contacts_except(requesting_endpoint, ipv4_start, limit, &mut contacts);
        } else {
            self.ipv4_peers
                .contacts_except(requesting_endpoint, ipv4_start, limit, &mut contacts);
            self.ipv6_peers
                .contacts_except(requesting_endpoint, ipv6_start, limit, &mut contacts);
        }

        contacts
    }
}

impl<K> PeerStore<K>
where
    K: Copy + Eq + Hash,
{
    fn get(&self, key: &K) -> Option<&PeerState> {
        match self {
            Self::Small(peers) => peers
                .iter()
                .find_map(|(stored_key, peer)| (stored_key == key).then_some(peer)),
            Self::Large(peers) => peers.get(key),
        }
    }

    fn insert(&mut self, key: K, peer: PeerState) -> Option<PeerState> {
        match self {
            Self::Small(peers) => {
                if let Some((_, stored_peer)) =
                    peers.iter_mut().find(|(stored_key, _)| *stored_key == key)
                {
                    return Some(std::mem::replace(stored_peer, peer));
                }

                if peers.len() < SMALL_SWARM_LIMIT {
                    peers.push((key, peer));
                    return None;
                }

                let mut large = HashMap::with_capacity(peers.len() + 1);
                large.extend(peers.drain(..));
                large.insert(key, peer);
                *self = Self::Large(large);
                None
            }
            Self::Large(peers) => peers.insert(key, peer),
        }
    }

    fn remove(&mut self, key: &K) -> Option<PeerState> {
        match self {
            Self::Small(peers) => peers
                .iter()
                .position(|(stored_key, _)| stored_key == key)
                .map(|index| peers.swap_remove(index).1),
            Self::Large(peers) => peers.remove(key),
        }
    }

    fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&K, &PeerState) -> bool,
    {
        match self {
            Self::Small(peers) => peers.retain(|(key, peer)| keep(key, peer)),
            Self::Large(peers) => peers.retain(|key, peer| keep(key, peer)),
        }
    }

    fn iter(&self) -> PeerStoreIter<'_, K> {
        match self {
            Self::Small(peers) => PeerStoreIter::Small(peers.iter()),
            Self::Large(peers) => PeerStoreIter::Large(peers.iter()),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Small(peers) => peers.len(),
            Self::Large(peers) => peers.len(),
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn seeded_start(&self, seed: u64) -> usize {
        if self.is_empty() {
            0
        } else {
            (seed as usize) % self.len()
        }
    }
}

impl PeerStore<Ipv4PeerKey> {
    fn contacts_except(
        &self,
        requesting_endpoint: PeerEndpoint,
        start: usize,
        limit: usize,
        contacts: &mut Vec<PeerContact>,
    ) {
        let candidates = self.iter().collect::<Vec<_>>();
        if candidates.is_empty() {
            return;
        }

        for offset in 0..candidates.len() {
            let (key, _) = candidates[(start + offset) % candidates.len()];
            if requesting_endpoint == PeerEndpoint::V4(key) {
                continue;
            }

            contacts.push(key.contact());

            if contacts.len() == limit {
                break;
            }
        }
    }
}

impl PeerStore<Ipv6PeerKey> {
    fn contacts_except(
        &self,
        requesting_endpoint: PeerEndpoint,
        start: usize,
        limit: usize,
        contacts: &mut Vec<PeerContact>,
    ) {
        let candidates = self.iter().collect::<Vec<_>>();
        if candidates.is_empty() {
            return;
        }

        for offset in 0..candidates.len() {
            let (key, _) = candidates[(start + offset) % candidates.len()];
            if requesting_endpoint == PeerEndpoint::V6(key) {
                continue;
            }

            contacts.push(key.contact());

            if contacts.len() == limit {
                break;
            }
        }
    }
}

enum PeerStoreIter<'a, K> {
    Small(std::slice::Iter<'a, (K, PeerState)>),
    Large(std::collections::hash_map::Iter<'a, K, PeerState>),
}

impl<'a, K> Iterator for PeerStoreIter<'a, K>
where
    K: Copy,
{
    type Item = (K, &'a PeerState);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Small(iter) => iter.next().map(|(key, peer)| (*key, peer)),
            Self::Large(iter) => iter.next().map(|(key, peer)| (*key, peer)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::{Duration, Instant};

    use super::{AnnounceInput, Tracker};
    use crate::types::{AnnounceEvent, InfoHash, PeerId};

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
