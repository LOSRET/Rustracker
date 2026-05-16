//! Packed peer storage, random selection, and swarm helpers.
//!
//! IPv4 peers are stored in 12-byte entries; IPv6 in 24-byte entries.
//! Selection uses fixed-point even-spacing (OpenTracker style).

use crate::types::{Ipv4PeerKey, Ipv6PeerKey, PeerContact, PeerState};

pub(crate) const FLAG_COMPLETE: u8 = 1;
pub(crate) const IPV4_ENTRY_LEN: usize = 12;
pub(crate) const IPV6_ENTRY_LEN: usize = 24;

// ── Packed IPv4 peers ────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub(crate) struct PackedIpv4Peers {
    pub(crate) bytes: Vec<u8>,
}

impl PackedIpv4Peers {
    pub(crate) fn insert(&mut self, key: Ipv4PeerKey, peer: PeerState) -> Option<PeerState> {
        if let Some(index) = self.find(&key) {
            let old = self.state_at(index);
            self.write_at(index, key, peer);
            Some(old)
        } else {
            self.push(key, peer);
            None
        }
    }

    pub(crate) fn remove(&mut self, key: &Ipv4PeerKey) -> Option<PeerState> {
        self.find(key).map(|index| self.swap_remove(index))
    }

    pub(crate) fn retain<F>(&mut self, mut keep: F)
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
        if self.bytes.capacity() > IPV4_ENTRY_LEN * 32
            && self.bytes.len() * 9 < self.bytes.capacity() * 10
        {
            self.bytes.shrink_to_fit();
        }
    }

    pub(crate) fn append_contacts(
        &self,
        exclude: Option<&Ipv4PeerKey>,
        contacts: &mut Vec<PeerContact>,
    ) {
        for index in 0..self.len() {
            let key = self.key_at(index);
            if exclude.map_or(true, |ex| &key != ex) {
                contacts.push(key.contact());
            }
        }
    }

    pub(crate) fn select_random(
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
            let advance = 1
                + if diff > 1 {
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

// ── Packed IPv6 peers ────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub(crate) struct PackedIpv6Peers {
    pub(crate) bytes: Vec<u8>,
}

impl PackedIpv6Peers {
    pub(crate) fn insert(&mut self, key: Ipv6PeerKey, peer: PeerState) -> Option<PeerState> {
        if let Some(index) = self.find(&key) {
            let old = self.state_at(index);
            self.write_at(index, key, peer);
            Some(old)
        } else {
            self.push(key, peer);
            None
        }
    }

    pub(crate) fn remove(&mut self, key: &Ipv6PeerKey) -> Option<PeerState> {
        self.find(key).map(|index| self.swap_remove(index))
    }

    pub(crate) fn retain<F>(&mut self, mut keep: F)
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
        if self.bytes.capacity() > IPV6_ENTRY_LEN * 32
            && self.bytes.len() * 9 < self.bytes.capacity() * 10
        {
            self.bytes.shrink_to_fit();
        }
    }

    pub(crate) fn append_contacts(
        &self,
        exclude: Option<&Ipv6PeerKey>,
        contacts: &mut Vec<PeerContact>,
    ) {
        for index in 0..self.len() {
            let key = self.key_at(index);
            if exclude.map_or(true, |ex| &key != ex) {
                contacts.push(key.contact());
            }
        }
    }

    pub(crate) fn select_random(
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
            let advance = 1
                + if diff > 1 {
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
