use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;

use crate::core::topk::{Top100All, TopKMerger};
use crate::core::tracker::{AnnounceInput, Tracker, TrackerSnapshot};
use crate::core::types::{AnnounceOutput, InfoHash};

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
    ) -> AnnounceOutput {
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
            return Top100All::empty();
        }

        let mut merger = TopKMerger::new(limit);
        for shard in &self.shards {
            merger.insert(&shard.read().await.top_torrents_all(limit));
        }
        merger.finish()
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
            combined.totals += snapshot.totals;
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

const EXPIRE_SWEEP_INTERVAL: Duration = Duration::from_secs(1);

/// Background task: run the peer expiry sweep every second.
pub(crate) fn spawn_expiry_sweep(tracker: Arc<TrackerPool>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(EXPIRE_SWEEP_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            tracker.expire_due(Instant::now());
        }
    });
}
