//! One-pass Top-K ranking across all four dimensions (peers, seeders,
//! leechers, downloaded). Four min-heaps share a single iteration over
//! the swarm table for efficiency.

#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

use std::cmp::Reverse;
use std::collections::{BinaryHeap, BTreeMap};

use super::swarm::Swarm;
use super::types::InfoHash;

/// Aggregated Top-K results for all four ranking dimensions.
pub(crate) struct Top100All {
    pub peers: Vec<(InfoHash, usize, usize, u64)>,
    pub seeders: Vec<(InfoHash, usize, usize, u64)>,
    pub leechers: Vec<(InfoHash, usize, usize, u64)>,
    pub downloaded: Vec<(InfoHash, usize, usize, u64)>,
}

/// Compute Top-K for all four rankings in a single pass over `swarms`.
pub(crate) fn top_torrents_all(swarms: &BTreeMap<InfoHash, Swarm>, limit: usize) -> Top100All {
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

    for (info_hash, swarm) in swarms {
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
        try_heap_insert(&mut heap_p, &mut min_p, limit, peers, *info_hash, stats.complete, stats.incomplete, dl);
        try_heap_insert(&mut heap_s, &mut min_s, limit, seeders, *info_hash, stats.complete, stats.incomplete, dl);
        try_heap_insert(&mut heap_l, &mut min_l, limit, leechers, *info_hash, stats.complete, stats.incomplete, dl);
        try_heap_insert(&mut heap_d, &mut min_d, limit, downloaded, *info_hash, stats.complete, stats.incomplete, dl);
    }

    Top100All {
        peers: drain_heap_by(heap_p, 0),
        seeders: drain_heap_by(heap_s, 1),
        leechers: drain_heap_by(heap_l, 2),
        downloaded: drain_heap_by(heap_d, 3),
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
        1 => result.sort_by(|a, b| b.1.cmp(&a.1)),
        2 => result.sort_by(|a, b| b.2.cmp(&a.2)),
        3 => result.sort_by(|a, b| b.3.cmp(&a.3)),
        _ => result.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2))),
    }
    result
}
