use std::collections::hash_map::DefaultHasher;
use std::collections::{BinaryHeap, HashMap};
use std::cmp::Reverse;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::core::topk::{self, Top100All};
use crate::core::tracker::{AnnounceInput, Tracker, TrackerSnapshot};
use crate::core::types::InfoHash;

pub const DEFAULT_TRACKER_SHARDS: usize = 64;

pub(crate) struct TrackerPool {
    shards: Vec<RwLock<Tracker>>,
}

impl TrackerPool {
    pub(crate) fn single(tracker: Tracker) -> Self {
        Self {
            shards: vec![RwLock::new(tracker)],
        }
    }

    pub(crate) fn new(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
        let shard_count = shards.max(1);
        let shards = (0..shard_count)
            .map(|_| RwLock::new(Tracker::new(interval, peer_timeout)))
            .collect();

        Self { shards }
    }

    pub(crate) async fn announce(
        &self,
        info_hash: InfoHash,
        input: AnnounceInput,
        now: Instant,
    ) -> crate::core::tracker::AnnounceOutput {
        self.shard(info_hash).write().await.announce(input, now)
    }

    pub(crate) async fn scrape(
        &self,
        info_hashes: &[InfoHash],
    ) -> HashMap<InfoHash, crate::core::types::TorrentStats> {
        let mut stats = HashMap::with_capacity(info_hashes.len());
        let mut by_shard = HashMap::<usize, Vec<InfoHash>>::new();

        for &info_hash in info_hashes {
            by_shard
                .entry(self.shard_index(info_hash))
                .or_default()
                .push(info_hash);
        }

        for (shard_index, shard_info_hashes) in by_shard {
            let shard_stats = self.shards[shard_index]
                .read()
                .await
                .scrape(&shard_info_hashes);
            stats.extend(shard_stats);
        }

        stats
    }

    pub(crate) async fn top_torrents_all(&self, limit: usize) -> Top100All {
        if limit == 0 {
            return Top100All {
                peers: Vec::new(),
                seeders: Vec::new(),
                leechers: Vec::new(),
                downloaded: Vec::new(),
            };
        }

        let mut heaps: [BinaryHeap<Reverse<(u64, InfoHash, usize, usize, u64)>>; 4] =
            std::array::from_fn(|_| BinaryHeap::with_capacity(limit));
        let mut mins: [u64; 4] = [0; 4];

        for shard in &self.shards {
            let all = shard.read().await.top_torrents_all(limit);
            for (info_hash, seeders, leechers, downloaded) in all.peers {
                topk::try_heap_insert(&mut heaps[0], &mut mins[0], limit, (seeders + leechers) as u64, info_hash, seeders, leechers, downloaded);
            }
            for (info_hash, seeders, leechers, downloaded) in all.seeders {
                topk::try_heap_insert(&mut heaps[1], &mut mins[1], limit, seeders as u64, info_hash, seeders, leechers, downloaded);
            }
            for (info_hash, seeders, leechers, downloaded) in all.leechers {
                topk::try_heap_insert(&mut heaps[2], &mut mins[2], limit, leechers as u64, info_hash, seeders, leechers, downloaded);
            }
            for (info_hash, seeders, leechers, downloaded) in all.downloaded {
                topk::try_heap_insert(&mut heaps[3], &mut mins[3], limit, downloaded, info_hash, seeders, leechers, downloaded);
            }
        }

        let [hp, hs, hl, hd] = heaps;
        Top100All {
            peers: topk::drain_heap_by(hp, 0),
            seeders: topk::drain_heap_by(hs, 1),
            leechers: topk::drain_heap_by(hl, 2),
            downloaded: topk::drain_heap_by(hd, 3),
        }
    }

    pub(crate) async fn snapshot(&self) -> TrackerSnapshot {
        let mut snapshots = Vec::with_capacity(self.shards.len());

        for shard in &self.shards {
            snapshots.push(shard.read().await.snapshot());
        }

        let mut combined = snapshots.first().cloned().unwrap_or(TrackerSnapshot {
            interval: 0,
            peer_timeout: 0,
            totals: Default::default(),
            clients: Vec::new(),
        });

        combined.totals = Default::default();
        let mut client_map: HashMap<u8, u64> = HashMap::new();
        for snapshot in snapshots {
            combined.totals.torrents += snapshot.totals.torrents;
            combined.totals.peers += snapshot.totals.peers;
            combined.totals.seeders += snapshot.totals.seeders;
            combined.totals.leechers += snapshot.totals.leechers;
            combined.totals.downloaded = combined
                .totals
                .downloaded
                .saturating_add(snapshot.totals.downloaded);
            for (tag, count) in snapshot.clients {
                *client_map.entry(tag).or_insert(0) += count;
            }
        }
        combined.clients = client_map.into_iter().collect();

        combined
    }

    fn shard(&self, info_hash: InfoHash) -> &RwLock<Tracker> {
        &self.shards[self.shard_index(info_hash)]
    }

    fn shard_index(&self, info_hash: InfoHash) -> usize {
        let mut hasher = DefaultHasher::new();
        info_hash.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }

    pub(crate) fn expire_due(&self, now: Instant) {
        for shard in &self.shards {
            if let Ok(mut tracker) = shard.try_write() {
                tracker.expire_due(now);
            }
        }
    }
}
