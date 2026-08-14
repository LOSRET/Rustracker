//! One-pass Top-K ranking across all four dimensions (peers, seeders,
//! leechers, downloaded). Four min-heaps share a single iteration over
//! the swarm table for efficiency.

#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use super::swarm::Swarm;
use super::types::InfoHash;

/// Aggregated Top-K results for all four ranking dimensions.
pub(crate) struct Top100All {
    pub peers: Vec<(InfoHash, usize, usize, u64)>,
    pub seeders: Vec<(InfoHash, usize, usize, u64)>,
    pub leechers: Vec<(InfoHash, usize, usize, u64)>,
    pub downloaded: Vec<(InfoHash, usize, usize, u64)>,
}

impl Top100All {
    pub(crate) fn empty() -> Self {
        Top100All {
            peers: Vec::new(),
            seeders: Vec::new(),
            leechers: Vec::new(),
            downloaded: Vec::new(),
        }
    }
}

/// Merges per-shard [`Top100All`] results into a single top-K, keeping
/// the heap bookkeeping contained in this module.
pub(crate) struct TopKMerger {
    heaps: [BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>>; 4],
    mins: [u64; 4],
    limit: usize,
}

impl TopKMerger {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            heaps: std::array::from_fn(|_| BinaryHeap::with_capacity(limit)),
            mins: [0; 4],
            limit,
        }
    }

    pub(crate) fn insert(&mut self, part: &Top100All) {
        let dims = [&part.peers, &part.seeders, &part.leechers, &part.downloaded];
        for (dim, entries) in dims.into_iter().enumerate() {
            for &(info_hash, seeders, leechers, downloaded) in entries {
                try_heap_insert(
                    &mut self.heaps[dim],
                    &mut self.mins[dim],
                    self.limit,
                    dim_key(dim, seeders, leechers, downloaded),
                    info_hash,
                    seeders,
                    leechers,
                    downloaded,
                );
            }
        }
    }

    pub(crate) fn finish(self) -> Top100All {
        let [hp, hs, hl, hd] = self.heaps;
        Top100All {
            peers: drain_heap_by(hp, 0),
            seeders: drain_heap_by(hs, 1),
            leechers: drain_heap_by(hl, 2),
            downloaded: drain_heap_by(hd, 3),
        }
    }
}

/// Rank key for each dimension of a torrent entry.
fn dim_key(dim: usize, seeders: usize, leechers: usize, downloaded: u64) -> u64 {
    match dim {
        0 => (seeders + leechers) as u64,
        1 => seeders as u64,
        2 => leechers as u64,
        _ => downloaded,
    }
}

/// Compute Top-K for all four rankings in a single pass over `swarms`.
pub(crate) fn top_torrents_all(swarms: &BTreeMap<InfoHash, Swarm>, limit: usize) -> Top100All {
    if limit == 0 {
        return Top100All::empty();
    }

    let mut heaps: [BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>>; 4] =
        std::array::from_fn(|_| BinaryHeap::with_capacity(limit));
    let mut mins = [0u64; 4];

    for (info_hash, swarm) in swarms {
        let stats = swarm.stats();
        let keys = [
            (stats.complete + stats.incomplete) as u64,
            stats.complete as u64,
            stats.incomplete as u64,
            stats.downloaded as u64,
        ];

        // Fast path: all four heaps are full and this torrent is
        // below every threshold — skip without constructing entries.
        if heaps[0].len() >= limit
            && keys[0] <= mins[0]
            && heaps[1].len() >= limit
            && keys[1] <= mins[1]
            && heaps[2].len() >= limit
            && keys[2] <= mins[2]
            && heaps[3].len() >= limit
            && keys[3] <= mins[3]
        {
            continue;
        }

        for dim in 0..4 {
            try_heap_insert(
                &mut heaps[dim],
                &mut mins[dim],
                limit,
                keys[dim],
                *info_hash,
                stats.complete,
                stats.incomplete,
                stats.downloaded as u64,
            );
        }
    }

    let [hp, hs, hl, hd] = heaps;
    Top100All {
        peers: drain_heap_by(hp, 0),
        seeders: drain_heap_by(hs, 1),
        leechers: drain_heap_by(hl, 2),
        downloaded: drain_heap_by(hd, 3),
    }
}

pub(crate) fn try_heap_insert(
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
                *min_key = peek.0 .0;
            }
        }
    } else {
        heap.pop();
        heap.push(entry);
        if let Some(peek) = heap.peek() {
            *min_key = peek.0 .0;
        }
    }
}

pub(crate) fn drain_heap_by(
    heap: BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>>,
    sort_field: u8, // 0 = peers (1+2), 1 = seeders, 2 = leechers, 3 = downloaded
) -> Vec<(InfoHash, usize, usize, u64)> {
    let mut result: Vec<_> = heap
        .into_iter()
        .map(|Reverse((_, info_hash, seeders, leechers, downloaded))| {
            (info_hash, seeders, leechers, downloaded)
        })
        .collect();
    match sort_field {
        1 => result.sort_by_key(|r| Reverse(r.1)),
        2 => result.sort_by_key(|r| Reverse(r.2)),
        3 => result.sort_by_key(|r| Reverse(r.3)),
        _ => result.sort_by_key(|r| Reverse(r.1 + r.2)),
    }
    result
}
