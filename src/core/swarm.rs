//! Packed peer storage, random selection, and swarm helpers.
//!
//! IPv4 peers are stored in 12-byte entries; IPv6 in 24-byte entries.
//! Selection uses fixed-point even-spacing (OpenTracker style).

use std::net::IpAddr;

use super::counters::{ExpireResult, PeerRemoval, PeerUpsert};
use super::types::{Ipv4PeerKey, Ipv6PeerKey, PeerState, TorrentStats};

pub(crate) const FLAG_COMPLETE: u8 = 1;
pub(crate) const IPV4_ENTRY_LEN: usize = 12;
pub(crate) const IPV6_ENTRY_LEN: usize = 24;
const PROMOTE_THRESHOLD: usize = 16;
const DEMOTE_THRESHOLD: usize = 8;

// ── Packed IPv4 peers ────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub(crate) struct PackedIpv4Peers {
    pub(crate) bytes: Vec<u8>,
    sorted: bool,
}

impl PackedIpv4Peers {
    pub(crate) fn insert(&mut self, key: Ipv4PeerKey, peer: PeerState) -> Option<PeerState> {
        if self.sorted {
            match self.search_index(&key) {
                Ok(index) => {
                    let old = self.state_at(index);
                    self.write_at(index, key, peer);
                    Some(old)
                }
                Err(index) => {
                    self.insert_at(index, key, peer);
                    None
                }
            }
        } else {
            let old = if let Some(index) = self.find_linear(&key) {
                let old = self.state_at(index);
                self.write_at(index, key, peer);
                Some(old)
            } else {
                self.push(key, peer);
                None
            };
            if self.len() >= PROMOTE_THRESHOLD {
                self.sort_and_promote();
            }
            old
        }
    }

    pub(crate) fn remove(&mut self, key: &Ipv4PeerKey) -> Option<PeerState> {
        if self.sorted {
            let result = self.find(key).map(|index| self.remove_at(index));
            if self.len() < DEMOTE_THRESHOLD {
                self.sorted = false;
            }
            result
        } else {
            self.find_linear(key).map(|index| self.swap_remove(index))
        }
    }

