#![allow(clippy::type_complexity)]

use std::collections::hash_map::DefaultHasher;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Reverse;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use axum::routing::get;
use axum::Router;
use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;

use crate::core::topk::{self, Top100All};
use crate::core::tracker::{AnnounceInput, Tracker, TrackerSnapshot};
use crate::core::types::InfoHash;

mod blacklist;
pub(crate) mod handlers;
mod trends;

use trends::TrendStore;

fn load_trends(trends_file: &Option<PathBuf>) -> TrendStore {
    let top_clients_file = trends_file
        .as_ref()
        .map(|p| p.parent().unwrap_or(Path::new(".")).join("top_clients.jsonl"));
    match trends_file
        .as_ref()
        .map(|p| trends::load_trends_from_file(p, top_clients_file.as_ref()))
        .transpose()
    {
        Ok(Some(store)) => store,
        Ok(None) => TrendStore::default(),
        Err(err) => {
            tracing::warn!("failed to load trend data: {err}");
            TrendStore::default()
        }
    }
}

pub const DEFAULT_TRACKER_SHARDS: usize = 64;
const EXPIRE_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(1);
const TREND_SAMPLE_INTERVAL: Duration = Duration::from_secs(10 * 60);
const BLACKLIST_WATCH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct AppState {
    pub(crate) tracker: Arc<TrackerPool>,
    pub(crate) trends: Arc<RwLock<TrendStore>>,
    pub(crate) blacklist: Arc<RwLock<Arc<HashSet<InfoHash>>>>,
    pub(crate) blacklist_path: Option<PathBuf>,
    pub(crate) admin_token: Option<String>,
    pub(crate) started_at: Instant,
    pub(crate) rps_counter: Arc<AtomicU64>,
    pub(crate) current_rps: Arc<AtomicU64>,
    #[cfg(feature = "dashboard")]
    pub(crate) versioned_index: String,
}

pub(crate) struct TrackerPool {
    shards: Vec<RwLock<Tracker>>,
}

impl AppState {
    pub fn new(tracker: Tracker, trends_file: Option<PathBuf>) -> Self {
        Self {
            tracker: Arc::new(TrackerPool::single(tracker)),
            trends: Arc::new(RwLock::new(load_trends(&trends_file))),
            blacklist: Arc::new(RwLock::new(Arc::new(HashSet::new()))),
            blacklist_path: None,
            admin_token: None,
            started_at: Instant::now(),
            rps_counter: Arc::new(AtomicU64::new(0)),
            current_rps: Arc::new(AtomicU64::new(0)),
            #[cfg(feature = "dashboard")]
            versioned_index: handlers::make_versioned_index(),
        }
    }

    pub fn sharded(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
        Self::sharded_with_blacklist_file(interval, peer_timeout, shards, None, None, None)
    }

    pub fn sharded_with_blacklist_file(
        interval: Duration,
        peer_timeout: Duration,
        shards: usize,
        blacklist_path: Option<PathBuf>,
        trends_file: Option<PathBuf>,
        admin_token: Option<String>,
    ) -> Self {
        let initial = blacklist_path
            .as_deref()
            .and_then(|path| match blacklist::load_blacklist(path) {
                Ok(set) => Some(set),
                Err(err) => {
                    tracing::warn!("{err}");
                    None
                }
            })
            .unwrap_or_default();

        let state = Self {
            tracker: Arc::new(TrackerPool::new(interval, peer_timeout, shards)),
            trends: Arc::new(RwLock::new(load_trends(&trends_file))),
            blacklist: Arc::new(RwLock::new(Arc::new(initial))),
            blacklist_path: blacklist_path.clone(),
            admin_token,
            started_at: Instant::now(),
            rps_counter: Arc::new(AtomicU64::new(0)),
            current_rps: Arc::new(AtomicU64::new(0)),
            #[cfg(feature = "dashboard")]
            versioned_index: handlers::make_versioned_index(),
        };

        state.spawn_maintenance(trends_file);
        state.spawn_rps_sampler();
        if let Some(path) = blacklist_path {
            state.spawn_blacklist_watcher(path);
        }
        state
    }

    fn spawn_maintenance(&self, trends_file: Option<PathBuf>) {
        let tracker = self.tracker.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(EXPIRE_MAINTENANCE_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                interval.tick().await;
                tracker.expire_due(Instant::now());
            }
        });

        let tracker = self.tracker.clone();
        let trends = self.trends.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(TREND_SAMPLE_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            interval.tick().await;

            loop {
                interval.tick().await;
                let snapshot = tracker.snapshot().await;
                let now = trends::unix_timestamp();
                let mut store = trends.write().await;
                store.record(now, &snapshot);
                if let Some(ref path) = trends_file {
                    let _ = trends::save_trend_point(path, now, &snapshot);
                }
                store.record_clients(now, &snapshot.clients);
                if let Some(ref path) = trends_file {
                    let _ = trends::save_client_point(
                        &path.parent().unwrap_or(Path::new(".")).join("top_clients.jsonl"),
                        now,
                        &snapshot.clients,
                    );
                }
            }
        });
    }

    fn spawn_rps_sampler(&self) {
        let rps_counter = self.rps_counter.clone();
        let current_rps = self.current_rps.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                let count = rps_counter.swap(0, Ordering::Relaxed);
                current_rps.store((count as f64).to_bits(), Ordering::Relaxed);
            }
        });
    }

    fn spawn_blacklist_watcher(&self, path: PathBuf) {
        let blacklist = self.blacklist.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(BLACKLIST_WATCH_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            let mut last_mtime = file_mtime(&path);

            loop {
                interval.tick().await;
                let mtime = file_mtime(&path);
                if mtime == last_mtime {
                    continue;
                }
                last_mtime = mtime;
                match blacklist::load_blacklist(&path) {
                    Ok(new_set) => {
                        let count = new_set.len();
                        *blacklist.write().await = Arc::new(new_set);
                        tracing::info!(count, "blacklist reloaded");
                    }
                    Err(err) => {
                        tracing::warn!("{err}");
                    }
                }
            }
        });
    }
}

pub fn router(state: AppState) -> Router {
    let r = Router::new()
        .route("/api/stats", get(handlers::stats))
        .route("/api/trends", get(handlers::trends))
        .route("/api/clients", get(handlers::clients))
        .route("/api/top100", get(handlers::top100))
        .route(
            "/api/blacklist",
            get(handlers::blacklist_status).post(handlers::add_blacklist),
        )
        .route("/announce", get(handlers::announce))
        .route("/scrape", get(handlers::scrape))
        .route("/healthz", get(handlers::healthz));

    #[cfg(feature = "dashboard")]
    let r = r
        .route("/", get(handlers::index))
        .route("/style.css", get(handlers::style))
        .route("/app.js", get(handlers::app_js));

    r.fallback(handlers::not_found)
        .with_state(state)
}

// ── TrackerPool ────────────────────────────────────────────────────────────

impl TrackerPool {
    fn single(tracker: Tracker) -> Self {
        Self {
            shards: vec![RwLock::new(tracker)],
        }
    }

    fn new(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
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

    fn expire_due(&self, now: Instant) {
        for shard in &self.shards {
            if let Ok(mut tracker) = shard.try_write() {
                tracker.expire_due(now);
            }
        }
    }
}

fn file_mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}
