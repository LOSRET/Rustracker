use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::types::{
    AnnounceEvent, InfoHash, Ipv4PeerKey, Ipv6PeerKey, PeerContact, PeerId, PeerState, TorrentStats,
};

const INTERVAL_JITTER_PERCENT: u64 = 10;
const EXPIRE_SWEEP_INTERVAL: Duration = Duration::from_secs(30);
const FLAG_COMPLETE: u8 = 1;
const IPV4_ENTRY_LEN: usize = 12;
const IPV6_ENTRY_LEN: usize = 24;

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
}

#[derive(Debug, Default)]
struct Swarm {
    ipv4_peers: PackedIpv4Peers,
    ipv6_peers: PackedIpv6Peers,
    complete: u32,
    downloaded: u32,
}

#[derive(Debug, Default)]
struct PackedIpv4Peers {
    bytes: Vec<u8>,
}

#[derive(Debug, Default)]
struct PackedIpv6Peers {
    bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PeerEndpoint {
    V4(Ipv4PeerKey),
    V6(Ipv6PeerKey),
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
            let swarm = self.swarms.entry(info_hash).or_default();

            let mut decr: Vec<u8> = Vec::new();
            let mut incr: Option<u8> = None;

            match input.event {
                AnnounceEvent::Stopped => {
                    if let Some(tag) = swarm.remove_peer_tag(endpoint) {
                        decr.push(tag);
                    }
                }
                AnnounceEvent::Completed => {
                    let old_peer = swarm.upsert_peer(endpoint, input.into_peer_state(now_secs));
                    let was_complete = old_peer.as_ref().map_or(false, |p| p.is_complete());
                    if !was_complete {
                        swarm.downloaded = swarm.downloaded.saturating_add(1);
                    }
                    let old_tag = old_peer.as_ref().map(|p| p.client_tag);
                    if let Some(tag) = old_tag {
                        if tag != new_tag {
                            decr.push(tag);
                        }
                    }
                    if old_tag != Some(new_tag) {
                        incr = Some(new_tag);
                    }
                }
                AnnounceEvent::Started | AnnounceEvent::Empty => {
                    let old_peer = swarm.upsert_peer(endpoint, input.into_peer_state(now_secs));
                    let old_tag = old_peer.as_ref().map(|p| p.client_tag);
                    if let Some(tag) = old_tag {
                        if tag != new_tag {
                            decr.push(tag);
                        }
                    }
                    if old_tag != Some(new_tag) {
                        incr = Some(new_tag);
                    }
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

        output
    }

    pub fn scrape(&self, info_hashes: &[InfoHash]) -> BTreeMap<InfoHash, TorrentStats> {
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

    pub fn top_torrents(&self, sort_by: &str, limit: usize) -> Vec<(InfoHash, usize, usize, u64)> {
        if limit == 0 {
            return Vec::new();
        }

        // Min-heap via Reverse: smallest sort key stays at the top.
        // Heap capacity is `limit`, so memory is O(limit) instead of O(N).
        let mut heap: BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>> =
            BinaryHeap::with_capacity(limit);

        for (info_hash, swarm) in &self.swarms {
            let stats = swarm.stats();
            let key: u64 = match sort_by {
                "seeders" => stats.complete as u64,
                "leechers" => stats.incomplete as u64,
                "downloaded" => stats.downloaded as u64,
                _ => (stats.complete + stats.incomplete) as u64,
            };

            let entry = Reverse((key, *info_hash, stats.complete, stats.incomplete, stats.downloaded as u64));

            if heap.len() < limit {
                heap.push(entry);
            } else {
                let min_key = heap.peek().unwrap().0 .0;
                if key > min_key {
                    heap.pop();
                    heap.push(entry);
                }
            }
        }

        let mut result: Vec<_> = heap
            .into_iter()
            .map(|Reverse((_, info_hash, seeders, leechers, downloaded))| {
                (info_hash, seeders, leechers, downloaded)
            })
            .collect();

        match sort_by {
            "seeders" => result.sort_by(|a, b| b.1.cmp(&a.1)),
            "leechers" => result.sort_by(|a, b| b.2.cmp(&a.2)),
            "downloaded" => result.sort_by(|a, b| b.3.cmp(&a.3)),
            _ => result.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2))),
        }

        result
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
                totals.downloaded = totals.downloaded.saturating_add(stats.downloaded as u64);
                totals
            });

        let clients = self.client_distribution().to_vec();

        TrackerSnapshot {
            interval: self.interval.as_secs(),
            peer_timeout: self.peer_timeout.as_secs(),
            totals,
            clients,
        }
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
        self.swarms.retain(|_, swarm| {
            let expired = swarm.expire(now_secs, timeout_secs);
            all_expired_tags.extend(expired);
            !swarm.is_empty()
        });
        for tag in all_expired_tags {
            self.decr_client(tag);
        }

        // BTreeMap nodes are deallocated on removal; no shrink needed.
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
    fn upsert_peer(&mut self, endpoint: PeerEndpoint, peer: PeerState) -> Option<PeerState> {
        let is_complete = peer.is_complete();
        let old = match endpoint {
            PeerEndpoint::V4(key) => self.ipv4_peers.insert(key, peer),
            PeerEndpoint::V6(key) => self.ipv6_peers.insert(key, peer),
        };

        if let Some(ref old_peer) = old {
            self.remove_from_counters(old_peer);
        }

        if is_complete {
            self.complete = self.complete.saturating_add(1);
        }

        old
    }

    fn remove_peer_tag(&mut self, endpoint: PeerEndpoint) -> Option<u8> {
        let removed = match endpoint {
            PeerEndpoint::V4(key) => self.ipv4_peers.remove(&key),
            PeerEndpoint::V6(key) => self.ipv6_peers.remove(&key),
        };

        removed.map(|peer| {
            self.remove_from_counters(&peer);
            peer.client_tag
        })
    }

    fn expire(&mut self, now_secs: u32, timeout_secs: u32) -> Vec<u8> {
        let mut expired_complete: usize = 0;
        let mut expired_tags: Vec<u8> = Vec::new();

        self.ipv4_peers.retain(|_, peer| {
            let keep = now_secs.saturating_sub(peer.last_seen_secs) <= timeout_secs;
            if !keep {
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

        expired_tags
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

impl PackedIpv4Peers {
    fn insert(&mut self, key: Ipv4PeerKey, peer: PeerState) -> Option<PeerState> {
        if let Some(index) = self.find(&key) {
            let old = self.state_at(index);
            self.write_at(index, key, peer);
            Some(old)
        } else {
            self.push(key, peer);
            None
        }
    }

    fn remove(&mut self, key: &Ipv4PeerKey) -> Option<PeerState> {
        self.find(key).map(|index| self.swap_remove(index))
    }

    fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(Ipv4PeerKey, PeerState) -> bool,
    {
        let mut index = 0;
        while index < self.len() {
            let key = self.key_at(index);
            let peer = self.state_at(index);
            if keep(key, peer) {
                index += 1;
            } else {
                self.swap_remove(index);
            }
        }
    }

    fn len(&self) -> usize {
        self.bytes.len() / IPV4_ENTRY_LEN
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn shrink_if_idle(&mut self) {
        if self.bytes.is_empty() {
            self.bytes = Vec::new();
            return;
        }
        if self.bytes.capacity() > IPV4_ENTRY_LEN * 32
            && self.bytes.len() * 9 < self.bytes.capacity() * 10
        {
            self.bytes.shrink_to_fit();
        }
    }

    fn append_contacts(&self, exclude: Option<&Ipv4PeerKey>, contacts: &mut Vec<PeerContact>) {
        for index in 0..self.len() {
            let key = self.key_at(index);
            if exclude.map_or(true, |ex| &key != ex) {
                contacts.push(key.contact());
            }
        }
    }

    fn select_random(
        &self,
        count: usize,
        rng: &mut Rng,
        exclude: Option<&Ipv4PeerKey>,
        contacts: &mut Vec<PeerContact>,
    ) {
        let total = self.len();
        if total == 0 || count == 0 {
            return;
        }

        if count >= total {
            self.append_contacts(exclude, contacts);
            return;
        }

        // Fixed-point even-spacing random selection (OpenTracker style)
        let mut shifted_total = total as u64;
        let mut shift: u32 = 0;
        while shifted_total < (1u64 << 62) {
            shifted_total <<= 1;
            shift += 1;
        }
        let shifted_step = shifted_total / count as u64;

        let mut pos = rng.next_usize(total);

        for remaining in (0..count).rev() {
            let diff = (((remaining as u64 + 1) * shifted_step) >> shift)
                .saturating_sub(((remaining as u64) * shifted_step) >> shift);
            let advance = 1 + if diff > 1 {
                rng.next_usize(diff as usize)
            } else {
                0
            };
            pos = (pos + advance) % total;

            let key = self.key_at(pos);
            if exclude.map_or(true, |ex| &key != ex) {
                contacts.push(key.contact());
            }
        }
    }

    fn find(&self, key: &Ipv4PeerKey) -> Option<usize> {
        (0..self.len()).find(|&index| self.key_at(index) == *key)
    }

    fn push(&mut self, key: Ipv4PeerKey, peer: PeerState) {
        self.bytes.extend_from_slice(&key.ip);
        self.bytes.extend_from_slice(&key.port.to_be_bytes());
        self.bytes.push(flags(&peer));
        self.bytes.push(peer.client_tag);
        self.bytes
            .extend_from_slice(&peer.last_seen_secs.to_be_bytes());
    }

    fn write_at(&mut self, index: usize, key: Ipv4PeerKey, peer: PeerState) {
        let entry = &mut self.bytes[ipv4_range(index)];
        entry[0..4].copy_from_slice(&key.ip);
        entry[4..6].copy_from_slice(&key.port.to_be_bytes());
        entry[6] = flags(&peer);
        entry[7] = peer.client_tag;
        entry[8..12].copy_from_slice(&peer.last_seen_secs.to_be_bytes());
    }

    fn swap_remove(&mut self, index: usize) -> PeerState {
        let removed = self.state_at(index);
        let last = self.len() - 1;
        if index != last {
            let mut replacement = [0_u8; IPV4_ENTRY_LEN];
            replacement.copy_from_slice(&self.bytes[ipv4_range(last)]);
            self.bytes[ipv4_range(index)].copy_from_slice(&replacement);
        }
        self.bytes.truncate(last * IPV4_ENTRY_LEN);
        removed
    }

    fn key_at(&self, index: usize) -> Ipv4PeerKey {
        let entry = &self.bytes[ipv4_range(index)];
        Ipv4PeerKey {
            ip: [entry[0], entry[1], entry[2], entry[3]],
            port: u16::from_be_bytes([entry[4], entry[5]]),
        }
    }

    fn state_at(&self, index: usize) -> PeerState {
        let entry = &self.bytes[ipv4_range(index)];
        PeerState {
            complete: entry[6] & FLAG_COMPLETE != 0,
            last_seen_secs: u32::from_be_bytes([entry[8], entry[9], entry[10], entry[11]]),
            client_tag: entry[7],
        }
    }
}

impl PackedIpv6Peers {
    fn insert(&mut self, key: Ipv6PeerKey, peer: PeerState) -> Option<PeerState> {
        if let Some(index) = self.find(&key) {
            let old = self.state_at(index);
            self.write_at(index, key, peer);
            Some(old)
        } else {
            self.push(key, peer);
            None
        }
    }

    fn remove(&mut self, key: &Ipv6PeerKey) -> Option<PeerState> {
        self.find(key).map(|index| self.swap_remove(index))
    }

    fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(Ipv6PeerKey, PeerState) -> bool,
    {
        let mut index = 0;
        while index < self.len() {
            let key = self.key_at(index);
            let peer = self.state_at(index);
            if keep(key, peer) {
                index += 1;
            } else {
                self.swap_remove(index);
            }
        }
    }

    fn len(&self) -> usize {
        self.bytes.len() / IPV6_ENTRY_LEN
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn shrink_if_idle(&mut self) {
        if self.bytes.is_empty() {
            self.bytes = Vec::new();
            return;
        }
        if self.bytes.capacity() > IPV6_ENTRY_LEN * 32
            && self.bytes.len() * 9 < self.bytes.capacity() * 10
        {
            self.bytes.shrink_to_fit();
        }
    }

    fn append_contacts(&self, exclude: Option<&Ipv6PeerKey>, contacts: &mut Vec<PeerContact>) {
        for index in 0..self.len() {
            let key = self.key_at(index);
            if exclude.map_or(true, |ex| &key != ex) {
                contacts.push(key.contact());
            }
        }
    }

    fn select_random(
        &self,
        count: usize,
        rng: &mut Rng,
        exclude: Option<&Ipv6PeerKey>,
        contacts: &mut Vec<PeerContact>,
    ) {
        let total = self.len();
        if total == 0 || count == 0 {
            return;
        }

        if count >= total {
            self.append_contacts(exclude, contacts);
            return;
        }

        // Fixed-point even-spacing random selection (OpenTracker style)
        let mut shifted_total = total as u64;
        let mut shift: u32 = 0;
        while shifted_total < (1u64 << 62) {
            shifted_total <<= 1;
            shift += 1;
        }
        let shifted_step = shifted_total / count as u64;

        let mut pos = rng.next_usize(total);

        for remaining in (0..count).rev() {
            let diff = (((remaining as u64 + 1) * shifted_step) >> shift)
                .saturating_sub(((remaining as u64) * shifted_step) >> shift);
            let advance = 1 + if diff > 1 {
                rng.next_usize(diff as usize)
            } else {
                0
            };
            pos = (pos + advance) % total;

            let key = self.key_at(pos);
            if exclude.map_or(true, |ex| &key != ex) {
                contacts.push(key.contact());
            }
        }
    }

    fn find(&self, key: &Ipv6PeerKey) -> Option<usize> {
        (0..self.len()).find(|&index| self.key_at(index) == *key)
    }

    fn push(&mut self, key: Ipv6PeerKey, peer: PeerState) {
        self.bytes.extend_from_slice(&key.ip);
        self.bytes.extend_from_slice(&key.port.to_be_bytes());
        self.bytes.push(flags(&peer));
        self.bytes.push(peer.client_tag);
        self.bytes
            .extend_from_slice(&peer.last_seen_secs.to_be_bytes());
    }

    fn write_at(&mut self, index: usize, key: Ipv6PeerKey, peer: PeerState) {
        let entry = &mut self.bytes[ipv6_range(index)];
        entry[0..16].copy_from_slice(&key.ip);
        entry[16..18].copy_from_slice(&key.port.to_be_bytes());
        entry[18] = flags(&peer);
        entry[19] = peer.client_tag;
        entry[20..24].copy_from_slice(&peer.last_seen_secs.to_be_bytes());
    }

    fn swap_remove(&mut self, index: usize) -> PeerState {
        let removed = self.state_at(index);
        let last = self.len() - 1;
        if index != last {
            let mut replacement = [0_u8; IPV6_ENTRY_LEN];
            replacement.copy_from_slice(&self.bytes[ipv6_range(last)]);
            self.bytes[ipv6_range(index)].copy_from_slice(&replacement);
        }
        self.bytes.truncate(last * IPV6_ENTRY_LEN);
        removed
    }

    fn key_at(&self, index: usize) -> Ipv6PeerKey {
        let entry = &self.bytes[ipv6_range(index)];
        let mut ip = [0_u8; 16];
        ip.copy_from_slice(&entry[0..16]);
        Ipv6PeerKey {
            ip,
            port: u16::from_be_bytes([entry[16], entry[17]]),
        }
    }

    fn state_at(&self, index: usize) -> PeerState {
        let entry = &self.bytes[ipv6_range(index)];
        PeerState {
            complete: entry[18] & FLAG_COMPLETE != 0,
            last_seen_secs: u32::from_be_bytes([entry[20], entry[21], entry[22], entry[23]]),
            client_tag: entry[19],
        }
    }
}

fn flags(peer: &PeerState) -> u8 {
    if peer.complete {
        FLAG_COMPLETE
    } else {
        0
    }
}

fn ipv4_range(index: usize) -> std::ops::Range<usize> {
    let start = index * IPV4_ENTRY_LEN;
    start..start + IPV4_ENTRY_LEN
}

fn ipv6_range(index: usize) -> std::ops::Range<usize> {
    let start = index * IPV6_ENTRY_LEN;
    start..start + IPV6_ENTRY_LEN
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn next_usize(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() as usize) % bound
    }
}

/// Allocate `amount` peers between IPv4 and IPv6 buckets.
/// Guarantees at least 1/4 from each family if available,
/// remaining slots filled proportionally by pool size.
fn allocate_v4_v6(total_v4: usize, total_v6: usize, amount: usize) -> (usize, usize) {
    if total_v4 + total_v6 <= amount {
        return (total_v4, total_v6);
    }

    let quarter = amount / 4;
    let mut v4 = quarter.min(total_v4);
    let mut v6 = quarter.min(total_v6);

    let amount_left = amount - v4 - v6;
    let left_v4 = total_v4 - v4;
    let left_v6 = total_v6 - v6;

    if left_v4 + left_v6 > 0 {
        let scale: usize = 1024;
        let pct_v4 = (scale * left_v4) / (left_v4 + left_v6);
        v4 += (amount_left * pct_v4) / scale;
        v6 += amount_left - (amount_left * pct_v4) / scale;
    }

    v4 = v4.min(total_v4);
    v6 = v6.min(total_v6);

    // Integer division rounding can leave out a peer
    while v4 + v6 < amount {
        if v6 < total_v6 {
            v6 += 1;
        } else if v4 < total_v4 {
            v4 += 1;
        } else {
            break;
        }
    }

    (v4, v6)
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