    pub(crate) fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(Ipv4PeerKey, PeerState) -> bool,
    {
        if self.sorted {
            let mut write = 0usize;
            for read in 0..self.len() {
                let key = self.key_at(read);
                let peer = self.state_at(read);
                if keep(key, peer) {
                    if write != read {
                        let src = read * IPV4_ENTRY_LEN;
                        let dst = write * IPV4_ENTRY_LEN;
                        self.bytes.copy_within(src..src + IPV4_ENTRY_LEN, dst);
                    }
                    write += 1;
                }
            }
            self.bytes.truncate(write * IPV4_ENTRY_LEN);
            if self.len() < DEMOTE_THRESHOLD {
                self.sorted = false;
            }
        } else {
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
    }

    pub(crate) fn len(&self) -> usize {
        self.bytes.len() / IPV4_ENTRY_LEN
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub(crate) fn shrink_if_idle(&mut self) {
        if self.bytes.is_empty() {
            self.bytes = Vec::new();
            return;
        }
        let cap = self.bytes.capacity();
        if cap > IPV4_ENTRY_LEN * 4 {
            let entries = self.bytes.len() / IPV4_ENTRY_LEN;
            let target = entries.next_power_of_two().max(4) * IPV4_ENTRY_LEN;
            if target < cap {
                self.bytes.shrink_to(target);
            }
        }
    }

    pub(crate) fn append_compact(&self, exclude: Option<&Ipv4PeerKey>, out: &mut Vec<u8>) {
        for index in 0..self.len() {
            let key = self.key_at(index);
            if exclude != Some(&key) {
                out.extend_from_slice(&key.ip);
                out.extend_from_slice(&key.port.to_be_bytes());
            }
        }
    }

    pub(crate) fn select_random_compact(
        &self,
        count: usize,
        rng: &mut Rng,
        exclude: Option<&Ipv4PeerKey>,
        out: &mut Vec<u8>,
    ) {
        let total = self.len();
        if total == 0 || count == 0 {
            return;
        }

        if count >= total {
            self.append_compact(exclude, out);
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
            if exclude != Some(&key) {
                out.extend_from_slice(&key.ip);
                out.extend_from_slice(&key.port.to_be_bytes());
            }
        }
    }

    fn find(&self, key: &Ipv4PeerKey) -> Option<usize> {
        if self.sorted {
            self.search_index(key).ok()
        } else {
            self.find_linear(key)
        }
    }

    fn find_linear(&self, key: &Ipv4PeerKey) -> Option<usize> {
        (0..self.len()).find(|&index| self.key_at(index) == *key)
    }

    fn search_index(&self, key: &Ipv4PeerKey) -> Result<usize, usize> {
        let mut size = self.len();
        let mut base = 0usize;
        while size > 0 {
            let half = size / 2;
            let mid = base + half;
            match self.key_at(mid).cmp(key) {
                std::cmp::Ordering::Less => {
                    base = mid + 1;
                    size -= half + 1;
                }
                std::cmp::Ordering::Greater => {
                    size = half;
                }
                std::cmp::Ordering::Equal => return Ok(mid),
            }
        }
        Err(base)
    }

    fn insert_at(&mut self, index: usize, key: Ipv4PeerKey, peer: PeerState) {
        let offset = index * IPV4_ENTRY_LEN;
        let old_len = self.bytes.len();
        self.bytes.resize(old_len + IPV4_ENTRY_LEN, 0);
        self.bytes
            .copy_within(offset..old_len, offset + IPV4_ENTRY_LEN);
        self.write_at(index, key, peer);
    }

    fn remove_at(&mut self, index: usize) -> PeerState {
        let removed = self.state_at(index);
        let offset = index * IPV4_ENTRY_LEN;
        let next = offset + IPV4_ENTRY_LEN;
        self.bytes.copy_within(next.., offset);
        self.bytes.truncate(self.bytes.len() - IPV4_ENTRY_LEN);
        removed
    }

    fn write_at(&mut self, index: usize, key: Ipv4PeerKey, peer: PeerState) {
        let entry = &mut self.bytes[ipv4_range(index)];
        entry[0..4].copy_from_slice(&key.ip);
        entry[4..6].copy_from_slice(&key.port.to_be_bytes());
        entry[6] = flags(&peer);
        entry[7] = peer.client_tag;
        entry[8..12].copy_from_slice(&peer.last_seen_secs.to_be_bytes());
    }

    fn push(&mut self, key: Ipv4PeerKey, peer: PeerState) {
        self.bytes.extend_from_slice(&key.ip);
        self.bytes.extend_from_slice(&key.port.to_be_bytes());
        self.bytes.push(flags(&peer));
        self.bytes.push(peer.client_tag);
        self.bytes
            .extend_from_slice(&peer.last_seen_secs.to_be_bytes());
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

    fn sort_and_promote(&mut self) {
        let n = self.len();
        for i in 1..n {
            let key = self.key_at(i);
            let peer = self.state_at(i);
            let mut j = i;
            while j > 0 && self.key_at(j - 1) > key {
                self.bytes.copy_within(
                    (j - 1) * IPV4_ENTRY_LEN..j * IPV4_ENTRY_LEN,
                    j * IPV4_ENTRY_LEN,
                );
                j -= 1;
            }
            if j != i {
                self.write_at(j, key, peer);
            }
        }
        self.sorted = true;
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

// ── Packed IPv6 peers ────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub(crate) struct PackedIpv6Peers {
    pub(crate) bytes: Vec<u8>,
    sorted: bool,
}

impl PackedIpv6Peers {
    pub(crate) fn insert(&mut self, key: Ipv6PeerKey, peer: PeerState) -> Option<PeerState> {
        if self.sorted {
            match self.search_index(&key) {
                Ok(index) => {
                    let old = self.state_at(index);
                    self.write_at(index, key, peer);
                    Some(old)
                }
                Err(index) => {
                    self.insert_at(index, key, peer);
                    None
                }
            }
        } else {
            let old = if let Some(index) = self.find_linear(&key) {
                let old = self.state_at(index);
                self.write_at(index, key, peer);
                Some(old)
            } else {
                self.push(key, peer);
                None
            };
            if self.len() >= PROMOTE_THRESHOLD {
                self.sort_and_promote();
            }
            old
        }
    }

    pub(crate) fn remove(&mut self, key: &Ipv6PeerKey) -> Option<PeerState> {
        if self.sorted {
            let result = self.find(key).map(|index| self.remove_at(index));
            if self.len() < DEMOTE_THRESHOLD {
                self.sorted = false;
            }
            result
        } else {
            self.find_linear(key).map(|index| self.swap_remove(index))
        }
    }

    pub(crate) fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(Ipv6PeerKey, PeerState) -> bool,
    {
        if self.sorted {
            let mut write = 0usize;
            for read in 0..self.len() {
                let key = self.key_at(read);
                let peer = self.state_at(read);
                if keep(key, peer) {
                    if write != read {
                        let src = read * IPV6_ENTRY_LEN;
                        let dst = write * IPV6_ENTRY_LEN;
                        self.bytes.copy_within(src..src + IPV6_ENTRY_LEN, dst);
                    }
                    write += 1;
                }
            }
            self.bytes.truncate(write * IPV6_ENTRY_LEN);
            if self.len() < DEMOTE_THRESHOLD {
                self.sorted = false;
            }
        } else {
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
    }

    pub(crate) fn len(&self) -> usize {
        self.bytes.len() / IPV6_ENTRY_LEN
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub(crate) fn shrink_if_idle(&mut self) {
        if self.bytes.is_empty() {
            self.bytes = Vec::new();
            return;
        }
        let cap = self.bytes.capacity();
        if cap > IPV6_ENTRY_LEN * 2 {
            let entries = self.bytes.len() / IPV6_ENTRY_LEN;
            let target = entries.next_power_of_two().max(2) * IPV6_ENTRY_LEN;
            if target < cap {
                self.bytes.shrink_to(target);
            }
        }
    }

    pub(crate) fn append_compact(&self, exclude: Option<&Ipv6PeerKey>, out: &mut Vec<u8>) {
        for index in 0..self.len() {
            let key = self.key_at(index);
            if exclude != Some(&key) {
                out.extend_from_slice(&key.ip);
                out.extend_from_slice(&key.port.to_be_bytes());
            }
        }
    }

    pub(crate) fn select_random_compact(
        &self,
        count: usize,
        rng: &mut Rng,
        exclude: Option<&Ipv6PeerKey>,
        out: &mut Vec<u8>,
    ) {
        let total = self.len();
        if total == 0 || count == 0 {
            return;
        }

        if count >= total {
            self.append_compact(exclude, out);
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
            if exclude != Some(&key) {
                out.extend_from_slice(&key.ip);
                out.extend_from_slice(&key.port.to_be_bytes());
            }
        }
    }

    fn find(&self, key: &Ipv6PeerKey) -> Option<usize> {
        if self.sorted {
            self.search_index(key).ok()
        } else {
            self.find_linear(key)
        }
    }

    fn find_linear(&self, key: &Ipv6PeerKey) -> Option<usize> {
        (0..self.len()).find(|&index| self.key_at(index) == *key)
    }

    fn search_index(&self, key: &Ipv6PeerKey) -> Result<usize, usize> {
        let mut size = self.len();
        let mut base = 0usize;
        while size > 0 {
            let half = size / 2;
            let mid = base + half;
            match self.key_at(mid).cmp(key) {
                std::cmp::Ordering::Less => {
                    base = mid + 1;
                    size -= half + 1;
                }
                std::cmp::Ordering::Greater => {
                    size = half;
                }
                std::cmp::Ordering::Equal => return Ok(mid),
            }
        }
        Err(base)
    }

    fn insert_at(&mut self, index: usize, key: Ipv6PeerKey, peer: PeerState) {
        let offset = index * IPV6_ENTRY_LEN;
        let old_len = self.bytes.len();
        self.bytes.resize(old_len + IPV6_ENTRY_LEN, 0);
        self.bytes
            .copy_within(offset..old_len, offset + IPV6_ENTRY_LEN);
        self.write_at(index, key, peer);
    }

    fn remove_at(&mut self, index: usize) -> PeerState {
        let removed = self.state_at(index);
        let offset = index * IPV6_ENTRY_LEN;
        let next = offset + IPV6_ENTRY_LEN;
        self.bytes.copy_within(next.., offset);
        self.bytes.truncate(self.bytes.len() - IPV6_ENTRY_LEN);
        removed
    }

    fn write_at(&mut self, index: usize, key: Ipv6PeerKey, peer: PeerState) {
        let entry = &mut self.bytes[ipv6_range(index)];
        entry[0..16].copy_from_slice(&key.ip);
        entry[16..18].copy_from_slice(&key.port.to_be_bytes());
        entry[18] = flags(&peer);
        entry[19] = peer.client_tag;
        entry[20..24].copy_from_slice(&peer.last_seen_secs.to_be_bytes());
    }

    fn push(&mut self, key: Ipv6PeerKey, peer: PeerState) {
        self.bytes.extend_from_slice(&key.ip);
        self.bytes.extend_from_slice(&key.port.to_be_bytes());
        self.bytes.push(flags(&peer));
        self.bytes.push(peer.client_tag);
        self.bytes
            .extend_from_slice(&peer.last_seen_secs.to_be_bytes());
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

    fn sort_and_promote(&mut self) {
        let n = self.len();
        for i in 1..n {
            let key = self.key_at(i);
            let peer = self.state_at(i);
            let mut j = i;
            while j > 0 && self.key_at(j - 1) > key {
                self.bytes.copy_within(
                    (j - 1) * IPV6_ENTRY_LEN..j * IPV6_ENTRY_LEN,
                    j * IPV6_ENTRY_LEN,
                );
                j -= 1;
            }
            if j != i {
                self.write_at(j, key, peer);
            }
        }
        self.sorted = true;
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

// ── Helpers ──────────────────────────────────────────────────────────────────

pub(crate) fn flags(peer: &PeerState) -> u8 {
    if peer.complete {
        FLAG_COMPLETE
    } else {
        0
    }
}

pub(crate) fn ipv4_range(index: usize) -> std::ops::Range<usize> {
    let start = index * IPV4_ENTRY_LEN;
    start..start + IPV4_ENTRY_LEN
}

pub(crate) fn ipv6_range(index: usize) -> std::ops::Range<usize> {
    let start = index * IPV6_ENTRY_LEN;
    start..start + IPV6_ENTRY_LEN
}

// ── XorShift RNG ─────────────────────────────────────────────────────────────

pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn next_usize(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() as usize) % bound
    }
}

// ── IPv4/IPv6 allocation ─────────────────────────────────────────────────────

/// Allocate `amount` peers between IPv4 and IPv6 buckets.
/// Guarantees at least 1/4 from each family if available,
/// remaining slots filled proportionally by pool size.
pub(crate) fn allocate_v4_v6(total_v4: usize, total_v6: usize, amount: usize) -> (usize, usize) {
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

// ── Peer endpoint ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PeerEndpoint {
    V4(Ipv4PeerKey),
    V6(Ipv6PeerKey),
}

impl PeerEndpoint {
    pub(crate) fn new(ip: IpAddr, port: u16) -> Self {
        match ip {
            IpAddr::V4(ip) => Self::V4(Ipv4PeerKey::new(ip, port)),
            IpAddr::V6(ip) => Self::V6(Ipv6PeerKey::new(ip, port)),
        }
    }
}

// ── Swarm ────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub(crate) struct Swarm {
    pub(crate) ipv4_peers: PackedIpv4Peers,
    pub(crate) ipv6_peers: PackedIpv6Peers,
    pub(crate) complete: u32,
    pub(crate) downloaded: u32,
}

impl Swarm {
    pub(crate) fn upsert_peer(&mut self, endpoint: PeerEndpoint, peer: PeerState) -> PeerUpsert {
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

    pub(crate) fn remove_peer_tag(&mut self, endpoint: PeerEndpoint) -> Option<PeerRemoval> {
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

    pub(crate) fn expire(&mut self, now_secs: u32, timeout_secs: u32) -> ExpireResult {
        let mut expired_complete: usize = 0;
        let mut expired_count: usize = 0;
        let mut expired_tags: Vec<(u8, bool)> = Vec::new();

        self.ipv4_peers.retain(|_, peer| {
            let keep = now_secs.saturating_sub(peer.last_seen_secs) <= timeout_secs;
            if !keep {
                expired_count += 1;
                let is_seeder = peer.is_complete();
                if is_seeder {
                    expired_complete += 1;
                }
                expired_tags.push((peer.client_tag, is_seeder));
            }
            keep
        });

        self.ipv6_peers.retain(|_, peer| {
            let keep = now_secs.saturating_sub(peer.last_seen_secs) <= timeout_secs;
            if !keep {
                expired_count += 1;
                let is_seeder = peer.is_complete();
                if is_seeder {
                    expired_complete += 1;
                }
                expired_tags.push((peer.client_tag, is_seeder));
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

    pub(crate) fn stats(&self) -> TorrentStats {
        let complete = self.complete as usize;
        TorrentStats {
            complete,
            downloaded: self.downloaded,
            incomplete: self.len().saturating_sub(complete),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.ipv4_peers.len() + self.ipv6_peers.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.ipv4_peers.is_empty() && self.ipv6_peers.is_empty()
    }

    pub(crate) fn contacts_excluding(
        &self,
        requesting_endpoint: PeerEndpoint,
        limit: usize,
        rng_seed: u64,
    ) -> (Vec<u8>, Vec<u8>) {
        if limit == 0 {
            return (Vec::new(), Vec::new());
        }

        let total = self.len();
        if total == 0 {
            return (Vec::new(), Vec::new());
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
            let mut v4_bytes = Vec::with_capacity(self.ipv4_peers.len() * 6);
            let mut v6_bytes = Vec::with_capacity(self.ipv6_peers.len() * 18);
            self.ipv4_peers
                .append_compact(v4_exclude.as_ref(), &mut v4_bytes);
            self.ipv6_peers
                .append_compact(v6_exclude.as_ref(), &mut v6_bytes);
            return (v4_bytes, v6_bytes);
        }

        // Allocate between v4 and v6 proportionally (at least 1/4 each if available)
        // Request one extra per pool that contains the excluded peer, so the
        // final result still has `limit` entries after exclusion.
        let extra = v4_exclude.is_some() as usize + v6_exclude.is_some() as usize;
        let (v4_amount, v6_amount) =
            allocate_v4_v6(self.ipv4_peers.len(), self.ipv6_peers.len(), limit + extra);

        let mut rng = Rng::new(rng_seed);
        let mut v4_bytes = Vec::with_capacity(v4_amount * 6);
        let mut v6_bytes = Vec::with_capacity(v6_amount * 18);

        self.ipv4_peers.select_random_compact(
            v4_amount,
            &mut rng,
            v4_exclude.as_ref(),
            &mut v4_bytes,
        );
        self.ipv6_peers.select_random_compact(
            v6_amount,
            &mut rng,
            v6_exclude.as_ref(),
            &mut v6_bytes,
        );

        // Truncate to limit — extra allocation may produce more than limit
        // when the excluded peer is not selected.
        let v4_count = v4_bytes.len() / 6;
        let v6_count = v6_bytes.len() / 18;
        let excess = (v4_count + v6_count).saturating_sub(limit);
        if excess > 0 {
            let v6_remove = excess.min(v6_count);
            v6_bytes.truncate((v6_count - v6_remove) * 18);
            let v4_remove = excess - v6_remove;
            if v4_remove > 0 {
                v4_bytes.truncate((v4_count - v4_remove) * 6);
            }
        }

        (v4_bytes, v6_bytes)
    }
}
